//! `brixo-web`: the Brixo website, at http://127.0.0.1:7420 by default.
//!
//! Commands (they use the same database as the website, even while it runs):
//!   brixo-web                       run the website
//!   brixo-web set-password NAME     set someone's password (e.g. Brixo's)
//!   brixo-web invite [COUNT]        make invite codes (1 unless you say)
//!   brixo-web invites               list invite codes and who used them
//!   brixo-web admin NAME            make someone an admin (the /admin page)
//!   brixo-web unadmin NAME          ...or not
//!   brixo-web ban NAME / unban NAME ban an account, or lift the ban
//!   brixo-web hide GAME_ID          take a game down (show GAME_ID: put it back)
//!
//! Settings (all optional; the defaults suit your own PC):
//! - PORT: the website's port (7420).
//! - BRIXO_WEB_DB: the database file (brixo-web.sqlite).
//! - BRIXO_WEB_BIND: the address the website listens on (127.0.0.1). On a
//!   real server it stays 127.0.0.1 behind Caddy, which adds HTTPS.
//! - BRIXO_PUBLIC_HOST: the address players' Brixo Player connects to for
//!   games (127.0.0.1), like playbrixo.com.
//! - BRIXO_GAME_PORTS: the ports game servers use, like 7500-7519 (any).
//!   Its size is also the most games that can run at once.
//! - BRIXO_SECURE_COOKIES=1: login cookies only over HTTPS.
//! - BRIXO_TRUST_PROXY=1: behind Caddy; take visitors' addresses from it.
//! - BRIXO_INVITE_ONLY=1: signing up needs an invite code.
//! - BRIXO_DOWNLOADS: the folder with the Player/Studio downloads and
//!   versions.json (./downloads). tools/release.ps1 fills it.
use std::sync::Arc;

use brixo_web::api::{self, App, Settings};

fn flag(name: &str) -> bool {
    std::env::var(name).is_ok_and(|v| matches!(v.trim(), "1" | "true" | "yes" | "on"))
}

fn fail(msg: impl std::fmt::Display) -> ! {
    eprintln!("{msg}");
    std::process::exit(2);
}

fn main() {
    let db_path = std::env::var("BRIXO_WEB_DB").unwrap_or_else(|_| "brixo-web.sqlite".to_string());
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        None => serve(&db_path),
        Some("set-password") => set_password(&db_path, args.get(1).unwrap_or_else(|| fail("usage: brixo-web set-password NAME"))),
        Some("invite") => {
            let count = args.get(1).map(|n| n.parse::<usize>().unwrap_or_else(|_| fail("usage: brixo-web invite [COUNT]"))).unwrap_or(1);
            let db = brixo_web::db::Db::open(&db_path).unwrap_or_else(|e| fail(e));
            for code in db.create_invites(count.min(1000)).unwrap_or_else(|e| fail(e)) {
                println!("{code}");
            }
        }
        Some("invites") => {
            let db = brixo_web::db::Db::open(&db_path).unwrap_or_else(|e| fail(e));
            for (code, used_by) in db.invites().unwrap_or_else(|e| fail(e)) {
                match used_by {
                    Some(name) => println!("{code}  used by {name}"),
                    None => println!("{code}  unused"),
                }
            }
        }
        Some(cmd @ ("admin" | "unadmin" | "ban" | "unban")) => {
            let name = args.get(1).unwrap_or_else(|| fail(format!("usage: brixo-web {cmd} NAME")));
            let db = brixo_web::db::Db::open(&db_path).unwrap_or_else(|e| fail(e));
            let found = match cmd {
                "admin" | "unadmin" => db.set_admin(name, cmd == "admin").unwrap_or_else(|e| fail(e)),
                _ => db.set_banned(name, cmd == "ban").unwrap_or_else(|e| fail(e)).is_some(),
            };
            if !found {
                fail(format!("there's no one called {name}"));
            }
            println!(
                "{}",
                match cmd {
                    "admin" => format!("{name} is an admin: they'll see an Admin tab on the site."),
                    "unadmin" => format!("{name} isn't an admin any more."),
                    // A running website also sends them out of games (the
                    // Admin page does that); from here they're kept out of
                    // new ones and logged out.
                    "ban" => format!("{name} is banned: logged out, and can't log in or play."),
                    _ => format!("{name} isn't banned any more."),
                }
            );
        }
        Some(cmd @ ("hide" | "show")) => {
            let id: i64 = args.get(1).and_then(|n| n.parse().ok()).unwrap_or_else(|| fail(format!("usage: brixo-web {cmd} GAME_ID (the number in the game's address)")));
            let db = brixo_web::db::Db::open(&db_path).unwrap_or_else(|e| fail(e));
            if !db.set_hidden(id, cmd == "hide").unwrap_or_else(|e| fail(e)) {
                fail(format!("there's no game {id}"));
            }
            println!("{}", if cmd == "hide" { format!("Game {id} is taken down.") } else { format!("Game {id} is back.") });
        }
        Some(other) => fail(format!("unknown command {other:?}: try set-password, invite, invites, admin, ban, unban, hide or show")),
    }
}

/// Asks for the new password twice (not shown as you type).
fn set_password(db_path: &str, name: &str) {
    let app = App::open(db_path).unwrap_or_else(|e| fail(e));
    api::seed_samples(&app); // makes sure the Brixo account exists
    let first = rpassword::prompt_password(format!("New password for {name}: ")).unwrap_or_else(|e| fail(e));
    api::check_password(&first).unwrap_or_else(|e| fail(e));
    let again = rpassword::prompt_password("Type it again: ").unwrap_or_else(|e| fail(e));
    if first != again {
        fail("those didn't match; nothing changed");
    }
    match app.db.set_password(name, &api::hash(&first)) {
        Ok(true) => println!("Password set for {name}. Any old logins for {name} were signed out."),
        Ok(false) => fail(format!("there's no one called {name}")),
        Err(e) => fail(e),
    }
}

#[tokio::main]
async fn serve(db_path: &str) {
    let port: u16 = std::env::var("PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(7420);
    let bind = std::env::var("BRIXO_WEB_BIND").unwrap_or_else(|_| "127.0.0.1".to_string());
    let settings = Settings {
        network: brixo_web::servers::Network::from_env().unwrap_or_else(|e| fail(e)),
        secure_cookies: flag("BRIXO_SECURE_COOKIES"),
        trust_proxy: flag("BRIXO_TRUST_PROXY"),
        invite_only: flag("BRIXO_INVITE_ONLY"),
        downloads: std::env::var_os("BRIXO_DOWNLOADS").map(Into::into).unwrap_or_else(|| "downloads".into()),
    };
    let app = Arc::new(App::open_with(db_path, settings.clone()).expect("couldn't open the database"));
    api::seed_samples(&app);
    tokio::spawn(brixo_web::servers::reap_forever(app.clone()));
    let listener = tokio::net::TcpListener::bind((bind.as_str(), port)).await.expect("couldn't use that address and port");
    println!("Brixo website: http://{bind}:{port}");
    let n = &settings.network;
    match n.ports {
        Some((a, b)) => println!("Game servers: {} on ports {a}-{b}", n.public_host),
        None => println!("Game servers: {} on any free port", n.public_host),
    }
    println!(
        "Secure cookies: {}  Behind proxy: {}  Invite-only: {}",
        settings.secure_cookies, settings.trust_proxy, settings.invite_only
    );
    let service = api::router(app).into_make_service_with_connect_info::<std::net::SocketAddr>();
    axum::serve(listener, service).await.unwrap();
}
