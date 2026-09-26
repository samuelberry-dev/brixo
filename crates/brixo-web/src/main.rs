//! `brixo-web`: the Brixo website, at http://127.0.0.1:7420 by default.
//!
//! Settings (all optional):
//! - PORT: the website's port (7420).
//! - BRIXO_WEB_DB: the database file (brixo-web.sqlite).
//! - BRIXO_WEB_BIND: the address the website listens on (127.0.0.1). On a
//!   real server it stays 127.0.0.1 behind Caddy, which adds HTTPS.
//! - BRIXO_PUBLIC_HOST: the address players' Brixo Player connects to for
//!   games (127.0.0.1), like play.example.com.
//! - BRIXO_GAME_PORTS: the ports game servers use, like 7500-7599 (any).
use std::sync::Arc;

#[tokio::main]
async fn main() {
    let port: u16 = std::env::var("PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(7420);
    let db_path = std::env::var("BRIXO_WEB_DB").unwrap_or_else(|_| "brixo-web.sqlite".to_string());
    let bind = std::env::var("BRIXO_WEB_BIND").unwrap_or_else(|_| "127.0.0.1".to_string());
    let network = brixo_web::servers::Network::from_env().unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(2);
    });
    let app = Arc::new(brixo_web::api::App::open_with(&db_path, network.clone()).expect("couldn't open the database"));
    brixo_web::api::seed_samples(&app);
    tokio::spawn(brixo_web::servers::reap_forever(app.clone()));
    let listener = tokio::net::TcpListener::bind((bind.as_str(), port)).await.expect("couldn't use that address and port");
    println!("Brixo website: http://{bind}:{port}");
    match network.ports {
        Some((a, b)) => println!("Game servers: {} on ports {a}-{b}", network.public_host),
        None => println!("Game servers: {} on any free port", network.public_host),
    }
    axum::serve(listener, brixo_web::api::router(app)).await.unwrap();
}
