//! What the website remembers: accounts, avatars, sessions and games.
//! One SQLite file behind a mutex: simple, and plenty for now.

use std::sync::Mutex;

use rand::RngCore;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

pub struct Db(Mutex<Connection>);

/// A consistent copy of the database at `db_path` in `out` (which mustn't
/// exist yet), safe while the website runs: SQLite's own `VACUUM INTO`, not
/// a file copy (a copy taken mid-write can be broken). The database is
/// opened as it is, without the website's upgrades, so a backup taken
/// before a deploy is exactly what the old version left. Then checks the
/// copy opens and is sound, and gives back (users, games) in it.
pub fn backup(db_path: &str, out: &str) -> Result<(i64, i64), String> {
    if !std::path::Path::new(db_path).exists() {
        return Err(format!("there's no database at {db_path}"));
    }
    if std::path::Path::new(out).exists() {
        return Err(format!("{out} already exists"));
    }
    {
        let conn = Connection::open_with_flags(db_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE).map_err(|e| format!("couldn't open {db_path}: {e}"))?;
        conn.busy_timeout(std::time::Duration::from_secs(30)).map_err(|e| e.to_string())?;
        conn.execute("VACUUM INTO ?1", params![out]).map_err(|e| format!("backup failed: {e}"))?;
    }
    let copy = Connection::open(out).map_err(|e| format!("the backup won't open: {e}"))?;
    let check: String = copy.query_row("PRAGMA integrity_check", [], |r| r.get(0)).map_err(|e| format!("couldn't check the backup: {e}"))?;
    if check != "ok" {
        return Err(format!("the backup failed its check: {check}"));
    }
    let users: i64 = copy.query_row("SELECT COUNT(*) FROM users", [], |r| r.get(0)).map_err(|e| e.to_string())?;
    let games: i64 = copy.query_row("SELECT COUNT(*) FROM games", [], |r| r.get(0)).map_err(|e| e.to_string())?;
    Ok((users, games))
}

pub type Rgb = (u8, u8, u8);

#[derive(Debug, Clone, PartialEq, Serialize, serde::Deserialize)]
pub struct Avatar {
    pub skin: Rgb,
    pub shirt: Rgb,
    pub pants: Rgb,
    /// Still saved (games' scripts can read shoes_color), but not drawn.
    pub shoes: Rgb,
    pub face: String,
    /// The accessories they wear, by name (one per slot).
    #[serde(default)]
    pub hats: Vec<String>,
    /// Each body part's colour: head, torso, left arm, right arm, left leg,
    /// right leg. None for avatars from before body colours (skin, with the
    /// torso in the shirt colour).
    #[serde(default)]
    pub body: Option<[Rgb; 6]>,
    /// Which shirt, pants and t-shirt picture ("" for none) they wear, by
    /// name. The colours above are what the shirt and pants are worn in.
    #[serde(default = "default_shirt")]
    pub shirt_style: String,
    #[serde(default = "default_pants")]
    pub pants_style: String,
    #[serde(default)]
    pub tshirt: String,
}

fn default_shirt() -> String {
    "tee".into()
}

fn default_pants() -> String {
    "plain".into()
}

impl Default for Avatar {
    fn default() -> Self {
        let skin = (227, 185, 138);
        Avatar {
            skin,
            shirt: (13, 105, 172),
            pants: (27, 42, 53),
            shoes: (27, 27, 27),
            face: "smile".into(),
            hats: Vec::new(),
            body: Some([skin, (13, 105, 172), skin, skin, skin, skin]),
            shirt_style: default_shirt(),
            pants_style: default_pants(),
            tshirt: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct User {
    pub id: i64,
    pub username: String,
    pub avatar: Avatar,
    /// A line or two about themselves, shown on their profile.
    pub blurb: String,
    /// When they joined (Unix seconds; 0 for accounts from before this was kept).
    pub created: i64,
    /// Can use the admin page (ban accounts, take games down).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub admin: bool,
    /// When they were last on the website (Unix seconds; 0 if never seen).
    #[serde(skip)]
    pub last_seen: i64,
    /// Their Brix (only they see this).
    pub brix: i64,
}

/// An account, as the admin page lists it.
#[derive(Debug, Clone, Serialize)]
pub struct AdminUser {
    pub username: String,
    pub created: i64,
    pub banned: bool,
    pub admin: bool,
    pub games: i64,
}

/// A game, as the admin page lists it.
#[derive(Debug, Clone, Serialize)]
pub struct AdminGame {
    pub id: i64,
    pub name: String,
    pub owner: String,
    pub visits: i64,
    pub created: i64,
    pub hidden: bool,
    pub owner_banned: bool,
    /// Only admins can see and play it (the Test Lab).
    pub admin_only: bool,
    /// In the big banner on Home and Games.
    pub featured: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct GameRow {
    pub id: i64,
    pub name: String,
    pub owner: String,
    pub description: String,
    /// How many times someone has pressed Play.
    pub visits: i64,
    /// When it was first published (Unix seconds; 0 if unknown).
    pub created: i64,
    pub has_thumbnail: bool,
    /// Picked by an admin for the big banner on Home and Games.
    pub featured: bool,
}

/// A toolbox item, as the Toolbox lists it (without what it's made of).
#[derive(Debug, Clone, Serialize)]
pub struct ToolboxRow {
    pub id: i64,
    pub name: String,
    pub category: String,
    pub description: String,
    pub has_thumbnail: bool,
    /// One of Brixo's own (seeded from the code).
    pub builtin: bool,
}

/// Someone on a friends list (or asking to be).
#[derive(Debug, Clone, Serialize)]
pub struct FriendRow {
    pub username: String,
    pub avatar: Avatar,
    /// When they were last on the website (Unix seconds; 0 if never seen).
    pub last_seen: i64,
}

/// How two accounts are related.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Relation {
    None,
    Friends,
    /// You asked them.
    Sent,
    /// They asked you.
    Received,
}

/// What asking someone to be friends did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Asked {
    Sent,
    /// They'd already asked you: now you're friends.
    NowFriends,
    AlreadyFriends,
    AlreadySent,
    /// You have as many friends as you can.
    Full,
}

/// Most friends an account can have.
pub const MAX_FRIENDS: i64 = 200;

pub fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

/// Games anyone can see and play: not taken down, and not by a banned account.
const LISTED: &str = "g.hidden = 0 AND u.banned = 0 AND g.admin_only = 0";

const GAME_COLUMNS: &str = "g.id, g.name, u.username, g.description, g.visits, g.created, g.thumbnail IS NOT NULL, g.featured";

fn game_from_row(r: &rusqlite::Row) -> rusqlite::Result<GameRow> {
    Ok(GameRow {
        id: r.get(0)?,
        name: r.get(1)?,
        owner: r.get(2)?,
        description: r.get(3)?,
        visits: r.get(4)?,
        created: r.get(5)?,
        has_thumbnail: r.get(6)?,
        featured: r.get::<_, i64>(7)? != 0,
    })
}

/// How long a login lasts before you have to log in again.
pub const SESSION_DAYS: i64 = 30;
/// How long a password reset link works, in seconds.
pub const RESET_SECONDS: i64 = 3600;

/// Invite codes use letters and digits that can't be mixed up (no O/0,
/// I/1/L), shown as XXXX-XXXX-XXXX.
const INVITE_ALPHABET: &[u8] = b"ABCDEFGHJKMNPQRSTUVWXYZ23456789";

fn new_invite_code() -> String {
    let mut bytes = [0u8; 12];
    rand::thread_rng().fill_bytes(&mut bytes);
    let chars: String = bytes.iter().map(|b| INVITE_ALPHABET[*b as usize % INVITE_ALPHABET.len()] as char).collect();
    format!("{}-{}-{}", &chars[0..4], &chars[4..8], &chars[8..12])
}

/// "brix-4f9k 2qxm" and "BRIX4F9K2QXM" are the same code as "BRIX-4F9K-2QXM".
pub fn normalize_invite(code: &str) -> String {
    let c: String = code.chars().filter(|c| c.is_ascii_alphanumeric()).map(|c| c.to_ascii_uppercase()).collect();
    if c.len() == 12 { format!("{}-{}-{}", &c[0..4], &c[4..8], &c[8..12]) } else { c }
}

/// A random hex token, for sessions and tickets.
pub fn random_token() -> String {
    let mut bytes = [0u8; 24];
    rand::thread_rng().fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

const USER_COLUMNS: &str = "id, username, avatar, blurb, created, admin, last_seen, brix";

fn user_from_row(r: &rusqlite::Row) -> rusqlite::Result<User> {
    let avatar: String = r.get(2)?;
    Ok(User {
        id: r.get(0)?,
        username: r.get(1)?,
        avatar: serde_json::from_str(&avatar).unwrap_or_default(),
        blurb: r.get(3)?,
        created: r.get(4)?,
        admin: r.get::<_, i64>(5)? != 0,
        last_seen: r.get(6)?,
        brix: r.get(7)?,
    })
}

/// Adds a column to an existing database if it isn't there yet (databases
/// made before the column existed keep working, with the default filled in).
fn add_column(conn: &Connection, table: &str, column: &str, decl: &str) -> rusqlite::Result<()> {
    let exists = conn
        .prepare(&format!("PRAGMA table_info({table})"))?
        .query_map([], |r| r.get::<_, String>(1))?
        .filter_map(|c| c.ok())
        .any(|c| c == column);
    if !exists {
        conn.execute_batch(&format!("ALTER TABLE {table} ADD COLUMN {column} {decl}"))?;
    }
    Ok(())
}

impl Db {
    /// Opens (or creates) the database. ":memory:" makes a throwaway one.
    pub fn open(path: &str) -> rusqlite::Result<Db> {
        let conn = Connection::open(path)?;
        // The website and its command-line tools (invites, passwords) can
        // use the file at the same time: wait for each other, don't fail.
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS users (
                 id INTEGER PRIMARY KEY,
                 username TEXT NOT NULL UNIQUE COLLATE NOCASE,
                 password_hash TEXT NOT NULL,
                 avatar TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS sessions (
                 token TEXT PRIMARY KEY,
                 user_id INTEGER NOT NULL REFERENCES users(id)
             );
             CREATE TABLE IF NOT EXISTS games (
                 id INTEGER PRIMARY KEY,
                 name TEXT NOT NULL,
                 owner_id INTEGER NOT NULL REFERENCES users(id),
                 data TEXT NOT NULL,
                 UNIQUE(name, owner_id)
             );
             CREATE TABLE IF NOT EXISTS saves (
                 game_id INTEGER NOT NULL REFERENCES games(id),
                 user_id INTEGER NOT NULL REFERENCES users(id),
                 data TEXT NOT NULL,
                 updated INTEGER NOT NULL,
                 PRIMARY KEY (game_id, user_id)
             );
             CREATE TABLE IF NOT EXISTS invites (
                 code TEXT PRIMARY KEY,
                 created INTEGER NOT NULL,
                 used_by INTEGER REFERENCES users(id),
                 used_at INTEGER
             );",
        )?;
        add_column(&conn, "sessions", "created", "INTEGER NOT NULL DEFAULT 0")?;
        conn.execute("DELETE FROM sessions WHERE created < ?1", [now() - SESSION_DAYS * 86400])?;
        add_column(&conn, "users", "blurb", "TEXT NOT NULL DEFAULT ''")?;
        add_column(&conn, "users", "created", "INTEGER NOT NULL DEFAULT 0")?;
        add_column(&conn, "games", "description", "TEXT NOT NULL DEFAULT ''")?;
        add_column(&conn, "games", "visits", "INTEGER NOT NULL DEFAULT 0")?;
        add_column(&conn, "games", "created", "INTEGER NOT NULL DEFAULT 0")?;
        add_column(&conn, "games", "thumbnail", "BLOB")?;
        // Moderation: banned accounts, site admins, and games taken down.
        add_column(&conn, "users", "banned", "INTEGER NOT NULL DEFAULT 0")?;
        add_column(&conn, "users", "admin", "INTEGER NOT NULL DEFAULT 0")?;
        add_column(&conn, "games", "hidden", "INTEGER NOT NULL DEFAULT 0")?;
        // Games only admins can see and play (the Test Lab).
        add_column(&conn, "games", "admin_only", "INTEGER NOT NULL DEFAULT 0")?;
        // Password reset links an admin makes (one per account, one use).
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS resets (
                 token TEXT PRIMARY KEY,
                 user_id INTEGER NOT NULL UNIQUE REFERENCES users(id),
                 created INTEGER NOT NULL
             );",
        )?;
        // Friends: pairs stored once (lower id first), and requests waiting
        // for an answer. last_seen says who's on the website right now.
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS friends (
                 a INTEGER NOT NULL REFERENCES users(id),
                 b INTEGER NOT NULL REFERENCES users(id),
                 since INTEGER NOT NULL,
                 PRIMARY KEY (a, b)
             );
             CREATE TABLE IF NOT EXISTS friend_requests (
                 from_id INTEGER NOT NULL REFERENCES users(id),
                 to_id INTEGER NOT NULL REFERENCES users(id),
                 created INTEGER NOT NULL,
                 PRIMARY KEY (from_id, to_id)
             );",
        )?;
        add_column(&conn, "users", "last_seen", "INTEGER NOT NULL DEFAULT 0")?;
        // The game an admin picked for the banner on Home and Games, and
        // what each player last played (Continue Playing on Home).
        add_column(&conn, "games", "featured", "INTEGER NOT NULL DEFAULT 0")?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS plays (
                 user_id INTEGER NOT NULL REFERENCES users(id),
                 game_id INTEGER NOT NULL REFERENCES games(id),
                 at INTEGER NOT NULL,
                 PRIMARY KEY (user_id, game_id)
             );",
        )?;
        // The toolbox: ready-made things Studio can insert. Built-in ones
        // have a slug (seeded from brixo_samples::toolbox, kept up to date);
        // admins add the rest. Removing one hides it (so a removed built-in
        // stays removed).
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS toolbox (
                 id INTEGER PRIMARY KEY,
                 slug TEXT UNIQUE,
                 name TEXT NOT NULL,
                 category TEXT NOT NULL,
                 description TEXT NOT NULL DEFAULT '',
                 content TEXT NOT NULL,
                 thumbnail BLOB,
                 removed INTEGER NOT NULL DEFAULT 0,
                 created INTEGER NOT NULL DEFAULT 0
             );",
        )?;
        // Brix and the Catalog: what everyone owns, admins' price changes,
        // a record of every Brix given or spent, the challenges games have
        // (and who's done them), saved outfits, and one-off upgrades done.
        add_column(&conn, "users", "brix", "INTEGER NOT NULL DEFAULT 0")?;
        add_column(&conn, "users", "bonus_day", "INTEGER NOT NULL DEFAULT 0")?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS owned (
                 user_id INTEGER NOT NULL REFERENCES users(id),
                 item TEXT NOT NULL,
                 at INTEGER NOT NULL,
                 PRIMARY KEY (user_id, item)
             );
             CREATE TABLE IF NOT EXISTS prices (
                 item TEXT PRIMARY KEY,
                 price INTEGER NOT NULL
             );
             CREATE TABLE IF NOT EXISTS brix_log (
                 id INTEGER PRIMARY KEY,
                 user_id INTEGER NOT NULL REFERENCES users(id),
                 amount INTEGER NOT NULL,
                 why TEXT NOT NULL,
                 at INTEGER NOT NULL
             );
             CREATE INDEX IF NOT EXISTS brix_log_user ON brix_log (user_id, at);
             CREATE TABLE IF NOT EXISTS challenges (
                 id INTEGER PRIMARY KEY,
                 game_id INTEGER NOT NULL REFERENCES games(id),
                 name TEXT NOT NULL,
                 title TEXT NOT NULL,
                 reward INTEGER NOT NULL DEFAULT 0,
                 daily INTEGER NOT NULL DEFAULT 0,
                 approved INTEGER NOT NULL DEFAULT 0,
                 created INTEGER NOT NULL,
                 UNIQUE (game_id, name)
             );
             CREATE TABLE IF NOT EXISTS challenge_done (
                 user_id INTEGER NOT NULL REFERENCES users(id),
                 challenge_id INTEGER NOT NULL REFERENCES challenges(id),
                 day INTEGER NOT NULL,
                 at INTEGER NOT NULL,
                 PRIMARY KEY (user_id, challenge_id, day)
             );
             CREATE TABLE IF NOT EXISTS outfits (
                 user_id INTEGER NOT NULL REFERENCES users(id),
                 slot INTEGER NOT NULL,
                 name TEXT NOT NULL,
                 avatar TEXT NOT NULL,
                 PRIMARY KEY (user_id, slot)
             );
             CREATE TABLE IF NOT EXISTS meta (
                 key TEXT PRIMARY KEY,
                 value TEXT NOT NULL
             );",
        )?;
        Ok(Db(Mutex::new(conn)))
    }

    /// Fails if the name is taken (in any capitalisation).
    pub fn create_user(&self, username: &str, password_hash: &str) -> rusqlite::Result<User> {
        let conn = self.0.lock().unwrap();
        let avatar = Avatar::default();
        let created = now();
        conn.execute(
            "INSERT INTO users (username, password_hash, avatar, created, brix) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![username, password_hash, serde_json::to_string(&avatar).unwrap(), created, crate::shop::WELCOME_BRIX],
        )?;
        Ok(User { id: conn.last_insert_rowid(), username: username.to_string(), avatar, blurb: String::new(), created, admin: false, last_seen: 0, brix: crate::shop::WELCOME_BRIX })
    }

    /// Makes an account with an invite code, using the code up. Ok(None)
    /// means the code is wrong or already used; an error means the name
    /// is taken.
    pub fn create_user_invited(&self, username: &str, password_hash: &str, code: &str) -> rusqlite::Result<Option<User>> {
        let mut conn = self.0.lock().unwrap();
        let tx = conn.transaction()?;
        let code = normalize_invite(code);
        let unused: bool = tx
            .query_row("SELECT 1 FROM invites WHERE code = ?1 AND used_by IS NULL", [&code], |_| Ok(true))
            .optional()?
            .unwrap_or(false);
        if !unused {
            return Ok(None);
        }
        let avatar = Avatar::default();
        let created = now();
        tx.execute(
            "INSERT INTO users (username, password_hash, avatar, created, brix) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![username, password_hash, serde_json::to_string(&avatar).unwrap(), created, crate::shop::WELCOME_BRIX],
        )?;
        let id = tx.last_insert_rowid();
        tx.execute("UPDATE invites SET used_by = ?1, used_at = ?2 WHERE code = ?3", params![id, created, code])?;
        tx.commit()?;
        Ok(Some(User { id, username: username.to_string(), avatar, blurb: String::new(), created, admin: false, last_seen: 0, brix: crate::shop::WELCOME_BRIX }))
    }

    /// Makes `count` new invite codes.
    pub fn create_invites(&self, count: usize) -> rusqlite::Result<Vec<String>> {
        let conn = self.0.lock().unwrap();
        let mut codes = Vec::new();
        while codes.len() < count {
            let code = new_invite_code();
            if conn.execute("INSERT OR IGNORE INTO invites (code, created) VALUES (?1, ?2)", params![code, now()])? == 1 {
                codes.push(code);
            }
        }
        Ok(codes)
    }

    /// Every invite code: (code, who used it, if anyone).
    pub fn invites(&self) -> rusqlite::Result<Vec<(String, Option<String>)>> {
        let conn = self.0.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT i.code, u.username FROM invites i LEFT JOIN users u ON u.id = i.used_by ORDER BY i.created, i.code",
        )?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect();
        rows
    }

    /// Changes someone's password and logs them out everywhere.
    /// False if there's no such user.
    pub fn set_password(&self, username: &str, password_hash: &str) -> rusqlite::Result<bool> {
        let conn = self.0.lock().unwrap();
        let Some(id) = conn
            .query_row("SELECT id FROM users WHERE username = ?1", [username], |r| r.get::<_, i64>(0))
            .optional()?
        else {
            return Ok(false);
        };
        conn.execute("UPDATE users SET password_hash = ?1 WHERE id = ?2", params![password_hash, id])?;
        conn.execute("DELETE FROM sessions WHERE user_id = ?1", [id])?;
        Ok(true)
    }

    /// A password reset link for someone (replacing any earlier one).
    /// None if there's no such account.
    pub fn new_reset(&self, username: &str) -> rusqlite::Result<Option<(String, String)>> {
        let conn = self.0.lock().unwrap();
        let Some((id, name)) = conn
            .query_row("SELECT id, username FROM users WHERE username = ?1", [username], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))
            .optional()?
        else {
            return Ok(None);
        };
        let token = random_token();
        conn.execute("DELETE FROM resets WHERE user_id = ?1 OR created < ?2", params![id, now() - RESET_SECONDS])?;
        conn.execute("INSERT INTO resets (token, user_id, created) VALUES (?1, ?2, ?3)", params![token, id, now()])?;
        Ok(Some((token, name)))
    }

    /// Whose reset link this is, if it's still good.
    pub fn reset_user(&self, token: &str) -> rusqlite::Result<Option<String>> {
        let conn = self.0.lock().unwrap();
        conn.query_row(
            "SELECT u.username FROM resets r JOIN users u ON u.id = r.user_id WHERE r.token = ?1 AND r.created >= ?2 AND u.banned = 0",
            params![token, now() - RESET_SECONDS],
            |r| r.get(0),
        )
        .optional()
    }

    /// Uses a reset link: sets the new password, logs the account out
    /// everywhere, and uses the link up. Gives back the account's id.
    pub fn use_reset(&self, token: &str, password_hash: &str) -> rusqlite::Result<Option<i64>> {
        let conn = self.0.lock().unwrap();
        let Some(id) = conn
            .query_row(
                "SELECT r.user_id FROM resets r JOIN users u ON u.id = r.user_id WHERE r.token = ?1 AND r.created >= ?2 AND u.banned = 0",
                params![token, now() - RESET_SECONDS],
                |r| r.get::<_, i64>(0),
            )
            .optional()?
        else {
            return Ok(None);
        };
        conn.execute("UPDATE users SET password_hash = ?1 WHERE id = ?2", params![password_hash, id])?;
        conn.execute("DELETE FROM sessions WHERE user_id = ?1", [id])?;
        conn.execute("DELETE FROM resets WHERE user_id = ?1", [id])?;
        Ok(Some(id))
    }

    /// Changes a logged-in account's password and logs out its other
    /// sessions (every one but `keep`).
    pub fn change_password(&self, user_id: i64, password_hash: &str, keep: &str) -> rusqlite::Result<()> {
        let conn = self.0.lock().unwrap();
        conn.execute("UPDATE users SET password_hash = ?1 WHERE id = ?2", params![password_hash, user_id])?;
        conn.execute("DELETE FROM sessions WHERE user_id = ?1 AND token != ?2", params![user_id, keep])?;
        conn.execute("DELETE FROM resets WHERE user_id = ?1", [user_id])?;
        Ok(())
    }

    pub fn password_hash(&self, user_id: i64) -> rusqlite::Result<Option<String>> {
        let conn = self.0.lock().unwrap();
        conn.query_row("SELECT password_hash FROM users WHERE id = ?1", [user_id], |r| r.get(0)).optional()
    }

    pub fn login_info(&self, username: &str) -> rusqlite::Result<Option<(i64, String)>> {
        let conn = self.0.lock().unwrap();
        conn.query_row("SELECT id, password_hash FROM users WHERE username = ?1", [username], |r| Ok((r.get(0)?, r.get(1)?)))
            .optional()
    }

    pub fn user(&self, id: i64) -> rusqlite::Result<Option<User>> {
        let conn = self.0.lock().unwrap();
        conn.query_row(&format!("SELECT {USER_COLUMNS} FROM users WHERE id = ?1"), [id], user_from_row).optional()
    }

    pub fn user_by_name(&self, name: &str) -> rusqlite::Result<Option<User>> {
        let conn = self.0.lock().unwrap();
        conn.query_row(&format!("SELECT {USER_COLUMNS} FROM users WHERE username = ?1 AND banned = 0"), [name], user_from_row).optional()
    }

    pub fn is_banned(&self, id: i64) -> rusqlite::Result<bool> {
        let conn = self.0.lock().unwrap();
        Ok(conn.query_row("SELECT banned FROM users WHERE id = ?1", [id], |r| r.get::<_, i64>(0)).optional()?.unwrap_or(0) != 0)
    }

    /// Bans or unbans an account (a ban also logs them out everywhere).
    /// Gives back their id, or None if there's no such account.
    pub fn set_banned(&self, username: &str, banned: bool) -> rusqlite::Result<Option<i64>> {
        let conn = self.0.lock().unwrap();
        let Some(id) = conn.query_row("SELECT id FROM users WHERE username = ?1", [username], |r| r.get::<_, i64>(0)).optional()? else {
            return Ok(None);
        };
        conn.execute("UPDATE users SET banned = ?1 WHERE id = ?2", params![banned as i64, id])?;
        if banned {
            conn.execute("DELETE FROM sessions WHERE user_id = ?1", [id])?;
        }
        Ok(Some(id))
    }

    /// Makes someone a site admin (or not). False if there's no such account.
    pub fn set_admin(&self, username: &str, admin: bool) -> rusqlite::Result<bool> {
        let conn = self.0.lock().unwrap();
        Ok(conn.execute("UPDATE users SET admin = ?1 WHERE username = ?2", params![admin as i64, username])? > 0)
    }

    /// Takes a game down (or puts it back). False if there's no such game.
    pub fn set_hidden(&self, game_id: i64, hidden: bool) -> rusqlite::Result<bool> {
        let conn = self.0.lock().unwrap();
        Ok(conn.execute("UPDATE games SET hidden = ?1 WHERE id = ?2", params![hidden as i64, game_id])? > 0)
    }

    /// Makes a game admins-only (or public again). False if there's no such game.
    pub fn set_admin_only(&self, game_id: i64, admin_only: bool) -> rusqlite::Result<bool> {
        let conn = self.0.lock().unwrap();
        Ok(conn.execute("UPDATE games SET admin_only = ?1 WHERE id = ?2", params![admin_only as i64, game_id])? > 0)
    }

    /// Whether an admin can play this game: it exists and isn't taken
    /// down (admins-only games included).
    pub fn admin_can_play(&self, game_id: i64) -> rusqlite::Result<bool> {
        let conn = self.0.lock().unwrap();
        Ok(conn.query_row("SELECT COUNT(*) FROM games WHERE id = ?1 AND hidden = 0", [game_id], |r| r.get::<_, i64>(0))? > 0)
    }

    /// For the admin page: the newest accounts, with their game counts.
    pub fn admin_users(&self, limit: i64) -> rusqlite::Result<Vec<AdminUser>> {
        let conn = self.0.lock().unwrap();
        let mut q = conn.prepare(
            "SELECT u.username, u.created, u.banned, u.admin, (SELECT COUNT(*) FROM games g WHERE g.owner_id = u.id)
             FROM users u ORDER BY u.id DESC LIMIT ?1",
        )?;
        let rows = q.query_map([limit], |r| {
            Ok(AdminUser { username: r.get(0)?, created: r.get(1)?, banned: r.get::<_, i64>(2)? != 0, admin: r.get::<_, i64>(3)? != 0, games: r.get(4)? })
        })?;
        rows.collect()
    }

    /// For the admin page: the newest games, taken down or not.
    pub fn admin_games(&self, limit: i64) -> rusqlite::Result<Vec<AdminGame>> {
        let conn = self.0.lock().unwrap();
        let mut q = conn.prepare(
            "SELECT g.id, g.name, u.username, g.visits, g.created, g.hidden, u.banned, g.admin_only, g.featured FROM games g JOIN users u ON u.id = g.owner_id
             ORDER BY g.id DESC LIMIT ?1",
        )?;
        let rows = q.query_map([limit], |r| {
            Ok(AdminGame {
                id: r.get(0)?,
                name: r.get(1)?,
                owner: r.get(2)?,
                visits: r.get(3)?,
                created: r.get(4)?,
                hidden: r.get::<_, i64>(5)? != 0,
                owner_banned: r.get::<_, i64>(6)? != 0,
                admin_only: r.get::<_, i64>(7)? != 0,
                featured: r.get::<_, i64>(8)? != 0,
            })
        })?;
        rows.collect()
    }

    pub fn new_session(&self, user_id: i64) -> rusqlite::Result<String> {
        let token = random_token();
        self.0.lock().unwrap().execute(
            "INSERT INTO sessions (token, user_id, created) VALUES (?1, ?2, ?3)",
            params![token, user_id, now()],
        )?;
        Ok(token)
    }

    pub fn session_user(&self, token: &str) -> rusqlite::Result<Option<User>> {
        let conn = self.0.lock().unwrap();
        conn.query_row(
            "SELECT u.id, u.username, u.avatar, u.blurb, u.created, u.admin, u.last_seen, u.brix FROM sessions s JOIN users u ON u.id = s.user_id
             WHERE s.token = ?1 AND s.created >= ?2 AND u.banned = 0",
            params![token, now() - SESSION_DAYS * 86400],
            user_from_row,
        )
        .optional()
    }

    pub fn end_session(&self, token: &str) -> rusqlite::Result<()> {
        self.0.lock().unwrap().execute("DELETE FROM sessions WHERE token = ?1", [token])?;
        Ok(())
    }

    pub fn set_avatar(&self, user_id: i64, avatar: &Avatar) -> rusqlite::Result<()> {
        self.0.lock().unwrap().execute(
            "UPDATE users SET avatar = ?1 WHERE id = ?2",
            params![serde_json::to_string(avatar).unwrap(), user_id],
        )?;
        Ok(())
    }

    pub fn set_blurb(&self, user_id: i64, blurb: &str) -> rusqlite::Result<()> {
        self.0.lock().unwrap().execute("UPDATE users SET blurb = ?1 WHERE id = ?2", params![blurb, user_id])?;
        Ok(())
    }

    /// Publishing again under the same name replaces your earlier version
    /// (keeping its visits, description and picture).
    pub fn publish(&self, owner_id: i64, name: &str, data: &str) -> rusqlite::Result<i64> {
        let conn = self.0.lock().unwrap();
        conn.execute(
            "INSERT INTO games (name, owner_id, data, created) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(name, owner_id) DO UPDATE SET data = excluded.data",
            params![name, owner_id, data, now()],
        )?;
        conn.query_row("SELECT id FROM games WHERE name = ?1 AND owner_id = ?2", params![name, owner_id], |r| r.get(0))
    }

    pub fn games(&self) -> rusqlite::Result<Vec<GameRow>> {
        let conn = self.0.lock().unwrap();
        let mut q = conn.prepare(&format!("SELECT {GAME_COLUMNS} FROM games g JOIN users u ON u.id = g.owner_id WHERE {LISTED} ORDER BY g.id"))?;
        let rows = q.query_map([], game_from_row)?;
        rows.collect()
    }

    pub fn game(&self, id: i64) -> rusqlite::Result<Option<GameRow>> {
        let conn = self.0.lock().unwrap();
        conn.query_row(&format!("SELECT {GAME_COLUMNS} FROM games g JOIN users u ON u.id = g.owner_id WHERE g.id = ?1 AND {LISTED}"), [id], game_from_row)
            .optional()
    }

    pub fn games_by(&self, owner_id: i64) -> rusqlite::Result<Vec<GameRow>> {
        let conn = self.0.lock().unwrap();
        let mut q = conn.prepare(&format!(
            "SELECT {GAME_COLUMNS} FROM games g JOIN users u ON u.id = g.owner_id WHERE g.owner_id = ?1 AND {LISTED} ORDER BY g.id"
        ))?;
        let rows = q.query_map([owner_id], game_from_row)?;
        rows.collect()
    }

    /// Only the game's owner can change these.
    pub fn set_game_info(&self, id: i64, owner_id: i64, description: &str) -> rusqlite::Result<bool> {
        let n = self.0.lock().unwrap().execute(
            "UPDATE games SET description = ?1 WHERE id = ?2 AND owner_id = ?3",
            params![description, id, owner_id],
        )?;
        Ok(n > 0)
    }

    pub fn set_thumbnail(&self, id: i64, png: &[u8]) -> rusqlite::Result<()> {
        self.0.lock().unwrap().execute("UPDATE games SET thumbnail = ?1 WHERE id = ?2", params![png, id])?;
        Ok(())
    }

    pub fn thumbnail(&self, id: i64) -> rusqlite::Result<Option<Vec<u8>>> {
        let conn = self.0.lock().unwrap();
        Ok(conn.query_row("SELECT thumbnail FROM games WHERE id = ?1", [id], |r| r.get::<_, Option<Vec<u8>>>(0)).optional()?.flatten())
    }

    /// Puts a game in the banner on Home and Games (only one at a time), or
    /// takes it out. False if there's no such game.
    pub fn set_featured(&self, id: i64, featured: bool) -> rusqlite::Result<bool> {
        let conn = self.0.lock().unwrap();
        if featured {
            conn.execute("UPDATE games SET featured = 0 WHERE featured = 1 AND id != ?1", [id])?;
        }
        Ok(conn.execute("UPDATE games SET featured = ?1 WHERE id = ?2", params![featured as i64, id])? > 0)
    }

    /// Notes that someone pressed Play on a game (for Continue Playing).
    pub fn played(&self, user_id: i64, game_id: i64) -> rusqlite::Result<()> {
        self.0.lock().unwrap().execute(
            "INSERT INTO plays (user_id, game_id, at) VALUES (?1, ?2, ?3) ON CONFLICT(user_id, game_id) DO UPDATE SET at = excluded.at",
            params![user_id, game_id, now()],
        )?;
        Ok(())
    }

    /// The games someone played last, newest first (listed ones only).
    pub fn recently_played(&self, user_id: i64, limit: i64) -> rusqlite::Result<Vec<GameRow>> {
        let conn = self.0.lock().unwrap();
        let mut q = conn.prepare(&format!(
            "SELECT {GAME_COLUMNS} FROM plays p JOIN games g ON g.id = p.game_id JOIN users u ON u.id = g.owner_id
             WHERE p.user_id = ?1 AND {LISTED} ORDER BY p.at DESC, g.id DESC LIMIT ?2"
        ))?;
        let rows = q.query_map(params![user_id, limit], game_from_row)?;
        rows.collect()
    }

    pub fn add_visit(&self, id: i64) -> rusqlite::Result<()> {
        self.0.lock().unwrap().execute("UPDATE games SET visits = visits + 1 WHERE id = ?1", [id])?;
        Ok(())
    }

    /// (accounts, games) on the site.
    pub fn counts(&self) -> rusqlite::Result<(i64, i64)> {
        let conn = self.0.lock().unwrap();
        let users: i64 = conn.query_row("SELECT COUNT(*) FROM users WHERE username != 'Brixo'", [], |r| r.get(0))?;
        let games: i64 = conn.query_row(&format!("SELECT COUNT(*) FROM games g JOIN users u ON u.id = g.owner_id WHERE {LISTED}"), [], |r| r.get(0))?;
        Ok((users, games))
    }

    /// A player's saved data in a game (what its scripts save()d), as JSON.
    pub fn player_save(&self, game_id: i64, user_id: i64) -> rusqlite::Result<Option<String>> {
        let conn = self.0.lock().unwrap();
        conn.query_row("SELECT data FROM saves WHERE game_id = ?1 AND user_id = ?2", [game_id, user_id], |r| r.get(0)).optional()
    }

    pub fn set_player_save(&self, game_id: i64, user_id: i64, data: &str) -> rusqlite::Result<()> {
        self.0.lock().unwrap().execute(
            "INSERT INTO saves (game_id, user_id, data, updated) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(game_id, user_id) DO UPDATE SET data = excluded.data, updated = excluded.updated",
            params![game_id, user_id, data, now()],
        )?;
        Ok(())
    }

    /// The saved game itself. Only the website and its game servers read
    /// this: players never download it.
    pub fn game_data(&self, id: i64) -> rusqlite::Result<Option<String>> {
        let conn = self.0.lock().unwrap();
        conn.query_row("SELECT data FROM games WHERE id = ?1", [id], |r| r.get(0)).optional()
    }

    /// Adds or updates a built-in toolbox item (by its slug). One that an
    /// admin removed stays removed.
    pub fn seed_toolbox(&self, slug: &str, name: &str, category: &str, description: &str, content: &str, thumbnail: &[u8]) -> rusqlite::Result<()> {
        self.0.lock().unwrap().execute(
            "INSERT INTO toolbox (slug, name, category, description, content, thumbnail, created) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(slug) DO UPDATE SET name = ?2, category = ?3, description = ?4, content = ?5, thumbnail = ?6",
            params![slug, name, category, description, content, thumbnail, now()],
        )?;
        Ok(())
    }

    /// Adds an item to the toolbox (an admin's). Gives back its id.
    pub fn add_toolbox(&self, name: &str, category: &str, description: &str, content: &str, thumbnail: Option<&[u8]>) -> rusqlite::Result<i64> {
        let conn = self.0.lock().unwrap();
        conn.execute(
            "INSERT INTO toolbox (name, category, description, content, thumbnail, created) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![name, category, description, content, thumbnail, now()],
        )?;
        Ok(conn.last_insert_rowid())
    }

    /// Everything in the toolbox (not removed), newest last.
    pub fn toolbox(&self) -> rusqlite::Result<Vec<ToolboxRow>> {
        let conn = self.0.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, category, description, thumbnail IS NOT NULL, slug IS NOT NULL FROM toolbox WHERE removed = 0 ORDER BY id",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(ToolboxRow { id: r.get(0)?, name: r.get(1)?, category: r.get(2)?, description: r.get(3)?, has_thumbnail: r.get(4)?, builtin: r.get(5)? })
        })?;
        rows.collect()
    }

    /// An item's name and what it's made of (Studio pastes it).
    pub fn toolbox_item(&self, id: i64) -> rusqlite::Result<Option<(String, String)>> {
        self.0
            .lock()
            .unwrap()
            .query_row("SELECT name, content FROM toolbox WHERE id = ?1 AND removed = 0", [id], |r| Ok((r.get(0)?, r.get(1)?)))
            .optional()
    }

    pub fn toolbox_thumbnail(&self, id: i64) -> rusqlite::Result<Option<Vec<u8>>> {
        self.0
            .lock()
            .unwrap()
            .query_row("SELECT thumbnail FROM toolbox WHERE id = ?1 AND removed = 0", [id], |r| r.get::<_, Option<Vec<u8>>>(0))
            .optional()
            .map(Option::flatten)
    }

    /// Takes an item out of the toolbox. False if there's no such item.
    pub fn remove_toolbox(&self, id: i64) -> rusqlite::Result<bool> {
        Ok(self.0.lock().unwrap().execute("UPDATE toolbox SET removed = 1 WHERE id = ?1 AND removed = 0", [id])? > 0)
    }

    // --- friends -----------------------------------------------------------------

    /// Notes that someone's on the website now (at most once a minute).
    pub fn seen(&self, user_id: i64) -> rusqlite::Result<()> {
        let t = now();
        self.0.lock().unwrap().execute("UPDATE users SET last_seen = ?1 WHERE id = ?2 AND last_seen < ?1 - 60", params![t, user_id])?;
        Ok(())
    }

    pub fn relation(&self, me: i64, other: i64) -> rusqlite::Result<Relation> {
        let conn = self.0.lock().unwrap();
        let (a, b) = (me.min(other), me.max(other));
        let friends: bool = conn.query_row("SELECT EXISTS(SELECT 1 FROM friends WHERE a = ?1 AND b = ?2)", params![a, b], |r| r.get(0))?;
        if friends {
            return Ok(Relation::Friends);
        }
        let sent: bool = conn.query_row("SELECT EXISTS(SELECT 1 FROM friend_requests WHERE from_id = ?1 AND to_id = ?2)", params![me, other], |r| r.get(0))?;
        if sent {
            return Ok(Relation::Sent);
        }
        let got: bool = conn.query_row("SELECT EXISTS(SELECT 1 FROM friend_requests WHERE from_id = ?1 AND to_id = ?2)", params![other, me], |r| r.get(0))?;
        Ok(if got { Relation::Received } else { Relation::None })
    }

    fn friend_count(conn: &Connection, id: i64) -> rusqlite::Result<i64> {
        conn.query_row("SELECT COUNT(*) FROM friends WHERE a = ?1 OR b = ?1", [id], |r| r.get(0))
    }

    fn befriend(conn: &Connection, x: i64, y: i64) -> rusqlite::Result<()> {
        conn.execute("DELETE FROM friend_requests WHERE (from_id = ?1 AND to_id = ?2) OR (from_id = ?2 AND to_id = ?1)", params![x, y])?;
        conn.execute("INSERT OR IGNORE INTO friends (a, b, since) VALUES (?1, ?2, ?3)", params![x.min(y), x.max(y), now()])?;
        Ok(())
    }

    /// `me` asks `other` to be friends (or accepts, if they'd asked first).
    pub fn ask_friend(&self, me: i64, other: i64) -> rusqlite::Result<Asked> {
        let rel = self.relation(me, other)?;
        let conn = self.0.lock().unwrap();
        Ok(match rel {
            Relation::Friends => Asked::AlreadyFriends,
            Relation::Sent => Asked::AlreadySent,
            _ if Self::friend_count(&conn, me)? >= MAX_FRIENDS || Self::friend_count(&conn, other)? >= MAX_FRIENDS => Asked::Full,
            Relation::Received => {
                Self::befriend(&conn, me, other)?;
                Asked::NowFriends
            }
            Relation::None => {
                conn.execute("INSERT INTO friend_requests (from_id, to_id, created) VALUES (?1, ?2, ?3)", params![me, other, now()])?;
                Asked::Sent
            }
        })
    }

    /// `me` says yes to `from`'s request. False if there wasn't one.
    pub fn accept_friend(&self, me: i64, from: i64) -> rusqlite::Result<bool> {
        if self.relation(me, from)? != Relation::Received {
            return Ok(false);
        }
        let conn = self.0.lock().unwrap();
        if Self::friend_count(&conn, me)? >= MAX_FRIENDS || Self::friend_count(&conn, from)? >= MAX_FRIENDS {
            return Ok(false);
        }
        Self::befriend(&conn, me, from)?;
        Ok(true)
    }

    /// Drops a request either way (declining theirs, or taking back yours).
    pub fn drop_request(&self, me: i64, other: i64) -> rusqlite::Result<bool> {
        Ok(self
            .0
            .lock()
            .unwrap()
            .execute("DELETE FROM friend_requests WHERE (from_id = ?1 AND to_id = ?2) OR (from_id = ?2 AND to_id = ?1)", params![me, other])?
            > 0)
    }

    pub fn unfriend(&self, me: i64, other: i64) -> rusqlite::Result<bool> {
        Ok(self.0.lock().unwrap().execute("DELETE FROM friends WHERE a = ?1 AND b = ?2", params![me.min(other), me.max(other)])? > 0)
    }

    fn people(&self, sql: &str, id: i64) -> rusqlite::Result<Vec<FriendRow>> {
        let conn = self.0.lock().unwrap();
        let mut stmt = conn.prepare(sql)?;
        let rows = stmt.query_map([id], |r| {
            let avatar: String = r.get(1)?;
            Ok(FriendRow { username: r.get(0)?, avatar: serde_json::from_str(&avatar).unwrap_or_default(), last_seen: r.get(2)? })
        })?;
        rows.collect()
    }

    /// Someone's friends (not banned ones), by name.
    pub fn friends(&self, id: i64) -> rusqlite::Result<Vec<FriendRow>> {
        self.people(
            "SELECT u.username, u.avatar, u.last_seen FROM friends f JOIN users u ON u.id = (CASE WHEN f.a = ?1 THEN f.b ELSE f.a END)
             WHERE (f.a = ?1 OR f.b = ?1) AND u.banned = 0 ORDER BY u.username COLLATE NOCASE",
            id,
        )
    }

    /// Who's asked to be friends with `id`, newest first.
    pub fn friend_requests_to(&self, id: i64) -> rusqlite::Result<Vec<FriendRow>> {
        self.people(
            "SELECT u.username, u.avatar, u.last_seen FROM friend_requests q JOIN users u ON u.id = q.from_id
             WHERE q.to_id = ?1 AND u.banned = 0 ORDER BY q.created DESC",
            id,
        )
    }

    /// Who `id` has asked, newest first.
    pub fn friend_requests_from(&self, id: i64) -> rusqlite::Result<Vec<FriendRow>> {
        self.people(
            "SELECT u.username, u.avatar, u.last_seen FROM friend_requests q JOIN users u ON u.id = q.to_id
             WHERE q.from_id = ?1 AND u.banned = 0 ORDER BY q.created DESC",
            id,
        )
    }

    // --- Brix and the Catalog ------------------------------------------------

    /// A one-off upgrade: true the first time it's asked about (and never again).
    pub fn first_time(&self, key: &str) -> rusqlite::Result<bool> {
        Ok(self.0.lock().unwrap().execute("INSERT OR IGNORE INTO meta (key, value) VALUES (?1, '1')", [key])? == 1)
    }

    pub fn user_ids(&self) -> rusqlite::Result<Vec<i64>> {
        let conn = self.0.lock().unwrap();
        let mut q = conn.prepare("SELECT id FROM users")?;
        let rows = q.query_map([], |r| r.get(0))?;
        rows.collect()
    }

    fn give(conn: &Connection, user_id: i64, amount: i64, why: &str) -> rusqlite::Result<()> {
        conn.execute("UPDATE users SET brix = brix + ?1 WHERE id = ?2", params![amount, user_id])?;
        conn.execute("INSERT INTO brix_log (user_id, amount, why, at) VALUES (?1, ?2, ?3, ?4)", params![user_id, amount, why, now()])?;
        Ok(())
    }

    /// Gives (or, negative, takes) Brix, noting why. Never below zero.
    pub fn add_brix(&self, user_id: i64, amount: i64, why: &str) -> rusqlite::Result<i64> {
        let conn = self.0.lock().unwrap();
        let have: i64 = conn.query_row("SELECT brix FROM users WHERE id = ?1", [user_id], |r| r.get(0))?;
        Self::give(&conn, user_id, amount.max(-have), why)?;
        conn.query_row("SELECT brix FROM users WHERE id = ?1", [user_id], |r| r.get(0))
    }

    /// The day's visit bonus, the first time someone's seen each day (UTC).
    /// Some(Brix given) when it was given just now.
    pub fn daily_bonus(&self, user_id: i64) -> rusqlite::Result<Option<i64>> {
        let today = now() / 86400;
        let conn = self.0.lock().unwrap();
        let n = conn.execute("UPDATE users SET bonus_day = ?1 WHERE id = ?2 AND bonus_day < ?1", params![today, user_id])?;
        if n == 0 {
            return Ok(None);
        }
        Self::give(&conn, user_id, crate::shop::DAILY_BONUS, "daily visit")?;
        Ok(Some(crate::shop::DAILY_BONUS))
    }

    /// The Catalog items someone owns (free ones aren't listed).
    pub fn owned(&self, user_id: i64) -> rusqlite::Result<Vec<String>> {
        let conn = self.0.lock().unwrap();
        let mut q = conn.prepare("SELECT item FROM owned WHERE user_id = ?1 ORDER BY at")?;
        let rows = q.query_map([user_id], |r| r.get(0))?;
        rows.collect()
    }

    pub fn grant(&self, user_id: i64, item: &str) -> rusqlite::Result<()> {
        self.0.lock().unwrap().execute("INSERT OR IGNORE INTO owned (user_id, item, at) VALUES (?1, ?2, ?3)", params![user_id, item, now()])?;
        Ok(())
    }

    /// Buys something for `price` Brix: the new balance, or why not.
    pub fn buy(&self, user_id: i64, item: &str, price: i64) -> rusqlite::Result<Result<i64, Bought>> {
        let mut conn = self.0.lock().unwrap();
        let tx = conn.transaction()?;
        let owned: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM owned WHERE user_id = ?1 AND item = ?2)", params![user_id, item], |r| r.get(0))?;
        if owned {
            return Ok(Err(Bought::AlreadyOwned));
        }
        let have: i64 = tx.query_row("SELECT brix FROM users WHERE id = ?1", [user_id], |r| r.get(0))?;
        if have < price {
            return Ok(Err(Bought::TooFewBrix { have }));
        }
        Self::give(&tx, user_id, -price, &format!("bought {item}"))?;
        tx.execute("INSERT INTO owned (user_id, item, at) VALUES (?1, ?2, ?3)", params![user_id, item, now()])?;
        tx.commit()?;
        Ok(Ok(have - price))
    }

    /// Admins' price changes: item -> Brix.
    pub fn prices(&self) -> rusqlite::Result<std::collections::HashMap<String, i64>> {
        let conn = self.0.lock().unwrap();
        let mut q = conn.prepare("SELECT item, price FROM prices")?;
        let rows = q.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
        rows.collect()
    }

    /// Changes a price (None: back to the usual one).
    pub fn set_price(&self, item: &str, price: Option<i64>) -> rusqlite::Result<()> {
        let conn = self.0.lock().unwrap();
        match price {
            Some(p) => conn.execute("INSERT INTO prices (item, price) VALUES (?1, ?2) ON CONFLICT(item) DO UPDATE SET price = excluded.price", params![item, p])?,
            None => conn.execute("DELETE FROM prices WHERE item = ?1", [item])?,
        };
        Ok(())
    }

    /// Someone's saved outfits: (slot, name, look).
    pub fn outfits(&self, user_id: i64) -> rusqlite::Result<Vec<Outfit>> {
        let conn = self.0.lock().unwrap();
        let mut q = conn.prepare("SELECT slot, name, avatar FROM outfits WHERE user_id = ?1 ORDER BY slot")?;
        let rows = q.query_map([user_id], |r| {
            let a: String = r.get(2)?;
            Ok(Outfit { slot: r.get(0)?, name: r.get(1)?, avatar: serde_json::from_str(&a).unwrap_or_default() })
        })?;
        rows.collect()
    }

    pub fn set_outfit(&self, user_id: i64, slot: i64, name: &str, avatar: &Avatar) -> rusqlite::Result<()> {
        self.0.lock().unwrap().execute(
            "INSERT INTO outfits (user_id, slot, name, avatar) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(user_id, slot) DO UPDATE SET name = excluded.name, avatar = excluded.avatar",
            params![user_id, slot, name, serde_json::to_string(avatar).unwrap()],
        )?;
        Ok(())
    }

    pub fn delete_outfit(&self, user_id: i64, slot: i64) -> rusqlite::Result<()> {
        self.0.lock().unwrap().execute("DELETE FROM outfits WHERE user_id = ?1 AND slot = ?2", params![user_id, slot])?;
        Ok(())
    }

    /// A player completed a game's challenge (`name`, from its script).
    /// A challenge a game hasn't used before is added, waiting for an admin
    /// to approve it and set its Brix; until then it doesn't count. Each
    /// counts once per player (or once a day, for daily ones), and pays its
    /// Brix up to the day's limit. None when it doesn't count.
    pub fn complete_challenge(&self, game_id: i64, user_id: i64, name: &str) -> rusqlite::Result<Option<(String, i64)>> {
        let mut conn = self.0.lock().unwrap();
        let tx = conn.transaction()?;
        let found = tx
            .query_row("SELECT id, title, reward, daily, approved FROM challenges WHERE game_id = ?1 AND name = ?2", params![game_id, name], |r| {
                Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?, r.get::<_, i64>(3)? != 0, r.get::<_, i64>(4)? != 0))
            })
            .optional()?;
        let Some((id, title, reward, daily, approved)) = found else {
            let count: i64 = tx.query_row("SELECT COUNT(*) FROM challenges WHERE game_id = ?1", [game_id], |r| r.get(0))?;
            if count < crate::shop::MAX_CHALLENGES {
                tx.execute(
                    "INSERT INTO challenges (game_id, name, title, created) VALUES (?1, ?2, ?3, ?4)",
                    params![game_id, name, brixo_runtime::challenge_title(name), now()],
                )?;
                tx.commit()?;
            }
            return Ok(None);
        };
        if !approved {
            return Ok(None);
        }
        let today = now() / 86400;
        let day = if daily { today } else { 0 };
        let fresh = tx.execute(
            "INSERT OR IGNORE INTO challenge_done (user_id, challenge_id, day, at) VALUES (?1, ?2, ?3, ?4)",
            params![user_id, id, day, now()],
        )? == 1;
        if !fresh {
            return Ok(None);
        }
        let earned: i64 = tx.query_row(
            "SELECT COALESCE(SUM(amount), 0) FROM brix_log WHERE user_id = ?1 AND at >= ?2 AND why LIKE 'challenge%'",
            params![user_id, today * 86400],
            |r| r.get(0),
        )?;
        let pay = reward.min(crate::shop::DAILY_CHALLENGE_LIMIT - earned).max(0);
        if pay > 0 {
            Self::give(&tx, user_id, pay, &format!("challenge {id}"))?;
        }
        tx.commit()?;
        Ok(Some((title, pay)))
    }

    /// A game's approved challenges, and whether `user_id` has done each
    /// (today, for daily ones).
    pub fn game_challenges(&self, game_id: i64, user_id: Option<i64>) -> rusqlite::Result<Vec<ChallengeRow>> {
        let conn = self.0.lock().unwrap();
        let today = now() / 86400;
        let mut q = conn.prepare(
            "SELECT c.title, c.reward, c.daily,
                    EXISTS(SELECT 1 FROM challenge_done d WHERE d.challenge_id = c.id AND d.user_id = ?2 AND d.day = (CASE WHEN c.daily = 1 THEN ?3 ELSE 0 END))
             FROM challenges c WHERE c.game_id = ?1 AND c.approved = 1 ORDER BY c.reward, c.id",
        )?;
        let rows = q.query_map(params![game_id, user_id.unwrap_or(-1), today], |r| {
            Ok(ChallengeRow { title: r.get(0)?, reward: r.get(1)?, daily: r.get::<_, i64>(2)? != 0, done: r.get::<_, i64>(3)? != 0 })
        })?;
        rows.collect()
    }

    /// Every game's challenges, waiting ones first, for the admin page.
    pub fn admin_challenges(&self) -> rusqlite::Result<Vec<AdminChallenge>> {
        let conn = self.0.lock().unwrap();
        let mut q = conn.prepare(
            "SELECT c.id, c.game_id, g.name, c.name, c.title, c.reward, c.daily, c.approved,
                    (SELECT COUNT(*) FROM challenge_done d WHERE d.challenge_id = c.id)
             FROM challenges c JOIN games g ON g.id = c.game_id ORDER BY c.approved, g.name, c.id",
        )?;
        let rows = q.query_map([], |r| {
            Ok(AdminChallenge {
                id: r.get(0)?,
                game_id: r.get(1)?,
                game: r.get(2)?,
                name: r.get(3)?,
                title: r.get(4)?,
                reward: r.get(5)?,
                daily: r.get::<_, i64>(6)? != 0,
                approved: r.get::<_, i64>(7)? != 0,
                completed: r.get(8)?,
            })
        })?;
        rows.collect()
    }

    /// An admin sets a challenge up. False if there's no such challenge.
    pub fn set_challenge(&self, id: i64, title: &str, reward: i64, daily: bool, approved: bool) -> rusqlite::Result<bool> {
        Ok(self.0.lock().unwrap().execute(
            "UPDATE challenges SET title = ?1, reward = ?2, daily = ?3, approved = ?4 WHERE id = ?5",
            params![title, reward, daily as i64, approved as i64, id],
        )? > 0)
    }

    /// Adds (or updates) a challenge for a game: the sample games' own.
    pub fn seed_challenge(&self, game_id: i64, name: &str, title: &str, reward: i64, daily: bool) -> rusqlite::Result<()> {
        self.0.lock().unwrap().execute(
            "INSERT INTO challenges (game_id, name, title, reward, daily, approved, created) VALUES (?1, ?2, ?3, ?4, ?5, 1, ?6)
             ON CONFLICT(game_id, name) DO NOTHING",
            params![game_id, name, title, reward, daily as i64, now()],
        )?;
        Ok(())
    }
}

/// Why buying something didn't work.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bought {
    AlreadyOwned,
    TooFewBrix { have: i64 },
}

/// A saved outfit.
#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct Outfit {
    pub slot: i64,
    pub name: String,
    pub avatar: Avatar,
}

/// A game's challenge, as its page shows it.
#[derive(Debug, Clone, Serialize)]
pub struct ChallengeRow {
    pub title: String,
    pub reward: i64,
    pub daily: bool,
    /// Done by the one looking (today, for daily ones).
    pub done: bool,
}

/// A challenge, as the admin page shows it.
#[derive(Debug, Clone, Serialize)]
pub struct AdminChallenge {
    pub id: i64,
    pub game_id: i64,
    pub game: String,
    pub name: String,
    pub title: String,
    pub reward: i64,
    pub daily: bool,
    pub approved: bool,
    /// How many times players have completed it.
    pub completed: i64,
}
