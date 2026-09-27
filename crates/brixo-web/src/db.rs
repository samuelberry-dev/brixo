//! What the website remembers: accounts, avatars, sessions and games.
//! One SQLite file behind a mutex: simple, and plenty for now.

use std::sync::Mutex;

use rand::RngCore;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

pub struct Db(Mutex<Connection>);

pub type Rgb = (u8, u8, u8);

#[derive(Debug, Clone, PartialEq, Serialize, serde::Deserialize)]
pub struct Avatar {
    pub skin: Rgb,
    pub shirt: Rgb,
    pub pants: Rgb,
    /// Still saved (games' scripts can read shoes_color), but not drawn.
    pub shoes: Rgb,
    pub face: String,
    /// The hats they wear, by name (up to brixo_core::MAX_HATS).
    #[serde(default)]
    pub hats: Vec<String>,
}

impl Default for Avatar {
    fn default() -> Self {
        Avatar { skin: (227, 185, 138), shirt: (13, 105, 172), pants: (27, 42, 53), shoes: (27, 27, 27), face: "smile".into(), hats: Vec::new() }
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
}

pub fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

/// Games anyone can see and play: not taken down, and not by a banned account.
const LISTED: &str = "g.hidden = 0 AND u.banned = 0";

const GAME_COLUMNS: &str = "g.id, g.name, u.username, g.description, g.visits, g.created, g.thumbnail IS NOT NULL";

fn game_from_row(r: &rusqlite::Row) -> rusqlite::Result<GameRow> {
    Ok(GameRow {
        id: r.get(0)?,
        name: r.get(1)?,
        owner: r.get(2)?,
        description: r.get(3)?,
        visits: r.get(4)?,
        created: r.get(5)?,
        has_thumbnail: r.get(6)?,
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

const USER_COLUMNS: &str = "id, username, avatar, blurb, created, admin";

fn user_from_row(r: &rusqlite::Row) -> rusqlite::Result<User> {
    let avatar: String = r.get(2)?;
    Ok(User {
        id: r.get(0)?,
        username: r.get(1)?,
        avatar: serde_json::from_str(&avatar).unwrap_or_default(),
        blurb: r.get(3)?,
        created: r.get(4)?,
        admin: r.get::<_, i64>(5)? != 0,
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
        // Password reset links an admin makes (one per account, one use).
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS resets (
                 token TEXT PRIMARY KEY,
                 user_id INTEGER NOT NULL UNIQUE REFERENCES users(id),
                 created INTEGER NOT NULL
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
            "INSERT INTO users (username, password_hash, avatar, created) VALUES (?1, ?2, ?3, ?4)",
            params![username, password_hash, serde_json::to_string(&avatar).unwrap(), created],
        )?;
        Ok(User { id: conn.last_insert_rowid(), username: username.to_string(), avatar, blurb: String::new(), created, admin: false })
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
            "INSERT INTO users (username, password_hash, avatar, created) VALUES (?1, ?2, ?3, ?4)",
            params![username, password_hash, serde_json::to_string(&avatar).unwrap(), created],
        )?;
        let id = tx.last_insert_rowid();
        tx.execute("UPDATE invites SET used_by = ?1, used_at = ?2 WHERE code = ?3", params![id, created, code])?;
        tx.commit()?;
        Ok(Some(User { id, username: username.to_string(), avatar, blurb: String::new(), created, admin: false }))
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
            "SELECT g.id, g.name, u.username, g.visits, g.created, g.hidden, u.banned FROM games g JOIN users u ON u.id = g.owner_id
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
            "SELECT u.id, u.username, u.avatar, u.blurb, u.created, u.admin FROM sessions s JOIN users u ON u.id = s.user_id
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
}
