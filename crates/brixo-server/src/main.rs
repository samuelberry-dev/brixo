//! `brixo-server game.brixo [--port 4570]`: hosts a game for players to join.

use brixo_core::DataModel;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let Some(path) = args.get(1).filter(|a| !a.starts_with("--")) else {
        eprintln!("usage: brixo-server <game.brixo> [--port {}]", brixo_server::DEFAULT_PORT);
        std::process::exit(2);
    };
    let port = args
        .iter()
        .position(|a| a == "--port")
        .and_then(|i| args.get(i + 1))
        .and_then(|p| p.parse().ok())
        .unwrap_or(brixo_server::DEFAULT_PORT);

    let model = match DataModel::load_file(path) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("couldn't open {path}: {e}");
            std::process::exit(1);
        }
    };
    let server = match brixo_server::start(model, port) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("couldn't start the server on port {port}: {e}");
            std::process::exit(1);
        }
    };
    println!("Serving {path} on port {}. Players join at <this computer's address>:{}", server.port(), server.port());
    println!("Press Ctrl+C to stop.");
    loop {
        for line in server.take_log() {
            let tag = if line.is_error { "ERROR " } else { "" };
            println!("[{}] {tag}{}", line.source, line.text);
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}
