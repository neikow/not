//! The web companion and shared lists, driven through the router in-process.

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use not_core::{Store, SyncClient, Task};

fn day() -> not_core::date::Date {
    not_core::date::Date::parse("2026-11-16").unwrap()
}

/// Top-level tasks tagged `tag`, newest first.
fn tagged(store: &Store, tag: &str) -> Vec<Task> {
    store
        .all_tasks()
        .unwrap()
        .into_iter()
        .filter(|t| t.parent_id.is_none() && t.tags.iter().any(|x| x == tag))
        .collect()
}
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
    let groceries = phone.create_view("Groceries", "#groceries").unwrap();
    phone.add_task(&groceries, "milk", day()).unwrap();
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
            "text=bread+%3Cb%3E",
            Some(&cookie),
        ),
    )
    .await;
    phone_client.sync(&phone).await.unwrap();
    let tasks = tagged(&phone, "groceries");
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
}

#[tokio::test]
async fn shared_lists_are_read_only_or_editable_and_revocable() {
    let (state, base) = start().await;
    let code = state.create_pairing_code().unwrap();
    let client = SyncClient::from_credentials(&SyncClient::pair(&base, &code.code, "Phone").await.unwrap());
    let phone = Store::open_in_memory().unwrap();
    let groceries = phone.create_view("Groceries", "#groceries").unwrap();
    let secret = phone.create_view("Secret", "#secret").unwrap();
    phone.add_task(&groceries, "milk", day()).unwrap();
    let hidden = phone.add_task(&secret, "hidden", day()).unwrap();
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
    let tasks = tagged(&phone, "groceries");
    assert_eq!(tasks[0].title, "eggs secret");
    assert_eq!(tasks[0].start.as_deref(), Some(today_plus_one_utc().as_str()));
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
    let groceries = phone.create_view("Groceries", "#groceries").unwrap();
    phone.add_task(&groceries, "milk", day()).unwrap();
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
    phone.add_task(&groceries, "eggs <b>", day()).unwrap();
    phone.update_view(&groceries, "Shopping", "#groceries").unwrap();
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
    phone.add_task(&groceries, "after", day()).unwrap();
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
async fn note_pages_show_checklists() {
    let (state, base) = start().await;
    let code = state.create_pairing_code().unwrap();
    let client = SyncClient::from_credentials(&SyncClient::pair(&base, &code.code, "Phone").await.unwrap());
    let phone = Store::open_in_memory().unwrap();
    let note = phone
        .create_note(None, "Trip\n- [ ] passport\n  - [ ] renew <b>")
        .unwrap();
    let body = phone.note_body(&note).unwrap();
    phone
        .checklist_to_tasks(&note, &body, 5, body.len() as u32, Some(""))
        .unwrap();
    phone
        .set_note_body(&note, &body.replace("- [ ] passport", "- [x] passport"))
        .unwrap();
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
}

/// A phone paired with the server, and a signed-in browser's session cookie.
async fn phone_and_browser(state: &AppState, base: &str) -> (SyncClient, Store, String) {
    let code = state.create_pairing_code().unwrap();
    let client = SyncClient::from_credentials(&SyncClient::pair(base, &code.code, "Phone").await.unwrap());
    let phone = Store::open_in_memory().unwrap();
    let code = state.create_pairing_code().unwrap().code;
    let response = not_server::router(state.clone())
        .oneshot(post("/login", &format!("code={code}&name=Browser"), None))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let cookie = response.headers()[header::SET_COOKIE]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned();
    (client, phone, cookie)
}

/// The web pages date things by the browser's offset cookie; without one, UTC.
fn utc_today() -> not_core::date::Date {
    let days = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
        / 86_400;
    not_core::date::Date::from_ymd(1970, 1, 1).unwrap().add_days(days)
}

#[tokio::test]
async fn signed_out_browsers_only_see_the_login_page() {
    let state = AppState::in_memory().unwrap();
    for path in ["/", "/l/today", "/l/inbox", "/notes", "/n/1@2"] {
        let (status, location, _) = send(&state, get(path, None)).await;
        assert_eq!(
            (status, location.as_deref()),
            (StatusCode::SEE_OTHER, Some("/login")),
            "GET {path}"
        );
    }
    for path in ["/l/inbox/add", "/t/1@2/toggle"] {
        let (status, location, _) = send(&state, post(path, "text=x", None)).await;
        assert_eq!(
            (status, location.as_deref()),
            (StatusCode::SEE_OTHER, Some("/login")),
            "POST {path}"
        );
    }
    // A forged or stale cookie is no better than none.
    let (_, location, _) = send(&state, get("/notes", Some("not_session=forged"))).await;
    assert_eq!(location.as_deref(), Some("/login"));
    // Images don't even admit they exist.
    let id = format!("{}.png", "a".repeat(64));
    let (status, _, _) = send(&state, get(&format!("/img/{id}"), None)).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (status, _, html) = send(&state, get("/login", None)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(html.contains(r#"<form method="post" action="/login""#));
    assert!(!html.contains("didn't work"));
    let (_, _, html) = send(&state, get("/login?failed=1", None)).await;
    assert!(html.contains("didn't work"));

    let response = not_server::router(state.clone())
        .oneshot(get("/style.css", None))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert!(
        response.headers()[header::CONTENT_TYPE]
            .to_str()
            .unwrap()
            .starts_with("text/css")
    );
}

#[tokio::test]
async fn logging_out_revokes_the_browser() {
    let (state, base) = start().await;
    let (_, _, cookie) = phone_and_browser(&state, &base).await;
    let (status, location, _) = send(&state, get("/", Some(&cookie))).await;
    assert_eq!(
        (status, location.as_deref()),
        (StatusCode::SEE_OTHER, Some("/l/today"))
    );

    let response = not_server::router(state.clone())
        .oneshot(post("/logout", "", Some(&cookie)))
        .await
        .unwrap();
    assert_eq!(response.headers()[header::LOCATION], "/login");
    assert!(
        response.headers()[header::SET_COOKIE]
            .to_str()
            .unwrap()
            .contains("Max-Age=0"),
        "the cookie is cleared"
    );
    assert!(
        !state.devices().unwrap().iter().any(|d| d.name == "Browser"),
        "and the session can't be replayed"
    );
    let (_, location, _) = send(&state, get("/l/today", Some(&cookie))).await;
    assert_eq!(location.as_deref(), Some("/login"));
}

#[tokio::test]
async fn task_rows_show_dates_tags_progress_and_completed_ones() {
    use not_core::Schedule;

    let (state, base) = start().await;
    let (client, phone, cookie) = phone_and_browser(&state, &base).await;
    let today = utc_today();
    let tags = ["home".to_owned()];
    let soon = Schedule {
        start: Some(today),
        time: Some(7 * 60),
        ..Schedule::default()
    };
    phone
        .add_task_with("#home", "Bins", &soon, 1, &tags, today)
        .unwrap();
    let by = Schedule {
        deadline: Some(today.add_days(3)),
        ..Schedule::default()
    };
    phone.add_task_with("#home", "Tax", &by, 3, &tags, today).unwrap();
    let party = phone.add_task("#home", "Party", today).unwrap();
    phone.set_notes(&party, "\nBring cake\nand candles").unwrap();
    let cake = phone.add_task(&party, "Cake", today).unwrap();
    phone.add_task(&party, "Candles", today).unwrap();
    phone.set_completed(&cake, true, today).unwrap();
    let old = phone.add_task("#home", "Old", today).unwrap();
    phone.set_completed(&old, true, today).unwrap();
    let parked = Schedule {
        someday: true,
        ..Schedule::default()
    };
    phone
        .add_task_with(not_core::SOMEDAY_VIEW, "Paint", &parked, 0, &[], today)
        .unwrap();
    client.sync(&phone).await.unwrap();

    let (status, _, html) = send(&state, get("/l/%23home", Some(&cookie))).await;
    assert_eq!(status, StatusCode::OK, "a tag is a page without saving a view");
    assert!(html.contains("<h1>#home</h1>"), "{html}");
    let row = |title: &str| -> String {
        let at = html
            .find(&format!(r#"<span class="title">{title}</span>"#))
            .expect(title);
        let start = html[..at].rfind("<li").unwrap();
        let end = at + html[at..].find("</li>").unwrap();
        html[start..end].to_owned()
    };
    assert!(row("Bins").contains("Today 7:00 · #home") && row("Bins").contains(r#"class=" p1""#));
    let due = format!(
        "Due {} · #home",
        not_core::due_label(today.add_days(3), None, today)
    );
    assert!(
        row("Tax").contains(&due) && row("Tax").contains(" p3"),
        "{}",
        row("Tax")
    );
    assert!(row("Party").contains("1/2 · #home"), "{}", row("Party"));
    assert!(
        row("Party").contains(r#"<span class="meta">Bring cake</span>"#),
        "the first line of the notes"
    );
    assert!(row("Candles").contains("--depth:1"), "subtasks indented");
    let completed = &html[html.find("<details").expect("a completed section")..];
    assert!(completed.contains(">Old<") && completed.contains(r#"class="done"#));
    assert!(row("Old").contains("Reopen Old"));

    let (_, _, html) = send(&state, get("/l/someday", Some(&cookie))).await;
    assert!(html.contains(">Paint<") && html.contains("Someday"), "{html}");

    let (_, _, html) = send(&state, get("/l/%23empty", Some(&cookie))).await;
    assert!(html.contains("Nothing here."));
    let (status, _, _) = send(&state, get("/l/no-such-view", Some(&cookie))).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn notes_list_by_folder_and_render_as_escaped_markdown() {
    let (state, base) = start().await;
    let (client, phone, cookie) = phone_and_browser(&state, &base).await;
    let png = b"\x89PNG\r\n\x1a\npixels";
    let image = phone.add_image(png).unwrap();
    let trips = phone.create_folder(None, "Trips").unwrap();
    phone.create_folder(None, "Empty folder").unwrap();
    let lisbon = phone
        .create_note(
            Some(&trips),
            &format!(
                "# Lisbon <3\nSome <script>alert(1)</script> text\n\n## Do\n- tram 28\n* pastéis\n- [x] book\n### Eat\nafter list\n{}\nend",
                not_core::markdown::image_markup(&image)
            ),
        )
        .unwrap();
    let pinned = phone
        .create_note(None, "Shopping\nmilk\n- [ ] eggs <b>\n- [x] bread")
        .unwrap();
    phone.set_note_pinned(&pinned, true).unwrap();
    phone.create_note(None, "").unwrap();
    client.sync(&phone).await.unwrap();

    let (status, _, html) = send(&state, get("/notes", Some(&cookie))).await;
    assert_eq!(status, StatusCode::OK);
    let trips_at = html.find("<h2>Trips</h2>").expect("a folder heading");
    assert!(
        html.find("Shopping").unwrap() < trips_at,
        "notes outside folders first"
    );
    assert!(html[trips_at..].contains("Lisbon &lt;3"));
    assert!(html.contains("📌 Shopping"), "pinned");
    assert!(
        html.contains("<span>milk<br>☐ eggs &lt;b&gt;</span>"),
        "previews keep their lines, two of them, escaped: {html}"
    );
    assert!(html.contains("Untitled"), "an empty note");
    assert!(
        !html.contains("Empty folder"),
        "folders without notes are left out"
    );

    let (status, _, html) = send(&state, get(&format!("/n/{lisbon}"), Some(&cookie))).await;
    assert_eq!(status, StatusCode::OK);
    assert!(html.contains("<title>Lisbon &lt;3 · not</title>"));
    let article = &html[html.find("<article").unwrap()..html.find("</article>").unwrap()];
    for part in [
        "<h1>Lisbon &lt;3</h1>",
        "<p>Some &lt;script&gt;alert(1)&lt;/script&gt; text</p>",
        "<br><h2>Do</h2><ul><li>tram 28</li><li>pastéis</li><li>☑ book</li></ul><h3>Eat</h3>",
        &format!(r#"<p>after list</p><img src="/img/{image}" alt="" loading="lazy"><p>end</p>"#),
    ] {
        assert!(article.contains(part), "{part}\nin\n{article}");
    }
    assert!(!article.contains("not-image:"), "no markup shows");

    // The image the note points at, once the phone uploaded it.
    let response = not_server::router(state.clone())
        .oneshot(get(&format!("/img/{image}"), Some(&cookie)))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()[header::CONTENT_TYPE], "image/png");
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(&bytes[..], &png[..]);
    let missing = format!("{}.png", "b".repeat(64));
    let (status, _, _) = send(&state, get(&format!("/img/{missing}"), Some(&cookie))).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _, _) = send(&state, get("/img/..%2Fetc%2Fpasswd", Some(&cookie))).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (status, _, _) = send(&state, get("/n/999@1", Some(&cookie))).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn pages_send_no_referrer_and_refuse_framing() {
    let state = AppState::in_memory().unwrap();
    for path in ["/login", "/s/not-a-token", "/healthz"] {
        let response = not_server::router(state.clone())
            .oneshot(get(path, None))
            .await
            .unwrap();
        let headers = response.headers();
        assert_eq!(headers["referrer-policy"], "no-referrer", "{path}");
        assert_eq!(headers["x-frame-options"], "DENY", "{path}");
        assert!(
            headers["content-security-policy"]
                .to_str()
                .unwrap()
                .contains("frame-ancestors 'none'"),
            "{path}"
        );
    }
}

#[tokio::test]
async fn too_many_wrong_codes_pause_pairing_for_a_while() {
    let state = AppState::in_memory().unwrap();
    let good = state.create_pairing_code().unwrap();
    // The real clock, as web login uses it.
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    for _ in 0..20 {
        assert!(matches!(
            state.pair_throttled("AAAAA-AAAAA", "Guess", now).unwrap(),
            not_server::PairOutcome::Refused
        ));
    }
    assert!(
        matches!(
            state.pair_throttled(&good.code, "Phone", now + 1).unwrap(),
            not_server::PairOutcome::Paused
        ),
        "even a good code waits"
    );
    let (status, location, _) = send(&state, post("/login", &format!("code={}", good.code), None)).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(location.as_deref(), Some("/login?failed=busy"));

    // Ten minutes on, a new window: the good code still works (it hasn't been used).
    assert!(matches!(
        state
            .pair_throttled(&good.code, "Phone", now + 10 * 60_000)
            .unwrap(),
        not_server::PairOutcome::Paired(_)
    ));
}
