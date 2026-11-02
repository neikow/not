use std::time::Duration;

use futures_util::StreamExt;
use not_core::{Store, SyncClient};
use not_protocol::Poke;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;

const TOKEN: &str = "test-token-test-token-test-token";

async fn serve() -> String {
    let state = not_server::AppState::in_memory(TOKEN).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, not_server::router(state)).await.unwrap() });
    format!("http://{addr}")
}

#[tokio::test]
async fn two_devices_converge_through_server() {
    let base = serve().await;
    let client = SyncClient::new(&base, TOKEN);
    let mac = Store::open_in_memory().unwrap();
    let phone = Store::open_in_memory().unwrap();

    let inbox = mac.create_list("Inbox").unwrap();
    let milk = mac.add_task(&inbox, "milk").unwrap();
    let report = client.sync(&mac).await.unwrap();
    assert!(report.pushed);
    assert_eq!(report.pulled, 0, "own updates are not echoed back");

    client.sync(&phone).await.unwrap();
    assert_eq!(phone.tasks(&inbox).unwrap(), mac.tasks(&inbox).unwrap());

    mac.rename(&milk, "oat milk").unwrap();
    phone.set_completed(&milk, true).unwrap();
    client.sync(&mac).await.unwrap();
    client.sync(&phone).await.unwrap();
    client.sync(&mac).await.unwrap();

    let tasks = mac.tasks(&inbox).unwrap();
    assert_eq!(tasks, phone.tasks(&inbox).unwrap());
    assert_eq!(tasks[0].title, "oat milk");
    assert!(tasks[0].completed_at.is_some());

    // Nothing left to exchange.
    let idle = client.sync(&phone).await.unwrap();
    assert!(!idle.pushed);
    assert_eq!(idle.pulled, 0);
}

#[tokio::test]
async fn rejects_wrong_token() {
    let base = serve().await;
    let store = Store::open_in_memory().unwrap();
    store.create_list("Inbox").unwrap();
    let err = SyncClient::new(&base, "nope").sync(&store).await.unwrap_err();
    assert!(matches!(err, not_core::Error::Status(401)), "{err}");
}

#[tokio::test]
async fn websocket_pokes_on_push() {
    let base = serve().await;
    let mut request = format!("{}/v1/ws", base.replace("http", "ws"))
        .into_client_request()
        .unwrap();
    request
        .headers_mut()
        .insert("authorization", format!("Bearer {TOKEN}").parse().unwrap());
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
    store.create_list("Inbox").unwrap();
    SyncClient::new(&base, TOKEN).sync(&store).await.unwrap();
    assert_eq!(next_poke(&mut socket).await, Poke { seq: 1 });
}
