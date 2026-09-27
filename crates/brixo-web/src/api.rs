//! The website's API and pages. Sessions are a random token in an
//! httpOnly cookie. Game servers are handed out at `App::network`'s public
//! address, on its port range.

use std::net::SocketAddr;
use std::sync::Arc;

use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use axum::extract::{ConnectInfo, DefaultBodyLimit, Path, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use brixo_core::{Color, DataModel, Face};
use serde::{Deserialize, Serialize};

use crate::db::{Avatar, Db, User};
use crate::limits::{self, Limiter};
use crate::servers::{Network, Servers, Tickets};

const COOKIE: &str = "brixo_session";

/// How the website runs. The defaults are right for your own PC; a real
/// server turns on the last three (see main.rs for the settings' names).
#[derive(Clone, Debug)]
pub struct Settings {
    /// Where players reach game servers.
    pub network: Network,
    /// Login cookies only travel over HTTPS (BRIXO_SECURE_COOKIES=1).
    pub secure_cookies: bool,
    /// The website sits behind Caddy, which says who the visitor really is
    /// in X-Forwarded-For (BRIXO_TRUST_PROXY=1). Only turn this on behind
    /// a proxy: otherwise anyone could claim to be anyone.
    pub trust_proxy: bool,
    /// Signing up needs an invite code (BRIXO_INVITE_ONLY=1).
    pub invite_only: bool,
    /// Where the Player and Studio downloads and their versions.json live
    /// (BRIXO_DOWNLOADS). Live, Caddy serves /files/ from here itself.
    pub downloads: std::path::PathBuf,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            network: Network::default(),
            secure_cookies: false,
            trust_proxy: false,
            invite_only: false,
            downloads: "downloads".into(),
        }
    }
}

pub struct App {
    pub db: Db,
    pub servers: Servers,
    pub tickets: Tickets,
    pub network: Network,
    pub settings: Settings,
    pub limits: Limiter,
}

impl App {
    /// A website for your own PC: game servers at 127.0.0.1, any port.
    pub fn open(db_path: &str) -> rusqlite::Result<App> {
        Self::open_with(db_path, Settings::default())
    }

    pub fn open_with(db_path: &str, settings: Settings) -> rusqlite::Result<App> {
        Ok(App {
            db: Db::open(db_path)?,
            servers: Servers::default(),
            tickets: Tickets::default(),
            network: settings.network.clone(),
            settings,
            limits: Limiter::default(),
        })
    }
}

/// Biggest game Studio may publish (Flagfall is about 4.5 MB).
pub const MAX_UPLOAD: usize = 32 * 1024 * 1024;

pub fn router(app: Arc<App>) -> Router {
    Router::new()
        .route("/api/signup", post(signup))
        .route("/api/login", post(login))
        .route("/api/logout", post(logout))
        .route("/api/me", get(me))
        .route("/api/avatar", put(set_avatar))
        .route("/api/me/blurb", put(set_blurb))
        .route("/api/stats", get(stats))
        .route("/api/games", get(games).post(publish).layer(DefaultBodyLimit::max(MAX_UPLOAD)))
        // axum 0.7 path parameters use `:name` (not `{name}`).
        .route("/api/games/:id", get(game).put(edit_game))
        .route("/api/games/:id/thumbnail", get(thumbnail))
        .route("/api/games/:id/play", post(play))
        .route("/api/users/:name", get(profile))
        .route("/api/version", get(version))
        .route("/files/:name", get(download_file))
        .route("/", get(|| async { Html(include_str!("web/index.html")) }))
        .route("/games", get(|| async { Html(include_str!("web/games.html")) }))
        .route("/games/:id", get(|| async { Html(include_str!("web/game.html")) }))
        .route("/users/:name", get(|| async { Html(include_str!("web/profile.html")) }))
        .route("/download", get(|| async { Html(include_str!("web/download.html")) }))
        .route("/login", get(|| async { Html(include_str!("web/login.html")) }))
        .route("/signup", get(|| async { Html(include_str!("web/signup.html")) }))
        .route("/avatar", get(|| async { Html(include_str!("web/avatar.html")) }))
        .route("/favicon.svg", get(|| async { ([(header::CONTENT_TYPE, "image/svg+xml")], include_str!("web/favicon.svg")) }))
        .route("/app.css", get(|| async { ([(header::CONTENT_TYPE, "text/css")], include_str!("web/app.css")) }))
        .route("/app.js", get(|| async { ([(header::CONTENT_TYPE, "application/javascript")], include_str!("web/app.js")) }))
        // The avatar and hat models, written by brixo-render's web_model test.
        .route(
            "/avatar-model.json",
            get(|| async {
                ([(header::CONTENT_TYPE, "application/json"), (header::CACHE_CONTROL, "public, max-age=600")], include_str!("web/avatar-model.json"))
            }),
        )
        .layer(axum::middleware::map_response(safety_headers))
        .with_state(app)
}

/// Stops other sites framing ours (clickjacking) and browsers guessing
/// file types.
async fn safety_headers(mut res: Response) -> Response {
    let h = res.headers_mut();
    h.insert("x-content-type-options", header::HeaderValue::from_static("nosniff"));
    h.insert("x-frame-options", header::HeaderValue::from_static("DENY"));
    h.insert("referrer-policy", header::HeaderValue::from_static("same-origin"));
    res
}

// --- errors ------------------------------------------------------------------

/// Every error is `{"error": "..."}` with a sensible status.
pub struct ApiError(StatusCode, String);

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(serde_json::json!({ "error": self.1 }))).into_response()
    }
}

fn bad(msg: &str) -> ApiError {
    ApiError(StatusCode::BAD_REQUEST, msg.to_string())
}
fn oops(e: impl std::fmt::Display) -> ApiError {
    ApiError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}
fn slow_down() -> ApiError {
    ApiError(StatusCode::TOO_MANY_REQUESTS, "too many tries; wait a few minutes and try again".to_string())
}
fn not_logged_in() -> ApiError {
    ApiError(StatusCode::UNAUTHORIZED, "log in first".to_string())
}

type Result<T> = std::result::Result<T, ApiError>;

fn session(headers: &HeaderMap) -> Option<String> {
    let cookies = headers.get(header::COOKIE)?.to_str().ok()?;
    cookies.split(';').find_map(|c| c.trim().strip_prefix(&format!("{COOKIE}=")).map(str::to_string))
}

fn user(app: &App, headers: &HeaderMap) -> Result<User> {
    let token = session(headers).ok_or_else(not_logged_in)?;
    app.db.session_user(&token).map_err(oops)?.ok_or_else(not_logged_in)
}

fn with_session(app: &App, token: &str, body: impl IntoResponse) -> Response {
    let secure = if app.settings.secure_cookies { "; Secure" } else { "" };
    let age = crate::db::SESSION_DAYS * 86400;
    let cookie = format!("{COOKIE}={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age={age}{secure}");
    ([(header::SET_COOKIE, cookie)], body).into_response()
}

/// Who's asking, for rate limits: their IP address. Behind Caddy that's
/// the last X-Forwarded-For entry (the one Caddy itself added).
fn client_ip(app: &App, headers: &HeaderMap, peer: Option<ConnectInfo<SocketAddr>>) -> String {
    if app.settings.trust_proxy {
        if let Some(ip) = headers
            .get("x-forwarded-for")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.rsplit(',').next())
            .map(str::trim)
            .filter(|ip| !ip.is_empty())
        {
            return ip.to_string();
        }
    }
    peer.map(|ConnectInfo(a)| a.ip().to_string()).unwrap_or_else(|| "unknown".to_string())
}

// --- accounts ------------------------------------------------------------------

#[derive(Deserialize)]
struct Credentials {
    username: String,
    password: String,
    /// Needed when the site is invite-only.
    #[serde(default)]
    invite: String,
}

/// Words that would make a name look like Brixo staff ("BrixoAdmin",
/// "Official_Mod"). Fans can still be "BrixoFan".
const STAFF_WORDS: [&str; 4] = ["admin", "moderator", "official", "staff"];

/// Usernames: 3-20 letters, numbers or _, and nothing the chat filter
/// would hide (so nobody's name is a rude word).
pub fn check_username(name: &str) -> std::result::Result<(), &'static str> {
    if !(3..=20).contains(&name.len()) || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err("usernames are 3-20 letters, numbers or _");
    }
    let spaced = name.replace('_', " ");
    if brixo_runtime::filter_chat(&spaced) != spaced {
        return Err("please pick a different username");
    }
    let lower = name.to_ascii_lowercase();
    if lower.trim_matches('_') == "brixo" || STAFF_WORDS.iter().any(|w| lower.contains(w)) {
        return Err("that name is saved for Brixo staff; please pick another");
    }
    Ok(())
}

/// Passwords: 8 to 128 characters (the cap stops giant ones being used
/// to tie up the server hashing them).
pub fn check_password(password: &str) -> std::result::Result<(), &'static str> {
    match password.chars().count() {
        n if n < 8 => Err("passwords need at least 8 characters"),
        n if n > 128 => Err("passwords can be up to 128 characters"),
        _ => Ok(()),
    }
}

pub fn hash(password: &str) -> String {
    Argon2::default().hash_password(password.as_bytes(), &SaltString::generate(&mut OsRng)).unwrap().to_string()
}

fn verify(password: &str, hash: &str) -> bool {
    PasswordHash::new(hash).is_ok_and(|h| Argon2::default().verify_password(password.as_bytes(), &h).is_ok())
}

/// Hashing is deliberately slow: do it off the threads serving pages.
async fn hash_off_thread(password: String) -> Result<String> {
    tokio::task::spawn_blocking(move || hash(&password)).await.map_err(oops)
}

async fn signup(
    State(app): State<Arc<App>>,
    headers: HeaderMap,
    peer: Option<ConnectInfo<SocketAddr>>,
    Json(c): Json<Credentials>,
) -> Result<Response> {
    let ip = client_ip(&app, &headers, peer);
    let name = c.username.trim();
    check_username(name).map_err(bad)?;
    check_password(&c.password).map_err(bad)?;
    let (signups, bad_invites) = (format!("signup:{ip}"), format!("invite:{ip}"));
    if !app.limits.ok(&signups, limits::SIGNUPS) || !app.limits.ok(&bad_invites, limits::BAD_INVITES) {
        return Err(slow_down());
    }
    let hashed = hash_off_thread(c.password).await?;
    let taken = |_| bad("that username is taken");
    let u = if app.settings.invite_only {
        if c.invite.trim().is_empty() {
            return Err(bad("Brixo is invite-only for now: you need an invite code"));
        }
        match app.db.create_user_invited(name, &hashed, &c.invite).map_err(taken)? {
            Some(u) => u,
            None => {
                app.limits.hit(&bad_invites);
                return Err(bad("that invite code isn't right, or it's been used"));
            }
        }
    } else {
        app.db.create_user(name, &hashed).map_err(taken)?
    };
    app.limits.hit(&signups);
    let token = app.db.new_session(u.id).map_err(oops)?;
    Ok(with_session(&app, &token, Json(u)))
}

async fn login(
    State(app): State<Arc<App>>,
    headers: HeaderMap,
    peer: Option<ConnectInfo<SocketAddr>>,
    Json(c): Json<Credentials>,
) -> Result<Response> {
    let wrong = || bad("wrong username or password");
    let name = c.username.trim().to_ascii_lowercase();
    // Wrong guesses count against both the address and the account, so
    // nobody can keep guessing one person's password from many places.
    let (by_ip, by_name) = (format!("login:{}", client_ip(&app, &headers, peer)), format!("login-user:{name}"));
    if !app.limits.ok(&by_ip, limits::LOGIN_FAILS) || !app.limits.ok(&by_name, limits::LOGIN_FAILS) {
        return Err(slow_down());
    }
    let failed = || {
        app.limits.hit(&by_ip);
        app.limits.hit(&by_name);
        wrong()
    };
    if c.password.chars().count() > 128 || name.len() > 20 {
        return Err(failed());
    }
    let Some((id, h)) = app.db.login_info(&name).map_err(oops)? else {
        return Err(failed());
    };
    let password = c.password;
    let right = tokio::task::spawn_blocking(move || verify(&password, &h)).await.map_err(oops)?;
    if !right {
        return Err(failed());
    }
    let u = app.db.user(id).map_err(oops)?.ok_or_else(wrong)?;
    let token = app.db.new_session(id).map_err(oops)?;
    Ok(with_session(&app, &token, Json(u)))
}

async fn logout(State(app): State<Arc<App>>, headers: HeaderMap) -> StatusCode {
    if let Some(t) = session(&headers) {
        let _ = app.db.end_session(&t);
    }
    StatusCode::NO_CONTENT
}

async fn me(State(app): State<Arc<App>>, headers: HeaderMap) -> Result<Json<User>> {
    Ok(Json(user(&app, &headers)?))
}

async fn set_avatar(State(app): State<Arc<App>>, headers: HeaderMap, Json(a): Json<Avatar>) -> Result<StatusCode> {
    let u = user(&app, &headers)?;
    if Face::ALL.iter().all(|f| f.name() != a.face) {
        return Err(bad("that isn't one of the faces"));
    }
    if a.hats.len() > brixo_core::MAX_HATS {
        return Err(bad(&format!("you can wear up to {} hats", brixo_core::MAX_HATS)));
    }
    for (i, h) in a.hats.iter().enumerate() {
        if brixo_core::Hat::from_name(h).is_none() {
            return Err(bad("that isn't one of the hats"));
        }
        if a.hats[..i].contains(h) {
            return Err(bad("you're already wearing that hat"));
        }
    }
    app.db.set_avatar(u.id, &a).map_err(oops)?;
    Ok(StatusCode::NO_CONTENT)
}

// --- games ------------------------------------------------------------------

#[derive(Serialize)]
struct CatalogEntry {
    id: i64,
    name: String,
    owner: String,
    playing: usize,
    visits: i64,
    description: String,
    created: i64,
    has_thumbnail: bool,
}

fn entry(g: crate::db::GameRow, counts: &std::collections::HashMap<i64, usize>) -> CatalogEntry {
    CatalogEntry {
        playing: counts.get(&g.id).copied().unwrap_or(0),
        id: g.id,
        name: g.name,
        owner: g.owner,
        visits: g.visits,
        description: g.description,
        created: g.created,
        has_thumbnail: g.has_thumbnail,
    }
}

fn not_found(what: &str) -> ApiError {
    ApiError(StatusCode::NOT_FOUND, format!("no such {what}"))
}

async fn games(State(app): State<Arc<App>>) -> Result<Json<Vec<CatalogEntry>>> {
    let counts = app.servers.player_counts();
    let rows = app.db.games().map_err(oops)?;
    Ok(Json(rows.into_iter().map(|g| entry(g, &counts)).collect()))
}

async fn game(State(app): State<Arc<App>>, Path(id): Path<i64>) -> Result<Json<CatalogEntry>> {
    let g = app.db.game(id).map_err(oops)?.ok_or_else(|| not_found("game"))?;
    Ok(Json(entry(g, &app.servers.player_counts())))
}

#[derive(Deserialize)]
struct GameInfo {
    description: String,
}

/// The game's owner edits its description (filtered like chat).
async fn edit_game(State(app): State<Arc<App>>, headers: HeaderMap, Path(id): Path<i64>, Json(info): Json<GameInfo>) -> Result<StatusCode> {
    let u = user(&app, &headers)?;
    let text = info.description.trim();
    if text.chars().count() > 1000 {
        return Err(bad("descriptions are up to 1000 characters"));
    }
    if app.db.set_game_info(id, u.id, &brixo_runtime::filter_chat(text)).map_err(oops)? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError(StatusCode::FORBIDDEN, "only the game's creator can change that".into()))
    }
}

async fn thumbnail(State(app): State<Arc<App>>, Path(id): Path<i64>) -> Result<Response> {
    let png = app.db.thumbnail(id).map_err(oops)?.ok_or_else(|| not_found("picture"))?;
    Ok(([(header::CONTENT_TYPE, "image/png"), (header::CACHE_CONTROL, "public, max-age=300")], png).into_response())
}

#[derive(Serialize)]
struct Profile {
    username: String,
    avatar: crate::db::Avatar,
    blurb: String,
    created: i64,
    games: Vec<CatalogEntry>,
}

async fn profile(State(app): State<Arc<App>>, Path(name): Path<String>) -> Result<Json<Profile>> {
    let u = app.db.user_by_name(&name).map_err(oops)?.ok_or_else(|| not_found("player"))?;
    let counts = app.servers.player_counts();
    let games = app.db.games_by(u.id).map_err(oops)?.into_iter().map(|g| entry(g, &counts)).collect();
    Ok(Json(Profile { username: u.username, avatar: u.avatar, blurb: u.blurb, created: u.created, games }))
}

#[derive(Deserialize)]
struct Blurb {
    blurb: String,
}

async fn set_blurb(State(app): State<Arc<App>>, headers: HeaderMap, Json(b): Json<Blurb>) -> Result<StatusCode> {
    let u = user(&app, &headers)?;
    let text = b.blurb.trim();
    if text.chars().count() > 200 {
        return Err(bad("blurbs are up to 200 characters"));
    }
    app.db.set_blurb(u.id, &brixo_runtime::filter_chat(text)).map_err(oops)?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Serialize)]
struct Stats {
    /// Players in games right now.
    online: usize,
    games: i64,
    users: i64,
    /// Whether signing up needs an invite code (the pages show the box).
    invite_only: bool,
}

async fn stats(State(app): State<Arc<App>>) -> Result<Json<Stats>> {
    let online = app.servers.player_counts().values().sum();
    let (users, games) = app.db.counts().map_err(oops)?;
    Ok(Json(Stats { online, games, users, invite_only: app.settings.invite_only }))
}

/// The latest Player and Studio downloads, as tools/release.ps1 wrote them:
/// {"player": {"version", "file", "bytes"}, "studio": {...}}. Empty until
/// the first release. Player and Studio check it for updates.
async fn version(State(app): State<Arc<App>>) -> Json<serde_json::Value> {
    let path = app.settings.downloads.join("versions.json");
    let v = tokio::fs::read_to_string(path).await.ok().and_then(|t| serde_json::from_str(&t).ok());
    Json(v.unwrap_or_else(|| serde_json::json!({})))
}

/// A download, on your own PC. (Live, Caddy answers /files/ before this.)
async fn download_file(State(app): State<Arc<App>>, Path(name): Path<String>) -> Result<Response> {
    let safe = !name.is_empty() && !name.starts_with('.') && name.chars().all(|c| c.is_ascii_alphanumeric() || "._-".contains(c));
    if !safe {
        return Err(not_found("file"));
    }
    let bytes = tokio::fs::read(app.settings.downloads.join(&name)).await.map_err(|_| not_found("file"))?;
    let disposition = format!("attachment; filename=\"{name}\"");
    Ok(([(header::CONTENT_TYPE, "application/octet-stream".to_string()), (header::CONTENT_DISPOSITION, disposition)], bytes).into_response())
}

#[derive(Deserialize)]
struct Upload {
    name: String,
    /// The game as Brixo Studio saves it (JSON).
    data: String,
}

/// Brixo Studio's Publish: uploads a game into the catalog.
async fn publish(State(app): State<Arc<App>>, headers: HeaderMap, Json(up): Json<Upload>) -> Result<Json<serde_json::Value>> {
    let u = user(&app, &headers)?;
    if !app.limits.take(&format!("publish:{}", u.id), limits::PUBLISHES) {
        return Err(slow_down());
    }
    let name = up.name.trim();
    if name.is_empty() || name.chars().count() > 40 {
        return Err(bad("game names are 1-40 characters"));
    }
    DataModel::from_json(&up.data).map_err(|_| bad("that isn't a Brixo game"))?;
    let id = app.db.publish(u.id, name, &up.data).map_err(oops)?;
    Ok(Json(serde_json::json!({ "id": id })))
}

#[derive(Serialize)]
pub struct PlayPass {
    /// The game server to join ("127.0.0.1:port").
    pub server: String,
    /// A one-time ticket for it.
    pub ticket: String,
}

/// Pressing Play: make sure this game has a server running, and hand out
/// a one-time ticket into it. The page then opens `brixo://play?...`.
async fn play(State(app): State<Arc<App>>, headers: HeaderMap, Path(game_id): Path<i64>) -> Result<Json<PlayPass>> {
    let u = user(&app, &headers)?;
    if !app.limits.take(&format!("play:{}", u.id), limits::PLAYS) {
        return Err(slow_down());
    }
    let data = app.db.game_data(game_id).map_err(oops)?.ok_or_else(|| ApiError(StatusCode::NOT_FOUND, "no such game".into()))?;
    let port = app
        .servers
        .port_for(game_id, app.network.ports, |port| {
            let model = DataModel::from_json(&data).map_err(std::io::Error::other)?;
            brixo_server::start_with_tickets(model, port, ticket_check(app.clone(), game_id))
        })
        .map_err(|e| {
            if crate::servers::all_busy(&e) {
                ApiError(StatusCode::SERVICE_UNAVAILABLE, "every game server is busy right now; try again in a minute".into())
            } else {
                oops(e)
            }
        })?;
    let ticket = app.tickets.issue(u.id, game_id);
    let _ = app.db.add_visit(game_id);
    Ok(Json(PlayPass { server: app.network.address(port), ticket }))
}

/// How a game server checks who's joining: the ticket must be fresh and
/// for this game; it's used up, and the player joins as their account.
fn ticket_check(app: Arc<App>, game_id: i64) -> brixo_server::TicketCheck {
    Arc::new(move |ticket: &str| {
        let user_id = app.tickets.redeem(ticket, game_id)?;
        let u = app.db.user(user_id).ok()??;
        Some(brixo_server::Identity { name: u.username, look: look_of(&u.avatar) })
    })
}

pub fn look_of(a: &Avatar) -> brixo_runtime::Look {
    let c = |(r, g, b): (u8, u8, u8)| Color::new(r, g, b);
    brixo_runtime::Look {
        skin: c(a.skin),
        shirt: c(a.shirt),
        pants: c(a.pants),
        shoes: c(a.shoes),
        face: Face::from_name(&a.face).unwrap_or_default(),
        hats: brixo_core::Hat::list(&a.hats),
    }
}

/// Makes sure the sample games are in the catalog, published by "Brixo".
pub fn seed_samples(app: &App) {
    let owner = match app.db.user_by_name("Brixo") {
        Ok(Some(u)) => u,
        // Nobody can log in as Brixo: its password is random and forgotten.
        _ => match app.db.create_user("Brixo", &hash(&crate::db::random_token())) {
            Ok(u) => u,
            Err(_) => return,
        },
    };
    let samples: [(&str, fn() -> DataModel, &str, &[u8]); 3] = [
        (
            "Flagfall",
            brixo_samples::flagfall,
            "Red against Blue across a river. Grab the enemy flag and bring it home to your own stand, but you can \
             only score while your own flag is safe. Three ways across: the Bridge, the Ruins, the Tunnel. First to 3 \
             captures wins.",
            include_bytes!("../assets/flagfall.png"),
        ),
        (
            "Spire Wars",
            brixo_samples::spire_wars,
            "Four teams, four towers of breakable bricks. Rockets, superballs, timebombs and swords: knock the other \
             towers down and be the last spire standing.",
            include_bytes!("../assets/spire-wars.png"),
        ),
        (
            "Coin Tycoon",
            brixo_samples::coin_tycoon,
            "Claim a plot, buy droppers and conveyors, and watch the coins roll in. Upgrade your way to the richest \
             factory on the server.",
            include_bytes!("../assets/coin-tycoon.png"),
        ),
    ];
    for (name, make, description, png) in samples {
        let data = make().to_json().expect("samples serialize");
        if let Ok(id) = app.db.publish(owner.id, name, &data) {
            let _ = app.db.set_game_info(id, owner.id, description);
            let _ = app.db.set_thumbnail(id, png);
        }
    }
    let _ = app.db.set_blurb(owner.id, "The official Brixo account. We make the sample games.");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usernames_are_checked() {
        assert!(check_username("Ann_2").is_ok());
        for bad in ["ab", "has space", "way_too_long_for_a_username", "noob", "shut_up"] {
            assert!(check_username(bad).is_err(), "{bad} was allowed");
        }
    }

    #[test]
    fn passwords_hash_and_verify() {
        let h = hash("correct horse");
        assert!(verify("correct horse", &h) && !verify("wrong", &h));
    }
}
