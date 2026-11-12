use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use futures_util::StreamExt;
use not_core::{Credentials, Error, INBOX, LiveSync, Store, SyncClient, SyncStatus};
use not_protocol::Poke;
use not_server::AppState;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;

fn today() -> not_core::date::Date {
    not_core::date::Date::parse("2026-11-08").unwrap()
}

struct Server {
    base: String,
    state: AppState,
}

impl Server {
    async fn start() -> Self {
        let state = AppState::in_memory().unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let router = not_server::router(state.clone());
        tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        Self {
            base: format!("http://{addr}"),
            state,
        }
    }

    /// Pairs like a new device would, with a code from `not-server pair`.
    async fn pair(&self, name: &str) -> Credentials {
        let code = self.state.create_pairing_code().unwrap();
        SyncClient::pair(&self.base, &code.code, name).await.unwrap()
    }

    async fn client(&self, name: &str) -> SyncClient {
        SyncClient::from_credentials(&self.pair(name).await)
    }
}

#[tokio::test]
async fn two_devices_converge_through_server() {
    let server = Server::start().await;
    let mac_client = server.client("Mac").await;
    let phone_client = server.client("Phone").await;
    let mac = Store::open_in_memory().unwrap();
    let phone = Store::open_in_memory().unwrap();

    let milk = mac.add_task(INBOX, "milk").unwrap();
    let report = mac_client.sync(&mac).await.unwrap();
    assert!(report.pushed);
    assert_eq!(report.pulled, 0, "own updates are not echoed back");

    phone_client.sync(&phone).await.unwrap();
    assert_eq!(phone.tasks(INBOX).unwrap(), mac.tasks(INBOX).unwrap());

    mac.rename(&milk, "oat milk").unwrap();
    phone.set_completed(&milk, true, today()).unwrap();
    mac_client.sync(&mac).await.unwrap();
    phone_client.sync(&phone).await.unwrap();
    mac_client.sync(&mac).await.unwrap();

    let tasks = mac.tasks(INBOX).unwrap();
    assert_eq!(tasks, phone.tasks(INBOX).unwrap());
    assert_eq!(tasks[0].title, "oat milk");
    assert!(tasks[0].completed_at.is_some());

    // Nothing left to exchange.
    let idle = phone_client.sync(&phone).await.unwrap();
    assert!(!idle.pushed);
    assert_eq!(idle.pulled, 0);
}

#[tokio::test]
async fn pairing_codes_are_single_use() {
    let server = Server::start().await;
    let code = server.state.create_pairing_code().unwrap();
    assert_eq!(code.code.len(), 11, "XXXXX-XXXXX");

    let mac = SyncClient::pair(&server.base, &code.code.to_lowercase(), "  Mac  ")
        .await
        .unwrap();
    assert_eq!(mac.device_name, "Mac");
    assert_eq!(mac.server_url, server.base);

    let reused = SyncClient::pair(&server.base, &code.code, "Phone")
        .await
        .unwrap_err();
    assert!(matches!(reused, Error::Invalid(_)), "{reused}");
    let wrong = SyncClient::pair(&server.base, "00000-00000", "Phone")
        .await
        .unwrap_err();
    assert!(matches!(wrong, Error::Invalid(_)), "{wrong}");
    let bad_url = SyncClient::pair("example.com", &code.code, "Phone")
        .await
        .unwrap_err();
    assert!(matches!(bad_url, Error::Invalid(_)), "{bad_url}");

    // A paired device can hand out codes too.
    let code = SyncClient::from_credentials(&mac)
        .create_pairing_code()
        .await
        .unwrap();
    let phone = SyncClient::pair(&server.base, &code.code, "Phone").await.unwrap();
    assert_ne!(phone.device_id, mac.device_id);
    assert_ne!(phone.token, mac.token);
}

#[tokio::test]
async fn rejects_unknown_token() {
    let server = Server::start().await;
    let store = Store::open_in_memory().unwrap();
    store.add_task(INBOX, "x").unwrap();
    let err = SyncClient::new(&server.base, "nope")
        .sync(&store)
        .await
        .unwrap_err();
    assert!(matches!(err, Error::Unauthorized), "{err}");
}

#[tokio::test]
async fn revoked_devices_lose_access() {
    let server = Server::start().await;
    let mac = server.pair("Mac").await;
    let phone = server.client("Phone").await;

    let names: Vec<String> = phone
        .devices()
        .await
        .unwrap()
        .into_iter()
        .map(|d| d.name)
        .collect();
    assert_eq!(names, ["Mac", "Phone"]);

    phone.revoke(mac.device_id).await.unwrap();
    let store = Store::open_in_memory().unwrap();
    store.add_task(INBOX, "x").unwrap();
    let err = SyncClient::from_credentials(&mac).sync(&store).await.unwrap_err();
    assert!(matches!(err, Error::Unauthorized), "{err}");
    assert_eq!(phone.devices().await.unwrap().len(), 1);
    assert!(matches!(
        phone.revoke(mac.device_id).await,
        Err(Error::NotFound(_))
    ));
}

#[tokio::test]
async fn websocket_pokes_on_push() {
    let server = Server::start().await;
    let credentials = server.pair("Mac").await;
    let mut request = format!("{}/v1/ws", server.base.replace("http", "ws"))
        .into_client_request()
        .unwrap();
    request.headers_mut().insert(
        "authorization",
        format!("Bearer {}", credentials.token).parse().unwrap(),
    );
    let (mut socket, _) = tokio_tungstenite::connect_async(request).await.unwrap();

    let next_poke = async |socket: &mut tokio_tungstenite::WebSocketStream<_>| -> Poke {
        let msg = tokio::time::timeout(Duration::from_secs(2), socket.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        serde_json::from_str(msg.to_text().unwrap()).unwrap()
    };
    assert_eq!(next_poke(&mut socket).await, Poke { seq: 0 }, "head on connect");

    let store = Store::open_in_memory().unwrap();
    store.add_task(INBOX, "x").unwrap();
    server.client("Phone").await.sync(&store).await.unwrap();
    assert_eq!(next_poke(&mut socket).await, Poke { seq: 1 });
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn live_sync_propagates_both_ways() {
    let server = Server::start().await;
    let runtime = tokio::runtime::Handle::current();
    let mac = Arc::new(Store::open_in_memory().unwrap());
    let phone = Arc::new(Store::open_in_memory().unwrap());
    let _mac_sync = LiveSync::start(&runtime, mac.clone(), server.client("Mac").await, |_| {});
    let _phone_sync = LiveSync::start(&runtime, phone.clone(), server.client("Phone").await, |_| {});

    mac.add_task(INBOX, "milk").unwrap();
    wait_until(|| phone.tasks(INBOX).unwrap().len() == 1).await;

    let milk = phone.tasks(INBOX).unwrap()[0].id.clone();
    phone.set_completed(&milk, true, today()).unwrap();
    wait_until(|| mac.tasks(INBOX).unwrap()[0].completed_at.is_some()).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn live_sync_reports_revocation() {
    let server = Server::start().await;
    let runtime = tokio::runtime::Handle::current();
    let credentials = server.pair("Mac").await;
    let store = Arc::new(Store::open_in_memory().unwrap());
    let last = Arc::new(Mutex::new(None));
    let live = LiveSync::start(
        &runtime,
        store.clone(),
        SyncClient::from_credentials(&credentials),
        {
            let last = last.clone();
            move |status| *last.lock().unwrap() = Some(status)
        },
    );
    wait_until(|| *last.lock().unwrap() == Some(SyncStatus::Synced)).await;

    assert!(server.state.revoke(credentials.device_id).unwrap());
    live.sync_now();
    wait_until(|| *last.lock().unwrap() == Some(SyncStatus::Unauthorized)).await;
}

async fn wait_until(condition: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !condition() {
        assert!(Instant::now() < deadline, "timed out waiting for sync");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn undo_works_while_live_sync_runs() {
    let server = Server::start().await;
    let runtime = tokio::runtime::Handle::current();
    let phone = Arc::new(Store::open_in_memory().unwrap());
    let mac = Arc::new(Store::open_in_memory().unwrap());
    let _phone_sync = LiveSync::start(&runtime, phone.clone(), server.client("Phone").await, |_| {});
    let _mac_sync = LiveSync::start(&runtime, mac.clone(), server.client("Mac").await, |_| {});

    let id = phone.add_task(INBOX, "probe").unwrap();
    wait_until(|| mac.tasks(INBOX).unwrap().len() == 1).await;
    phone.delete(&id).unwrap();
    wait_until(|| mac.tasks(INBOX).unwrap().is_empty()).await;

    assert!(phone.undo().unwrap());
    assert_eq!(phone.tasks(INBOX).unwrap().len(), 1, "restored on the phone");
    wait_until(|| mac.tasks(INBOX).unwrap().len() == 1).await;
}

#[tokio::test]
async fn note_images_travel_beside_the_doc() {
    let server = Server::start().await;
    let mac_client = server.client("Mac").await;
    let phone_client = server.client("Phone").await;
    let mac = Store::open_in_memory().unwrap();
    let phone = Store::open_in_memory().unwrap();

    let png = b"\x89PNG\r\n\x1a\nnot really pixels";
    let id = mac.add_image(png).unwrap();
    assert_eq!(mac.add_image(png).unwrap(), id, "same picture, one blob");
    let note = mac
        .create_note(
            None,
            &format!("Trip\n{}\nview", not_core::markdown::image_markup(&id)),
        )
        .unwrap();
    let report = mac_client.sync(&mac).await.unwrap();
    assert_eq!((report.images_up, report.image_error), (1, None));
    assert_eq!(mac_client.sync(&mac).await.unwrap().images_up, 0, "uploaded once");

    assert_eq!(phone.image(&id).unwrap(), None);
    let report = phone_client.sync(&phone).await.unwrap();
    assert_eq!(report.images_down, 1);
    assert_eq!(phone.image(&id).unwrap().as_deref(), Some(&png[..]));
    assert!(phone.note_body(&note).unwrap().contains(&id));

    // The server checks an upload is what its name says.
    let wrong = format!("{}.png", "0".repeat(64));
    use tower::ServiceExt;
    let request = axum::http::Request::put(format!("/v1/blobs/{wrong}"))
        .header(
            "authorization",
            format!("Bearer {}", server.pair("Curl").await.token),
        )
        .body(axum::body::Body::from(png.to_vec()))
        .unwrap();
    let response = not_server::router(server.state.clone())
        .oneshot(request)
        .await
        .unwrap();
    assert_eq!(response.status(), 422);
}
