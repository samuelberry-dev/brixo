//! The game servers Play starts: one per game, shared by everyone playing
//! it, shut down a while after the last player leaves.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use brixo_server::ServerHandle;

/// How long an empty server waits for players before shutting down.
pub const IDLE_SHUTDOWN: Duration = Duration::from_secs(60);
/// How long a Play ticket stays good.
pub const TICKET_LIFETIME: Duration = Duration::from_secs(60);

/// Where players reach the game servers this website starts.
#[derive(Clone, Debug)]
pub struct Network {
    /// The address handed to Brixo Player: a domain name or IP that
    /// players can reach (BRIXO_PUBLIC_HOST). 127.0.0.1 on your own PC.
    pub public_host: String,
    /// The ports game servers may use, first to last (BRIXO_GAME_PORTS,
    /// like "7500-7599"). None means any free port: fine on your own PC,
    /// but a real server's firewall needs to know which ports to open.
    pub ports: Option<(u16, u16)>,
}

impl Default for Network {
    fn default() -> Network {
        Network { public_host: "127.0.0.1".into(), ports: None }
    }
}

impl Network {
    /// Reads BRIXO_PUBLIC_HOST and BRIXO_GAME_PORTS.
    pub fn from_env() -> Result<Network, String> {
        let mut n = Network::default();
        if let Ok(host) = std::env::var("BRIXO_PUBLIC_HOST") {
            let host = host.trim();
            if host.is_empty() || host.contains(char::is_whitespace) || host.contains('/') {
                return Err(format!("BRIXO_PUBLIC_HOST should be a domain or IP, like play.example.com (got {host:?})"));
            }
            n.public_host = host.to_string();
        }
        if let Ok(range) = std::env::var("BRIXO_GAME_PORTS") {
            n.ports = Some(parse_ports(&range).ok_or_else(|| format!("BRIXO_GAME_PORTS should look like 7500-7599 (got {range:?})"))?);
        }
        Ok(n)
    }

    /// "host:port" for Brixo Player (IPv6 addresses go in brackets).
    pub fn address(&self, port: u16) -> String {
        if self.public_host.contains(':') && !self.public_host.starts_with('[') {
            format!("[{}]:{port}", self.public_host)
        } else {
            format!("{}:{port}", self.public_host)
        }
    }
}

/// "7500-7599" (or a single "7500") into (first, last).
pub fn parse_ports(s: &str) -> Option<(u16, u16)> {
    let s = s.trim();
    let (a, b) = s.split_once('-').unwrap_or((s, s));
    let (a, b): (u16, u16) = (a.trim().parse().ok()?, b.trim().parse().ok()?);
    (a > 0 && a <= b).then_some((a, b))
}

/// Every game server port is taken (the website turns this into "busy").
pub fn all_busy(e: &std::io::Error) -> bool {
    e.kind() == std::io::ErrorKind::WouldBlock
}

struct Running {
    handle: ServerHandle,
    /// When the server last had nobody in it (None while players are on).
    empty_since: Option<Instant>,
}

#[derive(Default)]
pub struct Servers {
    running: Mutex<HashMap<i64, Running>>,
}

/// A one-time pass into one game's server, made when someone presses Play.
pub struct Ticket {
    pub user_id: i64,
    pub game_id: i64,
    pub made: Instant,
}

#[derive(Default)]
pub struct Tickets(Mutex<HashMap<String, Ticket>>);

impl Tickets {
    pub fn issue(&self, user_id: i64, game_id: i64) -> String {
        let token = crate::db::random_token();
        let mut t = self.0.lock().unwrap();
        t.retain(|_, v| v.made.elapsed() < TICKET_LIFETIME); // tidy up old ones
        t.insert(token.clone(), Ticket { user_id, game_id, made: Instant::now() });
        token
    }

    /// Uses up a ticket if it's fresh and for this game; returns its user.
    pub fn redeem(&self, token: &str, game_id: i64) -> Option<i64> {
        let mut t = self.0.lock().unwrap();
        let ok = t.get(token).is_some_and(|v| v.game_id == game_id && v.made.elapsed() < TICKET_LIFETIME);
        if ok { t.remove(token).map(|v| v.user_id) } else { None }
    }
}

impl Servers {
    /// The port of this game's server, starting one if needed. `start` is
    /// given the port to use (0 = any). With a port range, it tries each
    /// free port in turn, skipping ones something else is using; when all
    /// are taken the error is `all_busy`.
    pub fn port_for(
        &self,
        game_id: i64,
        ports: Option<(u16, u16)>,
        start: impl Fn(u16) -> std::io::Result<ServerHandle>,
    ) -> std::io::Result<u16> {
        let mut running = self.running.lock().unwrap();
        if let Some(r) = running.get(&game_id) {
            return Ok(r.handle.port());
        }
        let handle = match ports {
            None => start(0)?,
            Some((first, last)) => {
                let taken: std::collections::HashSet<u16> = running.values().map(|r| r.handle.port()).collect();
                let mut found = None;
                for port in (first..=last).filter(|p| !taken.contains(p)) {
                    match start(port) {
                        Ok(h) => {
                            found = Some(h);
                            break;
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => continue,
                        Err(e) => return Err(e),
                    }
                }
                found.ok_or_else(|| std::io::Error::new(std::io::ErrorKind::WouldBlock, "every game server is busy"))?
            }
        };
        let port = handle.port();
        // Counts as empty from the start: if nobody shows up, it goes away.
        running.insert(game_id, Running { handle, empty_since: Some(Instant::now()) });
        Ok(port)
    }

    /// How many are playing each game right now.
    pub fn player_counts(&self) -> HashMap<i64, usize> {
        self.running.lock().unwrap().iter().map(|(id, r)| (*id, r.handle.player_count())).collect()
    }

    pub fn is_running(&self, game_id: i64) -> bool {
        self.running.lock().unwrap().contains_key(&game_id)
    }

    /// Shuts down servers that have been empty for `idle`.
    pub fn reap(&self, idle: Duration) {
        let mut running = self.running.lock().unwrap();
        for r in running.values_mut() {
            if r.handle.player_count() > 0 {
                r.empty_since = None;
            } else if r.empty_since.is_none() {
                r.empty_since = Some(Instant::now());
            }
        }
        // Dropping a handle stops that server.
        running.retain(|_, r| r.empty_since.is_none_or(|t| t.elapsed() < idle));
    }
}

pub async fn reap_forever(app: Arc<crate::api::App>) {
    loop {
        tokio::time::sleep(Duration::from_secs(5)).await;
        app.servers.reap(IDLE_SHUTDOWN);
    }
}
