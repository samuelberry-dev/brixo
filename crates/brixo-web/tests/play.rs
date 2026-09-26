//! The whole Roblox-style flow, over real HTTP and real sockets: an
//! account, an avatar, Play starting a game server, and joining it with
//! the ticket exactly the way Brixo Player does.

use std::sync::Arc;
use std::time::{Duration, Instant};

use brixo_server::NetClient;

fn start_site() -> (String, Arc<brixo_web::api::App>) {
    start_site_with(brixo_web::servers::Network::default())
}

fn start_site_with(network: brixo_web::servers::Network) -> (String, Arc<brixo_web::api::App>) {
    let app = Arc::new(brixo_web::api::App::open_with(":memory:", network).unwrap());
    brixo_web::api::seed_samples(&app);
    let rt = tokio::runtime::Runtime::new().unwrap();
    let listener = rt.block_on(tokio::net::TcpListener::bind("127.0.0.1:0")).unwrap();
    let addr = listener.local_addr().unwrap();
    let router = brixo_web::api::router(app.clone());
    std::thread::spawn(move || rt.block_on(async { axum::serve(listener, router).await.unwrap() }));
    (format!("http://{addr}"), app)
}

fn browser() -> ureq::Agent {
    ureq::AgentBuilder::new().build() // keeps cookies, like a browser
}

fn status(r: Result<ureq::Response, ureq::Error>) -> u16 {
    match r {
        Ok(r) => r.status(),
        Err(ureq::Error::Status(code, _)) => code,
        Err(e) => panic!("{e}"),
    }
}

fn wait_until(c: &mut NetClient, what: &str, done: impl Fn(&NetClient) -> bool) {
    let start = Instant::now();
    while !done(c) {
        c.poll();
        assert!(start.elapsed() < Duration::from_secs(5), "timed out: {what}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn sign_up_customize_press_play_and_join_as_yourself() {
    let (site, app) = start_site();
    let ann = browser();

    // The catalog is public, and Coin Tycoon is there from the start.
    let games: serde_json::Value = ann.get(&format!("{site}/api/games")).call().unwrap().into_json().unwrap();
    let tycoon = games.as_array().unwrap().iter().find(|g| g["name"] == "Coin Tycoon").expect("seeded");
    assert_eq!(tycoon["owner"], "Brixo");
    let game_id = tycoon["id"].as_i64().unwrap();

    // Playing needs an account.
    assert_eq!(status(ann.post(&format!("{site}/api/games/{game_id}/play")).call()), 401);
    let body = serde_json::json!({"username": "Ann", "password": "correct horse"});
    ann.post(&format!("{site}/api/signup")).send_json(body).unwrap();
    assert_eq!(status(browser().post(&format!("{site}/api/signup")).send_json(serde_json::json!({"username": "ann", "password": "whatever"}))), 400, "names are unique, whatever the case");
    assert_eq!(status(browser().post(&format!("{site}/api/signup")).send_json(serde_json::json!({"username": "noob", "password": "whatever"}))), 400, "no rude names");

    // Customize the avatar.
    let look = serde_json::json!({"skin": [204,142,105], "shirt": [196,40,28], "pants": [27,42,53], "shoes": [27,27,27], "face": "determined"});
    ann.put(&format!("{site}/api/avatar")).send_json(look).unwrap();
    let bad_face = serde_json::json!({"skin": [1,1,1], "shirt": [1,1,1], "pants": [1,1,1], "shoes": [1,1,1], "face": "evil"});
    assert_eq!(status(ann.put(&format!("{site}/api/avatar")).send_json(bad_face)), 400);

    // Press Play: a server starts, and we get a ticket into it.
    let pass: serde_json::Value = ann.post(&format!("{site}/api/games/{game_id}/play")).call().unwrap().into_json().unwrap();
    let (server, ticket) = (pass["server"].as_str().unwrap().to_string(), pass["ticket"].as_str().unwrap().to_string());
    assert!(app.servers.is_running(game_id));

    // Brixo Player joins with the ticket: we're Ann, wearing our avatar,
    // in Coin Tycoon (and the game's code never reached us).
    let mut player = NetClient::connect_with_ticket(&server, &ticket).unwrap();
    wait_until(&mut player, "joined", |c| c.me.is_some() && c.world.find_first("PlotA").is_some());
    let me = player.me.unwrap();
    assert_eq!(player.world.get(me).unwrap().name, "Ann");
    let p = player.world.player(me).unwrap();
    assert_eq!(p.face, brixo_core::Face::Determined);
    assert_eq!((p.shirt_color.r, p.shirt_color.g, p.shirt_color.b), (196, 40, 28));
    let game_script = player.world.find_first("Game").unwrap();
    assert_eq!(player.world.script(game_script).unwrap().source, "", "scripts stay on the server");

    // The catalog shows someone playing.
    std::thread::sleep(Duration::from_millis(200));
    let games: serde_json::Value = browser().get(&format!("{site}/api/games")).call().unwrap().into_json().unwrap();
    let t = games.as_array().unwrap().iter().find(|g| g["id"] == game_id).unwrap();
    assert_eq!(t["playing"], 1);

    // The ticket was used up: nobody else can ride in on it.
    let mut thief = NetClient::connect_with_ticket(&server, &ticket).unwrap();
    wait_until(&mut thief, "turned away", |c| !c.connected);
    assert!(thief.me.is_none());

    // A friend presses Play too: same server, and they see Ann there.
    let bob = browser();
    bob.post(&format!("{site}/api/signup")).send_json(serde_json::json!({"username": "Bob", "password": "hunter22"})).unwrap();
    let pass2: serde_json::Value = bob.post(&format!("{site}/api/games/{game_id}/play")).call().unwrap().into_json().unwrap();
    assert_eq!(pass2["server"].as_str().unwrap(), server, "one server per game, shared");
    let mut bobs = NetClient::connect_with_ticket(&server, pass2["ticket"].as_str().unwrap()).unwrap();
    wait_until(&mut bobs, "Bob sees Ann", |c| c.world.find_first("Ann").is_some() && c.me.is_some());
    assert_eq!(bobs.world.get(bobs.me.unwrap()).unwrap().name, "Bob");

    // Everyone leaves: the server shuts down once it's been idle a while.
    drop(player);
    drop(bobs);
    std::thread::sleep(Duration::from_millis(300));
    app.servers.reap(Duration::from_secs(60));
    assert!(app.servers.is_running(game_id), "not idle long enough yet");
    app.servers.reap(Duration::ZERO);
    assert!(!app.servers.is_running(game_id), "idle servers are shut down");
}

#[test]
fn studio_publishes_games_into_the_catalog() {
    let (site, _app) = start_site();
    let me = browser();
    me.post(&format!("{site}/api/signup")).send_json(serde_json::json!({"username": "Maker", "password": "abcdef"})).unwrap();
    let game = brixo_core::DataModel::new().to_json().unwrap();
    let r: serde_json::Value = me.post(&format!("{site}/api/games")).send_json(serde_json::json!({"name": "My Obby", "data": game})).unwrap().into_json().unwrap();
    assert!(r["id"].as_i64().is_some());
    assert_eq!(status(me.post(&format!("{site}/api/games")).send_json(serde_json::json!({"name": "Junk", "data": "not a game"}))), 400);
    assert_eq!(status(browser().post(&format!("{site}/api/games")).send_json(serde_json::json!({"name": "X", "data": game}))), 401);
    let games: serde_json::Value = browser().get(&format!("{site}/api/games")).call().unwrap().into_json().unwrap();
    assert!(games.as_array().unwrap().iter().any(|g| g["name"] == "My Obby" && g["owner"] == "Maker"));
}

#[test]
fn game_pages_profiles_thumbnails_and_stats() {
    let (site, app) = start_site();
    let get = |a: &ureq::Agent, path: &str| -> serde_json::Value { a.get(&format!("{site}{path}")).call().unwrap().into_json().unwrap() };
    let anon = browser();

    // Every page is served.
    for page in ["/", "/games", "/games/1", "/users/Brixo", "/download", "/login", "/signup", "/avatar", "/favicon.svg", "/app.css", "/app.js"] {
        assert_eq!(status(anon.get(&format!("{site}{page}")).call()), 200, "{page}");
    }

    // The samples come with pictures and descriptions.
    let games = get(&anon, "/api/games");
    let flagfall = games.as_array().unwrap().iter().find(|g| g["name"] == "Flagfall").expect("seeded");
    let id = flagfall["id"].as_i64().unwrap();
    assert_eq!(flagfall["has_thumbnail"], true);
    assert!(flagfall["description"].as_str().unwrap().len() > 20);
    let png = anon.get(&format!("{site}/api/games/{id}/thumbnail")).call().unwrap();
    assert_eq!(png.content_type(), "image/png");
    assert_eq!(get(&anon, &format!("/api/games/{id}"))["name"], "Flagfall");
    assert_eq!(status(anon.get(&format!("{site}/api/games/99999")).call()), 404);

    // Brixo's profile lists its games.
    let brixo = get(&anon, "/api/users/brixo");
    assert_eq!(brixo["username"], "Brixo");
    assert_eq!(brixo["games"].as_array().unwrap().len(), 3);
    assert_eq!(status(anon.get(&format!("{site}/api/users/nobody_here")).call()), 404);

    // A new player: blurb (filtered), and can't edit someone else's game.
    let bo = browser();
    bo.post(&format!("{site}/api/signup")).send_json(serde_json::json!({"username": "Bobby", "password": "pass words"})).unwrap();
    bo.put(&format!("{site}/api/me/blurb")).send_json(serde_json::json!({"blurb": "  I build castles.  "})).unwrap();
    assert_eq!(get(&anon, "/api/users/Bobby")["blurb"], "I build castles.");
    assert_eq!(status(bo.put(&format!("{site}/api/me/blurb")).send_json(serde_json::json!({"blurb": "x".repeat(201)}))), 400);
    assert_eq!(status(bo.put(&format!("{site}/api/games/{id}")).send_json(serde_json::json!({"description": "mine now"}))), 403);
    assert_eq!(status(anon.put(&format!("{site}/api/me/blurb")).send_json(serde_json::json!({"blurb": "hi"}))), 401);

    // Stats count real players, not Brixo itself.
    let stats = get(&anon, "/api/stats");
    assert_eq!(stats["users"], 1);
    assert_eq!(stats["games"], 3);

    // Pressing Play counts a visit.
    let before = flagfall["visits"].as_i64().unwrap();
    bo.post(&format!("{site}/api/games/{id}/play")).call().unwrap();
    assert_eq!(get(&anon, &format!("/api/games/{id}"))["visits"].as_i64().unwrap(), before + 1);
    drop(app);
}

#[test]
fn game_servers_use_the_public_address_and_port_range() {
    use brixo_web::servers::{parse_ports, Network};
    assert_eq!(parse_ports("7500-7599"), Some((7500, 7599)));
    assert_eq!(parse_ports(" 7500 "), Some((7500, 7500)));
    assert_eq!(parse_ports("7599-7500"), None);
    assert_eq!(parse_ports("lots"), None);
    let v6 = Network { public_host: "::1".into(), ports: None };
    assert_eq!(v6.address(7500), "[::1]:7500");

    // Something else already has the first port of the range: it's skipped.
    let squatter = std::net::TcpListener::bind("0.0.0.0:0").unwrap();
    let first = squatter.local_addr().unwrap().port();
    let (site, app) = start_site_with(Network { public_host: "localhost".into(), ports: Some((first, first + 2)) });
    let ann = browser();
    ann.post(&format!("{site}/api/signup")).send_json(serde_json::json!({"username": "Annie", "password": "pass words"})).unwrap();
    let games: serde_json::Value = ann.get(&format!("{site}/api/games")).call().unwrap().into_json().unwrap();
    let ids: Vec<i64> = games.as_array().unwrap().iter().map(|g| g["id"].as_i64().unwrap()).collect();
    assert_eq!(ids.len(), 3);

    let play = |id: i64| ann.post(&format!("{site}/api/games/{id}/play")).call();
    let pass: serde_json::Value = play(ids[0]).unwrap().into_json().unwrap();
    assert_eq!(pass["server"], format!("localhost:{}", first + 1));
    let again: serde_json::Value = play(ids[0]).unwrap().into_json().unwrap();
    assert_eq!(again["server"], pass["server"], "one server per game");
    let pass2: serde_json::Value = play(ids[1]).unwrap().into_json().unwrap();
    assert_eq!(pass2["server"], format!("localhost:{}", first + 2));

    // The range is full: the third game says busy instead of breaking.
    assert_eq!(status(play(ids[2])), 503);

    // The player can reach it by that address.
    let fresh: serde_json::Value = play(ids[0]).unwrap().into_json().unwrap();
    let mut c = NetClient::connect_with_ticket(fresh["server"].as_str().unwrap(), fresh["ticket"].as_str().unwrap()).unwrap();
    wait_until(&mut c, "joined through the public address", |c| c.me.is_some());

    // When a game's server shuts down, its port is free for the next game.
    drop(c);
    std::thread::sleep(Duration::from_millis(300));
    app.servers.reap(Duration::ZERO);
    let pass3: serde_json::Value = play(ids[2]).unwrap().into_json().unwrap();
    assert!(pass3["server"] == format!("localhost:{}", first + 1) || pass3["server"] == format!("localhost:{}", first + 2));
    drop(squatter);
}
