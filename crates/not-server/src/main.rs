use std::net::SocketAddr;
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use not_protocol::PairingLink;
use not_server::{AppState, PAIRING_CODE_TTL};
use qrcode::QrCode;
use qrcode::render::unicode::Dense1x2;
use tracing_subscriber::EnvFilter;

const USAGE: &str = "\
usage: not-server [command]

commands:
  serve         run the sync server (default)
  pair [URL]    print a one-time code to pair a device, plus a QR code when the
                server's public URL is known (argument or NOT_PUBLIC_URL)
  devices       list paired devices
  revoke <id>   unpair a device
  compact       fold the sync log into one snapshot (also done automatically)

web:
  /             your lists and notes in a browser; sign in with a pairing code
  /s/<token>    a shared list (links are made from the apps: list menu → Share)

environment:
  NOT_DB        database path (default /data/not.db)
  NOT_ADDR      listen address (default 0.0.0.0:8080)
  NOT_PUBLIC_URL  URL devices use to reach this server, for pairing QR codes
  NOT_DB_KEY    encrypts the databases at rest (or NOT_DB_KEY_FILE: a file holding it);
                an existing plain database is encrypted on first start with a key";

#[tokio::main]
async fn main() -> ExitCode {
    let db_path = std::env::var("NOT_DB").unwrap_or_else(|_| "/data/not.db".into());
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let result = match args.as_slice() {
        [] | ["serve"] => serve(&db_path).await,
        ["pair"] => pair(&db_path, std::env::var("NOT_PUBLIC_URL").ok()),
        ["pair", url] => pair(&db_path, Some(url.to_string())),
        ["devices"] => devices(&db_path),
        ["revoke", id] => revoke(&db_path, id),
        ["compact"] => compact(&db_path),
        ["-h" | "--help" | "help"] => {
            println!("{USAGE}");
            Ok(())
        }
        _ => {
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

type CliResult = Result<(), Box<dyn std::error::Error>>;

/// The databases, encrypted at rest when `NOT_DB_KEY` (or a file named by `NOT_DB_KEY_FILE`) gives
/// a key. Losing the key loses the data: keep it with your backups.
fn open_state(db_path: &str) -> Result<AppState, Box<dyn std::error::Error>> {
    let key = match (std::env::var("NOT_DB_KEY"), std::env::var("NOT_DB_KEY_FILE")) {
        (Ok(key), _) => Some(key),
        (_, Ok(file)) => Some(
            std::fs::read_to_string(&file)
                .map_err(|e| format!("NOT_DB_KEY_FILE {file}: {e}"))?
                .trim()
                .to_owned(),
        ),
        _ => None,
    }
    .filter(|k| !k.is_empty());
    Ok(AppState::open_with_key(db_path, key.as_deref())?)
}

async fn serve(db_path: &str) -> CliResult {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();
    let addr: SocketAddr = std::env::var("NOT_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:8080".into())
        .parse()
        .map_err(|e| format!("NOT_ADDR must be host:port: {e}"))?;

    let state = open_state(db_path)?;
    if state.devices()?.is_empty() {
        tracing::info!("no devices paired yet: run `not-server pair` to get a pairing code");
    }
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!(%addr, db = %db_path, "not-server listening");
    axum::serve(listener, not_server::router(state))
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}

fn pair(db_path: &str, public_url: Option<String>) -> CliResult {
    let code = open_state(db_path)?.create_pairing_code()?;
    println!("Pairing code: {}", code.code);
    println!(
        "Valid for {} minutes, for one device. In the app: Settings → Sync → Pair.",
        PAIRING_CODE_TTL.as_secs() / 60
    );
    if let Some(server_url) = public_url.filter(|u| !u.trim().is_empty()) {
        let link = PairingLink {
            server_url: server_url.trim().to_owned(),
            code: code.code,
        };
        // Light modules drawn in the terminal's foreground: right way round on dark terminals.
        let qr = QrCode::new(link.to_string())?
            .render::<Dense1x2>()
            .dark_color(Dense1x2::Light)
            .light_color(Dense1x2::Dark)
            .quiet_zone(true)
            .build();
        println!("\nOr scan with the Android app (Sync → Scan QR code):\n\n{qr}\n{link}");
    }
    Ok(())
}

/// Folds the log into a snapshot now (the server also does it by itself as the log grows).
fn compact(db_path: &str) -> CliResult {
    match open_state(db_path)?.compact()? {
        Some(c) => println!(
            "Compacted {} log rows ({} KB) into one snapshot ({} KB).",
            c.rows,
            c.bytes_before / 1024,
            c.bytes_after / 1024
        ),
        None => println!("Nothing to compact."),
    }
    Ok(())
}

fn devices(db_path: &str) -> CliResult {
    let devices = open_state(db_path)?.devices()?;
    if devices.is_empty() {
        println!("No devices paired. Run `not-server pair`.");
        return Ok(());
    }
    let width = devices
        .iter()
        .map(|d| d.name.chars().count())
        .max()
        .unwrap_or(0)
        .max(4);
    println!("{:<4}  {:<width$}  {:<10}  LAST SEEN", "ID", "NAME", "PAIRED");
    for d in devices {
        let seen = d.last_seen_at.map_or_else(|| "never".into(), ago);
        println!(
            "{:<4}  {:<width$}  {:<10}  {seen}",
            d.id,
            d.name,
            ago(d.created_at)
        );
    }
    Ok(())
}

fn revoke(db_path: &str, id: &str) -> CliResult {
    let id: i64 = id.parse().map_err(|_| format!("not a device id: {id}"))?;
    if !open_state(db_path)?.revoke(id)? {
        return Err(format!("no device {id}").into());
    }
    println!("Revoked device {id}.");
    Ok(())
}

fn ago(millis: i64) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64);
    match (now - millis).max(0) / 1000 {
        s if s < 60 => "just now".into(),
        s if s < 3600 => format!("{}m ago", s / 60),
        s if s < 86_400 => format!("{}h ago", s / 3600),
        s => format!("{}d ago", s / 86_400),
    }
}
