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
        .route("/api/admin", get(admin_overview))
        .route("/api/admin/ban", post(admin_ban))
        .route("/api/admin/hide", post(admin_hide))
        .route("/api/admin/reset", post(admin_reset))
        .route("/api/reset/check", post(reset_check))
        .route("/api/reset", post(reset_password))
        .route("/api/me/password", put(change_password))
        .route("/files/:name", get(download_file))
        .route("/", get(|| async { Html(include_str!("web/index.html")) }))
        .route("/games", get(|| async { Html(include_str!("web/games.html")) }))
        .route("/games/:id", get(|| async { Html(include_str!("web/game.html")) }))
        // The guide to making games (docs.rs).
        .route("/learn", get(|| async { axum::response::Redirect::permanent("/learn/welcome") }))
        .route("/learn/search.json", get(|| async {
            ([(header::CONTENT_TYPE, "application/json"), (header::CACHE_CONTROL, "public, max-age=600")], crate::docs::search_index())
        }))
        .route("/learn/:slug", get(learn_page))
        .route("/learn/samples/:file", get(sample_file))
        .route("/learn/img/:file", get(learn_image))
        .route("/users/:name", get(|| async { Html(include_str!("web/profile.html")) }))
        .route("/download", get(|| async { Html(include_str!("web/download.html")) }))
        .route("/login", get(|| async { Html(include_str!("web/login.html")) }))
        .route("/signup", get(|| async { Html(include_str!("web/signup.html")) }))
        .route("/avatar", get(|| async { Html(include_str!("web/avatar.html")) }))
        .route("/admin", get(|| async { Html(include_str!("web/admin.html")) }))
        .route("/reset", get(|| async { Html(include_str!("web/reset.html")) }))
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
    if app.db.is_banned(id).map_err(oops)? {
        return Err(ApiError(StatusCode::FORBIDDEN, "this account has been banned".into()));
    }
    let u = app.db.user(id).map_err(oops)?.ok_or_else(wrong)?;
    let token = app.db.new_session(id).map_err(oops)?;
    Ok(with_session(&app, &token, Json(u)))
}

#[derive(Deserialize)]
struct NewPassword {
    current: String,
    password: String,
}

/// Changes your own password (you have to know the current one). Your
/// other logins are logged out; this one stays.
async fn change_password(State(app): State<Arc<App>>, headers: HeaderMap, Json(p): Json<NewPassword>) -> Result<StatusCode> {
    let u = user(&app, &headers)?;
    let token = session(&headers).ok_or_else(not_logged_in)?;
    let key = format!("login-user:{}", u.username.to_ascii_lowercase());
    if !app.limits.ok(&key, limits::LOGIN_FAILS) {
        return Err(slow_down());
    }
    check_password(&p.password).map_err(bad)?;
    let h = app.db.password_hash(u.id).map_err(oops)?.ok_or_else(not_logged_in)?;
    let current = p.current;
    if current.chars().count() > 128 || !tokio::task::spawn_blocking(move || verify(&current, &h)).await.map_err(oops)? {
        app.limits.hit(&key);
        return Err(bad("your current password isn't right"));
    }
    let hashed = hash_off_thread(p.password).await?;
    app.db.change_password(u.id, &hashed, &token).map_err(oops)?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct ResetToken {
    token: String,
}

#[derive(Deserialize)]
struct ResetRequest {
    token: String,
    password: String,
}

fn bad_link() -> ApiError {
    bad("this reset link has expired or been used: ask an admin for a new one")
}

/// Whose reset link this is (the reset page shows the name).
async fn reset_check(
    State(app): State<Arc<App>>,
    headers: HeaderMap,
    peer: Option<ConnectInfo<SocketAddr>>,
    Json(r): Json<ResetToken>,
) -> Result<Json<serde_json::Value>> {
    let key = format!("reset:{}", client_ip(&app, &headers, peer));
    if !app.limits.ok(&key, limits::BAD_RESETS) {
        return Err(slow_down());
    }
    match app.db.reset_user(r.token.trim()).map_err(oops)? {
        Some(name) => Ok(Json(serde_json::json!({ "username": name }))),
        None => {
            app.limits.hit(&key);
            Err(bad_link())
        }
    }
}

/// Uses a reset link: new password, logged out everywhere else, logged in here.
async fn reset_password(
    State(app): State<Arc<App>>,
    headers: HeaderMap,
    peer: Option<ConnectInfo<SocketAddr>>,
    Json(r): Json<ResetRequest>,
) -> Result<Response> {
    let key = format!("reset:{}", client_ip(&app, &headers, peer));
    if !app.limits.ok(&key, limits::BAD_RESETS) {
        return Err(slow_down());
    }
    check_password(&r.password).map_err(bad)?;
    if app.db.reset_user(r.token.trim()).map_err(oops)?.is_none() {
        app.limits.hit(&key);
        return Err(bad_link());
    }
    let hashed = hash_off_thread(r.password).await?;
    let Some(id) = app.db.use_reset(r.token.trim(), &hashed).map_err(oops)? else {
        return Err(bad_link());
    };
    let u = app.db.user(id).map_err(oops)?.ok_or_else(bad_link)?;
    let token = app.db.new_session(id).map_err(oops)?;
    Ok(with_session(&app, &token, Json(u)))
}

async fn logout(State(app): State<Arc<App>>, headers: HeaderMap) -> StatusCode {
    if let Some(t) = session(&headers) {
        let _ = app.db.end_session(&t);
    }
    StatusCode::NO_CONTENT
}

async fn learn_page(Path(slug): Path<String>) -> Response {
    match crate::docs::page(&slug) {
        Some(html) => Html(html).into_response(),
        None => (StatusCode::NOT_FOUND, Html("<!DOCTYPE html><p>There's no guide page with that name. <a href=\"/learn\">Back to Learn Brixo</a></p>")).into_response(),
    }
}

/// A sample game as a .brixo file, to drop onto Studio and read.
async fn sample_file(Path(file): Path<String>) -> Response {
    let (make, name): (fn() -> brixo_core::DataModel, &str) = match file.as_str() {
        "coin-tycoon.brixo" => (brixo_samples::coin_tycoon, "Coin Tycoon"),
        "flagfall.brixo" => (brixo_samples::flagfall, "Flagfall"),
        "spire-wars.brixo" => (brixo_samples::spire_wars, "Spire Wars"),
        "gear-range.brixo" => (brixo_samples::gears::gear_range, "Gear Range"),
        _ => return StatusCode::NOT_FOUND.into_response(),
    };
    match make().to_json() {
        Ok(json) => (
            [
                (header::CONTENT_TYPE, "application/octet-stream".to_string()),
                (header::CONTENT_DISPOSITION, format!("attachment; filename=\"{name}.brixo\"")),
            ],
            json,
        )
            .into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

/// A guide picture. The pages ask for them with a fingerprint of the
/// contents (?v=...), so they can be kept for good: a new picture is a new
/// address.
async fn learn_image(Path(file): Path<String>) -> Response {
    match crate::docs::image(&file) {
        Some(bytes) => ([(header::CONTENT_TYPE, "image/png"), (header::CACHE_CONTROL, "public, max-age=31536000, immutable")], bytes).into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
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
    // Taken down (or by a banned account): as if it isn't there.
    if app.db.game(game_id).map_err(oops)?.is_none() {
        return Err(ApiError(StatusCode::NOT_FOUND, "no such game".into()));
    }
    let data = app.db.game_data(game_id).map_err(oops)?.ok_or_else(|| ApiError(StatusCode::NOT_FOUND, "no such game".into()))?;
    let port = app
        .servers
        .port_for(game_id, app.network.ports, |port| {
            let model = DataModel::from_json(&data).map_err(std::io::Error::other)?;
            brixo_server::start_for_site(model, port, ticket_check(app.clone(), game_id), Arc::new(SiteSaves { app: app.clone(), game_id }))
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
        if app.db.is_banned(user_id).unwrap_or(true) {
            return None;
        }
        let u = app.db.user(user_id).ok()??;
        Some(brixo_server::Identity { name: u.username, look: look_of(&u.avatar), save_key: Some(u.id.to_string()) })
    })
}

/// Where a website game keeps its players' saved data (save/load): the
/// saves table, one row per game and account (the key is the account id).
struct SiteSaves {
    app: Arc<App>,
    game_id: i64,
}

impl brixo_runtime::SaveStore for SiteSaves {
    fn load(&self, key: &str) -> Option<String> {
        let user: i64 = key.parse().ok()?;
        self.app.db.player_save(self.game_id, user).ok().flatten()
    }
    fn save(&self, key: &str, data: &str) {
        if let Ok(user) = key.parse::<i64>() {
            let _ = self.app.db.set_player_save(self.game_id, user, data);
        }
    }
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
        for fine in ["Ann_2", "NoobSlayer", "noob", "Dickens", "Classy_Bass"] {
            assert!(check_username(fine).is_ok(), "{fine} was refused");
        }
        for bad in ["ab", "has space", "way_too_long_for_a_username", "shithead", "fuck_you", "k_y_s"] {
            assert!(check_username(bad).is_err(), "{bad} was allowed");
        }
    }

    #[test]
    fn passwords_hash_and_verify() {
        let h = hash("correct horse");
        assert!(verify("correct horse", &h) && !verify("wrong", &h));
    }
}

// --- admin -----------------------------------------------------------------

/// Only site admins (`brixo-web admin NAME` makes one).
fn admin(app: &App, headers: &HeaderMap) -> Result<User> {
    let u = user(app, headers)?;
    if !u.admin {
        return Err(ApiError(StatusCode::FORBIDDEN, "admins only".into()));
    }
    Ok(u)
}

#[derive(Serialize)]
struct AdminOverview {
    users: Vec<crate::db::AdminUser>,
    games: Vec<crate::db::AdminGame>,
}

async fn admin_overview(State(app): State<Arc<App>>, headers: HeaderMap) -> Result<Json<AdminOverview>> {
    admin(&app, &headers)?;
    Ok(Json(AdminOverview { users: app.db.admin_users(200).map_err(oops)?, games: app.db.admin_games(200).map_err(oops)? }))
}

#[derive(Deserialize)]
struct BanRequest {
    username: String,
    banned: bool,
}

/// Bans (or unbans) an account: logged out, kept out of games (and sent
/// out of any they're in), their games and profile hidden.
async fn admin_ban(State(app): State<Arc<App>>, headers: HeaderMap, Json(b): Json<BanRequest>) -> Result<StatusCode> {
    let me = admin(&app, &headers)?;
    if b.username.eq_ignore_ascii_case(&me.username) || b.username.eq_ignore_ascii_case("Brixo") {
        return Err(bad("you can't ban that account"));
    }
    match app.db.set_banned(&b.username, b.banned).map_err(oops)? {
        None => Err(ApiError(StatusCode::NOT_FOUND, "no such account".into())),
        Some(_) => {
            if b.banned {
                app.servers.kick_everywhere(&b.username);
            }
            Ok(StatusCode::NO_CONTENT)
        }
    }
}

#[derive(Deserialize)]
struct ResetFor {
    username: String,
}

/// Makes a one-time password reset link for an account (good for an
/// hour). The admin sends it to them however they like.
async fn admin_reset(State(app): State<Arc<App>>, headers: HeaderMap, Json(r): Json<ResetFor>) -> Result<Json<serde_json::Value>> {
    admin(&app, &headers)?;
    match app.db.new_reset(&r.username).map_err(oops)? {
        None => Err(ApiError(StatusCode::NOT_FOUND, "no such account".into())),
        Some((token, name)) => Ok(Json(serde_json::json!({ "token": token, "username": name, "minutes": crate::db::RESET_SECONDS / 60 }))),
    }
}

#[derive(Deserialize)]
struct HideRequest {
    id: i64,
    hidden: bool,
}

/// Takes a game down (or puts it back). A game taken down disappears from
/// the site and its server shuts.
async fn admin_hide(State(app): State<Arc<App>>, headers: HeaderMap, Json(h): Json<HideRequest>) -> Result<StatusCode> {
    admin(&app, &headers)?;
    if !app.db.set_hidden(h.id, h.hidden).map_err(oops)? {
        return Err(ApiError(StatusCode::NOT_FOUND, "no such game".into()));
    }
    if h.hidden {
        app.servers.stop(h.id);
    }
    Ok(StatusCode::NO_CONTENT)
}

