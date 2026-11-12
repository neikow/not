//! The light web companion and shared lists, served from a replica the server keeps of the
//! workspace (it applies the log to its own `not-core` store; see ADR 0009). Edits made here are
//! appended to the log like a device's, so the apps pick them up.
//!
//! - `/` and below: the owner's pages, signed in by pairing the browser with a one-time code (it
//!   then shows among the devices and can be revoked like one).
//! - `/s/<token>`: one list, for anyone with the link; the owner chose read-only or edit.
//!
//! Plain HTML forms; scripts only report the browser's UTC offset, so dates typed in
//! ("tomorrow") mean the visitor's tomorrow, and keep shared lists live over a WebSocket
//! (`/s/<token>/live`), so a guest sees ticks and adds from the apps and other guests as they
//! happen.

use std::fmt::Write as _;

use axum::Router;
use axum::extract::ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Form, Path as UrlPath, Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use not_core::date::Date;
use not_core::{INBOX, OutlineRow, Task};
use not_protocol::ShareInfo;
use serde::Deserialize;
use tokio::sync::broadcast;

use crate::AppState;

const SESSION_COOKIE: &str = "not_session";
const OFFSET_COOKIE: &str = "not_tzo";
const TODAY: &str = "today";
const UPCOMING: &str = "upcoming";

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/", get(home))
        .route("/login", get(login_page).post(login))
        .route("/logout", post(logout))
        .route("/l/{list}", get(list_page))
        .route("/l/{list}/add", post(add_task))
        .route("/t/{task}/toggle", post(toggle_task))
        .route("/notes", get(notes_page))
        .route("/n/{note}", get(note_page))
        .route("/img/{id}", get(image))
        .route("/s/{token}", get(shared_page))
        .route("/s/{token}/live", get(shared_live))
        .route("/s/{token}/add", post(shared_add))
        .route("/s/{token}/t/{task}/toggle", post(shared_toggle))
        .route("/style.css", get(stylesheet))
}

// ---- owner pages ---------------------------------------------------------------------------------

async fn home(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if signed_in(&state, &headers).is_none() {
        return Redirect::to("/login").into_response();
    }
    Redirect::to(&format!("/l/{TODAY}")).into_response()
}

async fn login_page(Query(q): Query<LoginQuery>) -> Html<String> {
    let error = if q.failed.is_some() {
        r#"<p class="error">That code didn't work: it may have expired or been used. Make a new one.</p>"#
    } else {
        ""
    };
    page(
        "Sign in",
        None,
        &format!(
            r#"<main class="narrow">
<h1>not</h1>
<p class="muted">Sign this browser in with a pairing code: in the app, Settings → Sync → Add device, or <code>not-server pair</code> on the server.</p>
{error}
<form method="post" action="/login" class="stack">
  <input name="code" placeholder="7KQ2M-X9CHD" autocomplete="one-time-code" autofocus required>
  <input name="name" placeholder="Browser name (optional)">
  <button>Sign in</button>
</form>
</main>"#
        ),
    )
}

#[derive(Deserialize)]
struct LoginQuery {
    failed: Option<String>,
}

#[derive(Deserialize)]
struct LoginForm {
    code: String,
    #[serde(default)]
    name: String,
}

async fn login(State(state): State<AppState>, headers: HeaderMap, Form(form): Form<LoginForm>) -> Response {
    let name = if form.name.trim().is_empty() {
        "Web browser"
    } else {
        form.name.trim()
    };
    match state.pair(&form.code, name) {
        Ok(Some(paired)) => {
            tracing::info!(id = paired.device.id, name = %paired.device.name, "browser paired");
            let cookie = format!(
                "{SESSION_COOKIE}={}; Path=/; HttpOnly; SameSite=Lax; Max-Age=31536000{}",
                paired.token,
                if is_https(&headers) { "; Secure" } else { "" }
            );
            let mut response = Redirect::to("/").into_response();
            response
                .headers_mut()
                .insert(header::SET_COOKIE, HeaderValue::from_str(&cookie).unwrap());
            response
        }
        Ok(None) => Redirect::to("/login?failed=1").into_response(),
        Err(e) => server_error(e),
    }
}

/// Unpairs this browser; it disappears from the devices list.
async fn logout(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Some(device) = signed_in(&state, &headers) {
        let _ = state.revoke(device.0);
    }
    let mut response = Redirect::to("/login").into_response();
    response.headers_mut().insert(
        header::SET_COOKIE,
        HeaderValue::from_static("not_session=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0"),
    );
    response
}

async fn list_page(
    State(state): State<AppState>,
    headers: HeaderMap,
    UrlPath(list): UrlPath<String>,
) -> Response {
    if signed_in(&state, &headers).is_none() {
        return Redirect::to("/login").into_response();
    }
    let Some(store) = state.replica(true) else {
        return unavailable();
    };
    let today = today(&headers);
    let lists = match store.lists() {
        Ok(lists) => lists,
        Err(e) => return core_error(e),
    };
    let title_of = |id: &str| {
        lists
            .iter()
            .find(|l| l.id == id)
            .map_or("Inbox".to_owned(), |l| l.title.clone())
    };
    let (title, rows, completed, by_date) = match list.as_str() {
        TODAY => match store.due_tasks(today) {
            Ok(tasks) => ("Today".to_owned(), flat(tasks), Vec::new(), true),
            Err(e) => return core_error(e),
        },
        UPCOMING => match store.upcoming_tasks(today) {
            Ok(tasks) => ("Upcoming".to_owned(), flat(tasks), Vec::new(), true),
            Err(e) => return core_error(e),
        },
        id if lists.iter().any(|l| l.id == id) => match store.outline(id) {
            Ok(outline) => (title_of(id), outline.open, outline.completed, false),
            Err(e) => return core_error(e),
        },
        _ => return not_found(),
    };

    let mut nav = String::new();
    for (id, label) in [(TODAY, "Today"), (UPCOMING, "Upcoming")]
        .into_iter()
        .map(|(a, b)| (a.to_owned(), b.to_owned()))
        .chain(lists.iter().map(|l| (l.id.clone(), l.title.clone())))
    {
        let current = if id == list { r#" aria-current="page""# } else { "" };
        let _ = write!(nav, r#"<a href="/l/{}"{current}>{}</a>"#, esc(&id), esc(&label));
    }
    nav.push_str(r#"<a href="/notes">Notes</a>"#);

    let here = format!("/l/{}", esc(&list));
    let mut body = format!(r#"<main><h1>{}</h1>"#, esc(&title));
    let target = if by_date { INBOX.to_owned() } else { list.clone() };
    let placeholder = match list.as_str() {
        TODAY => "New task for today",
        UPCOMING => "New task for tomorrow",
        _ => "New task",
    };
    let _ = write!(
        body,
        r#"<form method="post" action="/l/{}/add" class="add"><input name="text" placeholder="{placeholder} — try “call mum fri 6pm !1”" required><input type="hidden" name="from" value="{}"><button aria-label="Add">+</button></form>"#,
        esc(&target),
        esc(&list)
    );
    body.push_str(&task_rows(
        today,
        &rows,
        |t| format!("/t/{}/toggle", esc(&t.id)),
        &here,
        by_date.then_some(&title_of as &dyn Fn(&str) -> String),
        true,
    ));
    if rows.is_empty() {
        body.push_str(r#"<p class="empty">Nothing here.</p>"#);
    }
    if !completed.is_empty() {
        let _ = write!(
            body,
            r#"<details><summary>Completed {}</summary>"#,
            completed.iter().filter(|r| r.depth == 0).count()
        );
        body.push_str(&task_rows(
            today,
            &completed,
            |t| format!("/t/{}/toggle", esc(&t.id)),
            &here,
            None,
            true,
        ));
        body.push_str("</details>");
    }
    body.push_str(r#"<form method="post" action="/logout" class="signout"><button class="quiet">Sign out this browser</button></form></main>"#);
    page(&title, Some(&nav), &body).into_response()
}

#[derive(Deserialize)]
struct AddForm {
    text: String,
    /// The page to go back to (a list, or Today / Upcoming).
    #[serde(default)]
    from: String,
}

async fn add_task(
    State(state): State<AppState>,
    headers: HeaderMap,
    UrlPath(list): UrlPath<String>,
    Form(form): Form<AddForm>,
) -> Response {
    if signed_in(&state, &headers).is_none() {
        return Redirect::to("/login").into_response();
    }
    let Some(store) = state.replica(true) else {
        return unavailable();
    };
    let today = today(&headers);
    let due = match form.from.as_str() {
        TODAY => Some(today),
        UPCOMING => Some(today.add_days(1)),
        _ => None,
    };
    if !form.text.trim().is_empty()
        && let Err(e) = store.quick_add(&list, &form.text, today, &[], due)
    {
        return core_error(e);
    }
    state.publish_replica();
    let back = if form.from.is_empty() { list } else { form.from };
    Redirect::to(&format!("/l/{}", urlencode(&back))).into_response()
}

#[derive(Deserialize)]
struct BackForm {
    #[serde(default)]
    back: String,
}

async fn toggle_task(
    State(state): State<AppState>,
    headers: HeaderMap,
    UrlPath(task): UrlPath<String>,
    Form(form): Form<BackForm>,
) -> Response {
    if signed_in(&state, &headers).is_none() {
        return Redirect::to("/login").into_response();
    }
    let Some(store) = state.replica(true) else {
        return unavailable();
    };
    if let Err(e) = toggle(&store, &task, today(&headers)) {
        return core_error(e);
    }
    state.publish_replica();
    Redirect::to(safe_back(&form.back, "/")).into_response()
}

async fn notes_page(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if signed_in(&state, &headers).is_none() {
        return Redirect::to("/login").into_response();
    }
    let Some(store) = state.replica(true) else {
        return unavailable();
    };
    let (folders, notes) = match (store.folders(), store.notes(None)) {
        (Ok(f), Ok(n)) => (f, n),
        (Err(e), _) | (_, Err(e)) => return core_error(e),
    };
    let mut body = String::from("<main><h1>Notes</h1>");
    let mut groups: Vec<(Option<String>, String)> = vec![(None, String::new())];
    groups.extend(folders.iter().map(|f| (Some(f.id.clone()), f.title.clone())));
    for (folder, title) in groups {
        let inside: Vec<_> = notes.iter().filter(|n| n.folder_id == folder).collect();
        if inside.is_empty() {
            continue;
        }
        if !title.is_empty() {
            let _ = write!(body, "<h2>{}</h2>", esc(&title));
        }
        body.push_str(r#"<ul class="notes">"#);
        for n in inside {
            let _ = write!(
                body,
                r#"<li><a href="/n/{}"><strong>{}{}</strong><span>{}</span></a></li>"#,
                esc(&n.id),
                if n.pinned { "📌 " } else { "" },
                esc(if n.title.is_empty() { "Untitled" } else { &n.title }),
                esc(&n.preview)
            );
        }
        body.push_str("</ul>");
    }
    body.push_str("</main>");
    page(
        "Notes",
        Some(r#"<a href="/">Tasks</a><a href="/notes" aria-current="page">Notes</a>"#),
        &body,
    )
    .into_response()
}

async fn note_page(
    State(state): State<AppState>,
    headers: HeaderMap,
    UrlPath(note): UrlPath<String>,
) -> Response {
    if signed_in(&state, &headers).is_none() {
        return Redirect::to("/login").into_response();
    }
    let Some(store) = state.replica(true) else {
        return unavailable();
    };
    let Ok(text) = store.note_body(&note) else {
        return not_found();
    };
    let title = text
        .lines()
        .next()
        .unwrap_or("Note")
        .trim_start_matches('#')
        .trim()
        .to_owned();
    let body = format!(
        r#"<main><article class="note">{}</article></main>"#,
        markdown_html(&text)
    );
    page(
        &title,
        Some(r#"<a href="/">Tasks</a><a href="/notes">Notes</a>"#),
        &body,
    )
    .into_response()
}

/// A note's image, for the signed-in owner only.
async fn image(State(state): State<AppState>, headers: HeaderMap, UrlPath(id): UrlPath<String>) -> Response {
    if signed_in(&state, &headers).is_none() {
        return StatusCode::NOT_FOUND.into_response();
    }
    crate::image_response(&state, &id).unwrap_or_else(server_error)
}

// ---- shared lists --------------------------------------------------------------------------------

async fn shared_page(
    State(state): State<AppState>,
    headers: HeaderMap,
    UrlPath(token): UrlPath<String>,
) -> Response {
    let Some(share) = state.share_by_token(&token) else {
        return not_found();
    };
    let view = match shared_view(&state, &share, &token, today(&headers)) {
        Ok(view) => view,
        Err(SharedError::Gone) => return not_found(),
        Err(SharedError::Unavailable) => return unavailable(),
        Err(SharedError::Core(e)) => return core_error(e),
    };
    let here = format!("/s/{}", esc(&token));
    let mut body = format!(r#"<main><h1>{}</h1>"#, esc(&view.title));
    if share.can_edit {
        let _ = write!(
            body,
            r#"<form method="post" action="{here}/add" class="add"><input name="text" placeholder="Add to {}" required><button aria-label="Add">+</button></form>"#,
            esc(&view.title)
        );
    } else {
        body.push_str(r#"<p class="muted">Shared with you to read.</p>"#);
    }
    let _ = write!(
        body,
        r#"<div id="tasks">{}</div></main><script>{LIVE_SCRIPT}</script>"#,
        view.tasks
    );
    page(&view.title, None, &body).into_response()
}

/// A shared list's title and task rows: the part of its page that changes, and what `/live`
/// sends.
struct SharedView {
    title: String,
    tasks: String,
}

enum SharedError {
    /// The link was revoked or its list deleted.
    Gone,
    Unavailable,
    Core(not_core::Error),
}

fn shared_view(
    state: &AppState,
    share: &ShareInfo,
    token: &str,
    today: Date,
) -> Result<SharedView, SharedError> {
    let store = state.replica(true).ok_or(SharedError::Unavailable)?;
    let lists = store.lists().map_err(SharedError::Core)?;
    let title = lists
        .into_iter()
        .find(|l| l.id == share.list_id)
        .ok_or(SharedError::Gone)?
        .title;
    let outline = store.outline(&share.list_id).map_err(SharedError::Core)?;
    let here = format!("/s/{}", esc(token));
    let toggle = |t: &Task| format!("{here}/t/{}/toggle", esc(&t.id));
    let mut tasks = task_rows(today, &outline.open, toggle, &here, None, share.can_edit);
    if outline.open.is_empty() {
        tasks.push_str(r#"<p class="empty">All done.</p>"#);
    }
    if !outline.completed.is_empty() {
        let _ = write!(
            tasks,
            r#"<details><summary>Done {}</summary>"#,
            outline.completed.iter().filter(|r| r.depth == 0).count()
        );
        tasks.push_str(&task_rows(
            today,
            &outline.completed,
            toggle,
            &here,
            None,
            share.can_edit,
        ));
        tasks.push_str("</details>");
    }
    Ok(SharedView { title, tasks })
}

/// Live updates for a shared list's page: on connect and after every change to the log, the
/// list's title and rows as `{"title", "html"}` (only when they changed). Closes with
/// [`GONE_CLOSE_CODE`] once the link is revoked or the list deleted.
async fn shared_live(
    State(state): State<AppState>,
    headers: HeaderMap,
    UrlPath(token): UrlPath<String>,
    upgrade: WebSocketUpgrade,
) -> Response {
    if state.share_by_token(&token).is_none() {
        return not_found();
    }
    let offset = utc_offset_min(&headers);
    upgrade.on_upgrade(move |socket| live_updates(socket, state, token, offset))
}

/// A private-use close code (4000–4999) the page answers by reloading into "Not here".
const GONE_CLOSE_CODE: u16 = 4404;

async fn live_updates(mut socket: WebSocket, state: AppState, token: String, offset_min: i64) {
    let mut pokes = state.pokes.subscribe();
    let mut last_sent = String::new();
    loop {
        // Looked up each time: revoking a link cuts off pages already open.
        let view = state
            .share_by_token(&token)
            .ok_or(SharedError::Gone)
            .and_then(|share| shared_view(&state, &share, &token, today_at(offset_min)));
        match view {
            Ok(view) => {
                let message = serde_json::json!({ "title": view.title, "html": view.tasks }).to_string();
                if message != last_sent {
                    if socket.send(Message::Text(message.clone().into())).await.is_err() {
                        return;
                    }
                    last_sent = message;
                }
            }
            Err(SharedError::Gone) => {
                let frame = CloseFrame {
                    code: GONE_CLOSE_CODE,
                    reason: "gone".into(),
                };
                let _ = socket.send(Message::Close(Some(frame))).await;
                return;
            }
            // Passing trouble with the replica: keep the socket, try again at the next change.
            Err(SharedError::Unavailable) => {}
            Err(SharedError::Core(e)) => tracing::warn!(error = %e, "live shared list failed"),
        }
        // Until the log changes; other frames (pings, stray text) are ignored.
        loop {
            tokio::select! {
                poke = pokes.recv() => match poke {
                    // Missed pokes only mean several changes at once: render the latest.
                    Ok(_) | Err(broadcast::error::RecvError::Lagged(_)) => break,
                    Err(broadcast::error::RecvError::Closed) => return,
                },
                incoming = socket.recv() => match incoming {
                    Some(Ok(Message::Close(_)) | Err(_)) | None => return,
                    Some(Ok(_)) => {}
                },
            }
        }
    }
}

/// Swaps in the rows the server sends over `/live` (the add box is left alone, so typing isn't
/// lost), and posts ticks and adds in the background instead of reloading. Without it the forms
/// still work as plain posts.
const LIVE_SCRIPT: &str = r#"(()=>{const tasks=document.getElementById("tasks"),h1=document.querySelector("main h1");let ws,timer,delay=1e3;
function connect(){ws=new WebSocket(location.href.replace(/[?#].*$/,"").replace(/^http/,"ws")+"/live");
ws.onopen=()=>{delay=1e3};
ws.onmessage=e=>{const m=JSON.parse(e.data),open=tasks.querySelector("details")?.open;tasks.innerHTML=m.html;if(open)tasks.querySelector("details")?.setAttribute("open","");h1.textContent=m.title;document.title=m.title+" · not"};
ws.onclose=e=>{if(e.code===4404)return location.reload();timer=setTimeout(connect,delay);delay=Math.min(delay*2,3e4)}}
connect();
document.addEventListener("visibilitychange",()=>{if(!document.hidden&&ws.readyState>1){clearTimeout(timer);delay=1e3;connect()}});
document.addEventListener("submit",e=>{const f=e.target;e.preventDefault();const live=ws.readyState===1;
fetch(f.action,{method:"POST",body:new URLSearchParams(new FormData(f)),redirect:"manual"}).then(r=>{if(r.type!=="opaqueredirect"&&!r.ok||!live)location.reload()},()=>location.reload());
if(f.classList.contains("add"))f.reset();else f.querySelector(".check")?.classList.toggle("on")})})()"#;

async fn shared_add(
    State(state): State<AppState>,
    headers: HeaderMap,
    UrlPath(token): UrlPath<String>,
    Form(form): Form<AddForm>,
) -> Response {
    let Some(share) = state.share_by_token(&token).filter(|s| s.can_edit) else {
        return not_found();
    };
    let Some(store) = state.replica(true) else {
        return unavailable();
    };
    // Guests' text is a title: no `#list` jumping out of the shared list. Dates still parse.
    let text = form.text.replace('#', "");
    if !text.trim().is_empty()
        && let Err(e) = store.quick_add(&share.list_id, &text, today(&headers), &[], None)
    {
        return core_error(e);
    }
    state.publish_replica();
    Redirect::to(&format!("/s/{}", urlencode(&token))).into_response()
}

async fn shared_toggle(
    State(state): State<AppState>,
    headers: HeaderMap,
    UrlPath((token, task)): UrlPath<(String, String)>,
) -> Response {
    let Some(share) = state.share_by_token(&token).filter(|s| s.can_edit) else {
        return not_found();
    };
    let Some(store) = state.replica(true) else {
        return unavailable();
    };
    // Only tasks of the shared list.
    let in_list = store
        .tasks(&share.list_id)
        .is_ok_and(|tasks| tasks.iter().any(|t| t.id == task));
    if !in_list {
        return not_found();
    }
    if let Err(e) = toggle(&store, &task, today(&headers)) {
        return core_error(e);
    }
    state.publish_replica();
    Redirect::to(&format!("/s/{}", urlencode(&token))).into_response()
}

// ---- pieces --------------------------------------------------------------------------------------

fn toggle(store: &not_core::Store, id: &str, today: Date) -> not_core::Result<()> {
    let task = store.task_by_id(id)?;
    store.set_completed(id, task.completed_at.is_none(), today)
}

fn flat(tasks: Vec<Task>) -> Vec<OutlineRow> {
    tasks
        .into_iter()
        .map(|task| OutlineRow {
            task,
            depth: 0,
            done: false,
        })
        .collect()
}

/// Rows with a tick button each (a form, so no script), titles, and when/where for date views.
#[allow(clippy::too_many_arguments)]
fn task_rows(
    today: Date,
    rows: &[OutlineRow],
    action: impl Fn(&Task) -> String,
    back: &str,
    list_title: Option<&dyn Fn(&str) -> String>,
    can_edit: bool,
) -> String {
    let mut out = String::from(r#"<ul class="tasks">"#);
    for row in rows {
        let t = &row.task;
        let done = row.done || t.completed_at.is_some();
        let mut meta = Vec::new();
        if let Some(due) = t.due.as_deref().and_then(|d| Date::parse(d).ok()) {
            meta.push(not_core::due_label(due, t.due_time, today));
        }
        if let Some(title_of) = list_title {
            meta.push(title_of(&t.list_id));
        }
        let check = if can_edit {
            format!(
                r#"<form method="post" action="{}"><input type="hidden" name="back" value="{}"><button class="check{}" aria-label="{} {}"></button></form>"#,
                action(t),
                back,
                if done { " on" } else { "" },
                if done { "Reopen" } else { "Complete" },
                esc(&t.title)
            )
        } else {
            format!(
                r#"<span class="check{}" aria-hidden="true"></span>"#,
                if done { " on" } else { "" }
            )
        };
        let _ = write!(
            out,
            r#"<li class="{}{}" style="--depth:{}">{check}<div><span class="title">{}</span>{}{}</div></li>"#,
            if done { "done" } else { "" },
            match t.priority {
                3 => " p3",
                2 => " p2",
                1 => " p1",
                _ => "",
            },
            row.depth,
            esc(&t.title),
            if meta.is_empty() {
                String::new()
            } else {
                format!(r#"<span class="meta">{}</span>"#, esc(&meta.join(" · ")))
            },
            t.notes
                .lines()
                .find(|l| !l.trim().is_empty())
                .map_or(String::new(), |n| format!(
                    r#"<span class="meta">{}</span>"#,
                    esc(n)
                ))
        );
    }
    out.push_str("</ul>");
    out
}

/// Headings, bullets, checkboxes and paragraphs: enough to read a note; everything escaped.
fn markdown_html(text: &str) -> String {
    let mut out = String::new();
    let mut in_list = false;
    let images: std::collections::HashMap<usize, String> = {
        // Line index → image id, from the core's parser.
        let mut by_start = std::collections::HashMap::new();
        let mut line_of = std::collections::HashMap::new();
        let mut offset = 0u32;
        for (i, line) in text.split('\n').enumerate() {
            line_of.insert(offset, i);
            offset += line.encode_utf16().count() as u32 + 1;
        }
        for img in not_core::markdown::images(text) {
            if let Some(&i) = line_of.get(&img.start) {
                by_start.insert(i, img.id);
            }
        }
        by_start
    };
    for (index, line) in text.lines().enumerate() {
        let trimmed = line.trim_start();
        // A task block of an earlier version, until the note is opened in an app: markup only.
        if !not_core::markdown::task_blocks(line).is_empty() {
            continue;
        }
        if let Some(id) = images.get(&index) {
            if in_list {
                out.push_str("</ul>");
                in_list = false;
            }
            let _ = write!(out, r#"<img src="/img/{}" alt="" loading="lazy">"#, esc(id));
            continue;
        }
        let item = trimmed.strip_prefix("- ").or_else(|| trimmed.strip_prefix("* "));
        if item.is_none() && in_list {
            out.push_str("</ul>");
            in_list = false;
        }
        if let Some(item) = item {
            if !in_list {
                out.push_str("<ul>");
                in_list = true;
            }
            let (mark, rest) = if let Some(r) = item.strip_prefix("[ ] ") {
                ("☐ ", r)
            } else if let Some(r) = item.strip_prefix("[x] ").or_else(|| item.strip_prefix("[X] ")) {
                ("☑ ", r)
            } else {
                ("", item)
            };
            let _ = write!(out, "<li>{mark}{}</li>", esc(rest));
        } else if let Some(h) = trimmed.strip_prefix("### ") {
            let _ = write!(out, "<h3>{}</h3>", esc(h));
        } else if let Some(h) = trimmed.strip_prefix("## ") {
            let _ = write!(out, "<h2>{}</h2>", esc(h));
        } else if let Some(h) = trimmed.strip_prefix("# ") {
            let _ = write!(out, "<h1>{}</h1>", esc(h));
        } else if trimmed.is_empty() {
            out.push_str("<br>");
        } else {
            let _ = write!(out, "<p>{}</p>", esc(line));
        }
    }
    if in_list {
        out.push_str("</ul>");
    }
    out
}

fn page(title: &str, nav: Option<&str>, body: &str) -> Html<String> {
    Html(format!(
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta name="robots" content="noindex">
<title>{} · not</title>
<link rel="stylesheet" href="/style.css">
<script>document.cookie="{OFFSET_COOKIE}="+new Date().getTimezoneOffset()+"; path=/; SameSite=Lax; max-age=31536000"</script>
</head>
<body>
{}
{body}
</body>
</html>"#,
        esc(title),
        nav.map_or(String::new(), |n| format!("<nav>{n}</nav>"))
    ))
}

async fn stylesheet() -> impl IntoResponse {
    (
        [
            (header::CONTENT_TYPE, "text/css; charset=utf-8"),
            (header::CACHE_CONTROL, "max-age=3600"),
        ],
        STYLE,
    )
}

const STYLE: &str = r#":root{--bg:#f6f7f9;--surface:#fff;--ink:#1d2030;--muted:#5d6275;--faint:#9aa0b2;--line:#e4e6ec;--accent:#8be9a8;--accent-ink:#1d7a45;--danger:#d9534f;color-scheme:light dark}
@media (prefers-color-scheme:dark){:root{--bg:#1a1c28;--surface:#23263a;--ink:#eef0f6;--muted:#a3a8bb;--faint:#6d7288;--line:#33374d;--accent:#8be9a8;--accent-ink:#8be9a8}}
*{box-sizing:border-box}body{margin:0;background:var(--bg);color:var(--ink);font:16px/1.45 system-ui,-apple-system,"Segoe UI",sans-serif}
nav{display:flex;gap:4px;overflow-x:auto;padding:12px 16px;border-bottom:1px solid var(--line);background:var(--surface)}
nav a{white-space:nowrap;padding:6px 12px;border-radius:999px;color:var(--muted);text-decoration:none}nav a[aria-current]{background:var(--accent);color:#14301f}
main{max-width:640px;margin:0 auto;padding:16px}main.narrow{max-width:420px;padding-top:15vh}
h1{font-size:2rem;margin:.5rem 0 1rem;letter-spacing:-.02em}h2{font-size:1rem;color:var(--muted);margin:1.5rem 0 .5rem}
.muted,.empty{color:var(--muted)}.error{color:var(--danger)}
input{font:inherit;width:100%;padding:12px 16px;border-radius:999px;border:1px solid var(--line);background:var(--surface);color:var(--ink)}
input:focus{outline:2px solid var(--accent);border-color:transparent}
button{font:inherit;cursor:pointer;border:0;border-radius:999px;padding:10px 18px;background:var(--accent);color:#14301f;font-weight:600}
button.quiet{background:none;color:var(--muted);font-weight:400;padding:8px 0}
.stack{display:grid;gap:10px}.add{display:flex;gap:8px;margin-bottom:12px}.add input{flex:1;min-width:0;width:auto}.add button{width:46px;padding:0;font-size:1.4rem}
ul.tasks{list-style:none;margin:0;padding:0}
ul.tasks li{display:flex;gap:12px;align-items:flex-start;padding:10px 4px 10px calc(4px + var(--depth)*28px);border-bottom:1px solid var(--line)}
ul.tasks li div{display:flex;flex-direction:column;min-width:0}.title{overflow-wrap:anywhere}
.meta{font-size:.85rem;color:var(--faint)}li.done .title{color:var(--faint);text-decoration:line-through}
.check{display:block;width:24px;height:24px;flex:none;padding:0;border-radius:50%;border:2px solid var(--faint);background:none;margin-top:1px}
.p3 .check{border-color:#e5534b}.p2 .check{border-color:#e8a33d}.p1 .check{border-color:#5b8def}
.check.on{background:var(--accent);border-color:var(--accent)}.check.on::after{content:"";display:block;width:6px;height:11px;margin:3px auto 0;border:solid #14301f;border-width:0 2px 2px 0;transform:rotate(45deg)}
button.check:hover{border-color:var(--accent-ink)}
details{margin-top:16px}summary{cursor:pointer;color:var(--muted);padding:8px 0}
ul.notes{list-style:none;padding:0;margin:0;display:grid;gap:8px}ul.notes a{display:flex;flex-direction:column;padding:12px 16px;border-radius:16px;background:var(--surface);color:inherit;text-decoration:none}
ul.notes span{color:var(--muted);font-size:.9rem;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}
.note{background:var(--surface);border-radius:20px;padding:8px 20px}.note p{margin:.3rem 0}.note img{display:block;max-width:100%;max-height:70vh;border-radius:12px;margin:.6rem 0}
.signout{margin-top:32px}code{background:var(--surface);padding:1px 6px;border-radius:6px}
"#;

fn signed_in(state: &AppState, headers: &HeaderMap) -> Option<crate::DeviceId> {
    let token = cookie(headers, SESSION_COOKIE)?;
    state.authenticate(&token).ok().flatten()
}

fn cookie(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .filter_map(|pair| pair.trim().split_once('='))
        .find(|(k, _)| *k == name)
        .map(|(_, v)| v.to_owned())
}

/// The visitor's today, from the UTC offset their browser reported (UTC until it has).
fn today(headers: &HeaderMap) -> Date {
    today_at(utc_offset_min(headers))
}

/// Minutes behind UTC, as `Date.getTimezoneOffset()` reports them.
fn utc_offset_min(headers: &HeaderMap) -> i64 {
    cookie(headers, OFFSET_COOKIE)
        .and_then(|v| v.parse().ok())
        .unwrap_or(0)
}

fn today_at(offset_min: i64) -> Date {
    let now_ms = crate::now_millis() - offset_min * 60_000;
    Date::from_ymd(1970, 1, 1)
        .expect("epoch")
        .add_days(now_ms.div_euclid(86_400_000))
}

fn is_https(headers: &HeaderMap) -> bool {
    headers.get("x-forwarded-proto").and_then(|v| v.to_str().ok()) == Some("https")
}

/// Only our own paths: never an open redirect.
fn safe_back<'a>(back: &'a str, fallback: &'a str) -> &'a str {
    if back.starts_with('/') && !back.starts_with("//") {
        back
    } else {
        fallback
    }
}

fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

fn urlencode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'@' => {
                (b as char).to_string()
            }
            b => format!("%{b:02X}"),
        })
        .collect()
}

fn not_found() -> Response {
    (StatusCode::NOT_FOUND, page("Not found", None, r#"<main class="narrow"><h1>Not here</h1><p class="muted">This link doesn't lead anywhere (any more).</p></main>"#)).into_response()
}

fn unavailable() -> Response {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        "the server's copy of your data isn't available",
    )
        .into_response()
}

fn core_error(e: not_core::Error) -> Response {
    tracing::warn!(error = %e, "web request failed");
    (
        StatusCode::BAD_REQUEST,
        page(
            "Something went wrong",
            None,
            &format!(
                r#"<main class="narrow"><h1>Something went wrong</h1><p class="muted">{}</p></main>"#,
                esc(&e.to_string())
            ),
        ),
    )
        .into_response()
}

fn server_error(e: rusqlite::Error) -> Response {
    tracing::error!(error = %e, "storage error");
    StatusCode::INTERNAL_SERVER_ERROR.into_response()
}
