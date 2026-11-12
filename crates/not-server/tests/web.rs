//! The web companion and shared lists, driven through the router in-process.

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use not_core::{INBOX, Store, SyncClient};
use not_server::AppState;
use tower::ServiceExt;

fn today() -> not_core::date::Date {
    not_core::date::Date::parse("2026-11-08").unwrap()
}

async fn start() -> (AppState, String) {
    let state = AppState::in_memory().unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let router = not_server::router(state.clone());
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    (state, format!("http://{addr}"))
}

async fn send(state: &AppState, request: Request<Body>) -> (StatusCode, Option<String>, String) {
    let response = not_server::router(state.clone()).oneshot(request).await.unwrap();
    let status = response.status();
    let location = response
        .headers()
        .get(header::LOCATION)
        .map(|v| v.to_str().unwrap().to_owned());
    let cookie = response
        .headers()
        .get(header::SET_COOKIE)
        .map(|v| v.to_str().unwrap().to_owned());
    let body = String::from_utf8(response.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap();
    (status, location.or(cookie), body)
}

fn get(path: &str, cookie: Option<&str>) -> Request<Body> {
    let mut r = Request::get(path);
    if let Some(c) = cookie {
        r = r.header(header::COOKIE, c);
    }
    r.body(Body::empty()).unwrap()
}

fn post(path: &str, form: &str, cookie: Option<&str>) -> Request<Body> {
    let mut r = Request::post(path).header(header::CONTENT_TYPE, "application/x-www-form-urlencoded");
    if let Some(c) = cookie {
        r = r.header(header::COOKIE, c);
    }
    r.body(Body::from(form.to_owned())).unwrap()
}

#[tokio::test]
async fn browser_signs_in_with_a_code_and_edits_flow_back() {
    let (state, base) = start().await;
    let code = state.create_pairing_code().unwrap();
    let phone_client =
        SyncClient::from_credentials(&SyncClient::pair(&base, &code.code, "Phone").await.unwrap());
    let phone = Store::open_in_memory().unwrap();
    let groceries = phone.create_list("Groceries").unwrap();
    phone.add_task(&groceries, "milk").unwrap();
    phone_client.sync(&phone).await.unwrap();

    // Signed out: to the login page.
    let (status, location, _) = send(&state, get("/", None)).await;
    assert_eq!(
        (status, location.as_deref()),
        (StatusCode::SEE_OTHER, Some("/login"))
    );

    let (_, location, _) = send(&state, post("/login", "code=nope", None)).await;
    assert_eq!(location.as_deref(), Some("/login?failed=1"));
    let code = state.create_pairing_code().unwrap().code;
    let response = not_server::router(state.clone())
        .oneshot(post("/login", &format!("code={code}&name=Firefox"), None))
        .await
        .unwrap();
    let cookie = response
        .headers()
        .get(header::SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned();
    assert!(
        state.devices().unwrap().iter().any(|d| d.name == "Firefox"),
        "a browser is a device"
    );

    let (status, _, html) = send(&state, get(&format!("/l/{groceries}"), Some(&cookie))).await;
    assert_eq!(status, StatusCode::OK);
    assert!(html.contains("milk") && html.contains("Groceries"));

    // Adding and ticking from the browser reach the phone.
    send(
        &state,
        post(
            &format!("/l/{groceries}/add"),
            "text=bread+%3Cb%3E&from=",
            Some(&cookie),
        ),
    )
    .await;
    phone_client.sync(&phone).await.unwrap();
    let tasks = phone.tasks(&groceries).unwrap();
    assert_eq!(tasks[0].title, "bread <b>");
    let (_, _, html) = send(&state, get(&format!("/l/{groceries}"), Some(&cookie))).await;
    assert!(html.contains("bread &lt;b&gt;"), "escaped");

    send(
        &state,
        post(&format!("/t/{}/toggle", tasks[1].id), "back=/", Some(&cookie)),
    )
    .await;
    phone_client.sync(&phone).await.unwrap();
    assert!(phone.task_by_id(&tasks[1].id).unwrap().completed_at.is_some());
    let _ = INBOX;
}

#[tokio::test]
async fn shared_lists_are_read_only_or_editable_and_revocable() {
    let (state, base) = start().await;
    let code = state.create_pairing_code().unwrap();
    let client = SyncClient::from_credentials(&SyncClient::pair(&base, &code.code, "Phone").await.unwrap());
    let phone = Store::open_in_memory().unwrap();
    let groceries = phone.create_list("Groceries").unwrap();
    let secret = phone.create_list("Secret").unwrap();
    phone.add_task(&groceries, "milk").unwrap();
    let hidden = phone.add_task(&secret, "hidden").unwrap();
    client.sync(&phone).await.unwrap();

    let view = client.create_share(&groceries, false).await.unwrap();
    let edit = client.create_share(&groceries, true).await.unwrap();
    assert_eq!(client.shares().await.unwrap().len(), 2);

    let (status, _, html) = send(&state, get(&format!("/s/{}", view.token), None)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        html.contains("milk") && !html.contains(">hidden<"),
        "only the shared list"
    );
    assert!(!html.contains("<form"), "read only: no forms");
    let (status, _, _) = send(&state, post(&format!("/s/{}/add", view.token), "text=x", None)).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // Edit link: add (a #tag can't move it to another list) and tick.
    send(
        &state,
        post(
            &format!("/s/{}/add", edit.token),
            "text=eggs+%23secret+tomorrow",
            None,
        ),
    )
    .await;
    client.sync(&phone).await.unwrap();
    let tasks = phone.tasks(&groceries).unwrap();
    assert_eq!(tasks[0].title, "eggs secret");
    assert_eq!(tasks[0].due.as_deref(), Some(today_plus_one_utc().as_str()));
    send(
        &state,
        post(&format!("/s/{}/t/{}/toggle", edit.token, tasks[1].id), "", None),
    )
    .await;
    // Not a task of the shared list.
    let (status, _, _) = send(
        &state,
        post(&format!("/s/{}/t/{hidden}/toggle", edit.token), "", None),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    client.sync(&phone).await.unwrap();
    assert!(phone.task_by_id(&tasks[1].id).unwrap().completed_at.is_some());
    assert!(phone.task_by_id(&hidden).unwrap().completed_at.is_none());

    client.revoke_share(edit.id).await.unwrap();
    let (status, _, _) = send(&state, get(&format!("/s/{}", edit.token), None)).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let _ = today();
}

fn today_plus_one_utc() -> String {
    let days = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
        / 86_400;
    not_core::date::Date::from_ymd(1970, 1, 1)
        .unwrap()
        .add_days(days + 1)
        .to_string()
}

#[tokio::test]
async fn shared_lists_update_live_and_close_when_revoked() {
    use futures_util::StreamExt;
    use tokio_tungstenite::tungstenite::Message;

    let (state, base) = start().await;
    let code = state.create_pairing_code().unwrap();
    let client = SyncClient::from_credentials(&SyncClient::pair(&base, &code.code, "Phone").await.unwrap());
    let phone = Store::open_in_memory().unwrap();
    let groceries = phone.create_list("Groceries").unwrap();
    phone.add_task(&groceries, "milk").unwrap();
    client.sync(&phone).await.unwrap();
    let share = client.create_share(&groceries, true).await.unwrap();

    let (status, _, html) = send(&state, get(&format!("/s/{}", share.token), None)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(html.contains(r#"<div id="tasks">"#) && html.contains("/live"));

    let url = format!("{}/s/{}/live", base.replace("http", "ws"), share.token);
    let (mut socket, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
    let next = async |socket: &mut tokio_tungstenite::WebSocketStream<_>| -> Message {
        tokio::time::timeout(std::time::Duration::from_secs(5), socket.next())
            .await
            .expect("a message in time")
            .unwrap()
            .unwrap()
    };
    let view = |m: Message| -> serde_json::Value { serde_json::from_str(m.to_text().unwrap()).unwrap() };

    // The current state on connect.
    let first = view(next(&mut socket).await);
    assert_eq!(first["title"], "Groceries");
    assert!(first["html"].as_str().unwrap().contains("milk"));

    // A change from the phone arrives without asking.
    phone.add_task(&groceries, "eggs <b>").unwrap();
    phone.rename(&groceries, "Shopping").unwrap();
    client.sync(&phone).await.unwrap();
    let update = view(next(&mut socket).await);
    assert_eq!(update["title"], "Shopping");
    assert!(update["html"].as_str().unwrap().contains("eggs &lt;b&gt;"));

    // So does another guest's edit.
    send(
        &state,
        post(&format!("/s/{}/add", share.token), "text=bread", None),
    )
    .await;
    let update = view(next(&mut socket).await);
    assert!(update["html"].as_str().unwrap().contains("bread"));

    // Revoking cuts the open page off at the next change.
    client.revoke_share(share.id).await.unwrap();
    phone.add_task(&groceries, "after").unwrap();
    client.sync(&phone).await.unwrap();
    match next(&mut socket).await {
        Message::Close(Some(frame)) => assert_eq!(u16::from(frame.code), 4404),
        other => panic!("expected a close, got {other:?}"),
    }
    assert!(
        tokio_tungstenite::connect_async(&url).await.is_err(),
        "a revoked link can't reconnect"
    );
}

#[tokio::test]
async fn note_pages_show_linked_checklists_as_checklists() {
    let (state, base) = start().await;
    let code = state.create_pairing_code().unwrap();
    let client = SyncClient::from_credentials(&SyncClient::pair(&base, &code.code, "Phone").await.unwrap());
    let phone = Store::open_in_memory().unwrap();
    let note = phone
        .create_note(None, "Trip\n- [ ] passport\n  - [ ] renew <b>")
        .unwrap();
    let body = phone.note_body(&note).unwrap();
    let today = not_core::date::Date::parse("2026-11-12").unwrap();
    phone
        .checklist_to_tasks(&note, &body, 5, body.len() as u32, None, "", today)
        .unwrap();
    let list = phone.lists().unwrap().into_iter().find(|l| l.title == "Trip").unwrap();
    let passport = phone.tasks(&list.id).unwrap().into_iter().find(|t| t.title == "passport").unwrap();
    phone.set_completed(&passport.id, true, today).unwrap();
    client.sync(&phone).await.unwrap();

    let code = state.create_pairing_code().unwrap().code;
    let response = not_server::router(state.clone())
        .oneshot(post("/login", &format!("code={code}"), None))
        .await
        .unwrap();
    let cookie = response
        .headers()
        .get(header::SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned();
    let (status, _, html) = send(&state, get(&format!("/n/{note}"), Some(&cookie))).await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        html.contains("☑ passport") && html.contains("☐ renew &lt;b&gt;"),
        "{html}"
    );
    assert!(!html.contains("not-tasks:"), "no markup shows");
    assert!(!html.contains("task:"), "no link shows");
}
