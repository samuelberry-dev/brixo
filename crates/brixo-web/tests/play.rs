//! The whole Roblox-style flow, over real HTTP and real sockets: an
//! account, an avatar, Play starting a game server, and joining it with
//! the ticket exactly the way Brixo Player does.

use std::sync::Arc;
use std::time::{Duration, Instant};

use brixo_server::NetClient;

fn start_site() -> (String, Arc<brixo_web::api::App>) {
    start_site_with(brixo_web::api::Settings::default())
}

fn with_network(network: brixo_web::servers::Network) -> brixo_web::api::Settings {
    brixo_web::api::Settings { network, ..Default::default() }
}

fn start_site_with(settings: brixo_web::api::Settings) -> (String, Arc<brixo_web::api::App>) {
    let app = Arc::new(brixo_web::api::App::open_with(":memory:", settings).unwrap());
    brixo_web::api::seed_samples(&app);
    let rt = tokio::runtime::Runtime::new().unwrap();
    let listener = rt.block_on(tokio::net::TcpListener::bind("127.0.0.1:0")).unwrap();
    let addr = listener.local_addr().unwrap();
    let router = brixo_web::api::router(app.clone());
    let service = router.into_make_service_with_connect_info::<std::net::SocketAddr>();
    std::thread::spawn(move || rt.block_on(async { axum::serve(listener, service).await.unwrap() }));
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
    assert_eq!(status(browser().post(&format!("{site}/api/signup")).send_json(serde_json::json!({"username": "shithead", "password": "whatever"}))), 400, "no rude names");

    // Customize the avatar.
    let look = serde_json::json!({"skin": [204,142,105], "shirt": [196,40,28], "pants": [27,42,53], "shoes": [27,27,27], "face": "determined", "hats": ["top_hat", "headphones"]});
    ann.put(&format!("{site}/api/avatar")).send_json(look).unwrap();
    let bad_face = serde_json::json!({"skin": [1,1,1], "shirt": [1,1,1], "pants": [1,1,1], "shoes": [1,1,1], "face": "evil"});
    assert_eq!(status(ann.put(&format!("{site}/api/avatar")).send_json(bad_face)), 400);
    let hat = |hats: serde_json::Value| serde_json::json!({"skin": [1,1,1], "shirt": [1,1,1], "pants": [1,1,1], "shoes": [1,1,1], "face": "smile", "hats": hats});
    assert_eq!(status(ann.put(&format!("{site}/api/avatar")).send_json(hat(serde_json::json!(["sombrero"])))), 400, "unknown hat");
    assert_eq!(status(ann.put(&format!("{site}/api/avatar")).send_json(hat(serde_json::json!(["cap", "cap"])))), 400, "same hat twice");
    assert_eq!(status(ann.put(&format!("{site}/api/avatar")).send_json(hat(serde_json::json!(["cap", "halo", "crown", "beanie"])))), 400, "too many hats");

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
    assert_eq!(p.hats, [Some(brixo_core::Hat::TopHat), Some(brixo_core::Hat::Headphones), None], "wearing our hats");
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
    me.post(&format!("{site}/api/signup")).send_json(serde_json::json!({"username": "Maker", "password": "abcdefgh"})).unwrap();
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
    // The range sits below the ports the OS hands out for outgoing
    // connections (Windows gives those in order, and a client's own end can
    // land on a game port and connect to itself), like 7500-7519 on the server.
    let (squatter, first) = (20000..30000u16)
        .step_by(7)
        .find_map(|p| {
            let free = |q: u16| std::net::TcpListener::bind(("0.0.0.0", q)).is_ok();
            (free(p + 1) && free(p + 2)).then(|| std::net::TcpListener::bind(("0.0.0.0", p)).ok().map(|l| (l, p))).flatten()
        })
        .expect("three free ports between 20000 and 30000");
    let (site, app) = start_site_with(with_network(Network { public_host: "localhost".into(), ports: Some((first, first + 2)) }));
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

fn signup_as(site: &str, name: &str, password: &str, invite: &str) -> (ureq::Agent, u16) {
    let a = browser();
    let code = status(a.post(&format!("{site}/api/signup")).send_json(serde_json::json!({"username": name, "password": password, "invite": invite})));
    (a, code)
}

#[test]
fn invite_only_signups() {
    let (site, app) = start_site_with(brixo_web::api::Settings { invite_only: true, ..Default::default() });
    let stats: serde_json::Value = browser().get(&format!("{site}/api/stats")).call().unwrap().into_json().unwrap();
    assert_eq!(stats["invite_only"], true, "the pages know to show the invite box");

    assert_eq!(signup_as(&site, "Cara", "long enough", "").1, 400, "no code, no account");
    assert_eq!(signup_as(&site, "Cara", "long enough", "AAAA-BBBB-CCCC").1, 400, "made-up codes don't work");

    let codes = app.db.create_invites(2).unwrap();
    assert_eq!(codes.len(), 2);
    assert!(codes[0].len() == 14 && codes[0].chars().nth(4) == Some('-'));
    // Friends type codes however they like.
    let sloppy = codes[0].to_lowercase().replace('-', " ");
    let (cara, code) = signup_as(&site, "Cara", "long enough", &sloppy);
    assert_eq!(code, 200);
    assert_eq!(status(cara.get(&format!("{site}/api/me")).call()), 200, "signed in right away");
    assert_eq!(signup_as(&site, "Dev", "long enough", &codes[0]).1, 400, "each code works once");
    assert_eq!(signup_as(&site, "Cara", "long enough", &codes[1]).1, 400, "name taken");
    assert_eq!(signup_as(&site, "Dev", "long enough", &codes[1]).1, 200, "a failed signup doesn't use up the code");

    let list = app.db.invites().unwrap();
    assert!(list.iter().any(|(c, by)| c == &codes[0] && by.as_deref() == Some("Cara")));
    assert!(list.iter().any(|(c, by)| c == &codes[1] && by.as_deref() == Some("Dev")));
}

#[test]
fn passwords_names_and_rate_limits() {
    let (site, app) = start_site_with(brixo_web::api::Settings { secure_cookies: true, ..Default::default() });
    // Rules for new accounts.
    assert_eq!(signup_as(&site, "Evan", "short", "").1, 400, "8+ characters");
    assert_eq!(signup_as(&site, "Evan", &"x".repeat(129), "").1, 400, "not giant");
    for name in ["Brixo", "brixo_", "BrixoAdmin", "Official_Mod", "TheStaff"] {
        assert_eq!(signup_as(&site, name, "long enough", "").1, 400, "{name} looks like staff");
    }
    assert_eq!(signup_as(&site, "BrixoFan", "long enough", "").1, 200, "fans are fine");

    // Secure cookies when asked for, lasting 30 days.
    let res = browser().post(&format!("{site}/api/login")).send_json(serde_json::json!({"username": "BrixoFan", "password": "long enough"})).unwrap();
    let cookie = res.header("set-cookie").unwrap().to_string();
    assert!(cookie.contains("; Secure") && cookie.contains("HttpOnly") && cookie.contains("Max-Age=2592000"), "{cookie}");
    assert_eq!(res.header("x-frame-options"), Some("DENY"));

    // Ten wrong guesses, then even the right password has to wait.
    let guesser = browser();
    let guess = |pw: &str| status(guesser.post(&format!("{site}/api/login")).send_json(serde_json::json!({"username": "brixofan", "password": pw})));
    for _ in 0..10 {
        assert_eq!(guess("wrong guess"), 400);
    }
    assert_eq!(guess("wrong guess"), 429);
    assert_eq!(guess("long enough"), 429, "locked for a while, even with the right one");

    // Only 5 new accounts an hour from one address.
    let (fan, _) = signup_as(&site, "Fan0", "long enough", "");
    for i in 1..4 {
        assert_eq!(signup_as(&site, &format!("Fan{i}"), "long enough", "").1, 200);
    }
    assert_eq!(signup_as(&site, "Fan9", "long enough", "").1, 429);

    // The command-line password change works and signs old logins out.
    assert_eq!(status(fan.get(&format!("{site}/api/me")).call()), 200);
    assert!(app.db.set_password("Fan0", &brixo_web::api::hash("a new password")).unwrap());
    assert_eq!(status(fan.get(&format!("{site}/api/me")).call()), 401, "signed out");
    assert!(!app.db.set_password("Nobody_Here", "x").unwrap());
}

#[test]
fn studio_can_publish_big_games() {
    let (site, _app) = start_site();
    let (maker, _) = signup_as(&site, "Maker", "long enough", "");
    // Flagfall is about 4.5 MB; the old 2 MB limit refused it.
    let data = brixo_samples::flagfall().to_json().unwrap();
    assert!(data.len() > 2 * 1024 * 1024, "the test needs a big game ({} bytes)", data.len());
    let code = status(maker.post(&format!("{site}/api/games")).send_json(serde_json::json!({"name": "My Flagfall", "data": data})));
    assert_eq!(code, 200);
}


#[test]
fn downloads_and_versions() {
    let dir = std::env::temp_dir().join(format!("brixo-downloads-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let (site, _app) = start_site_with(brixo_web::api::Settings { downloads: dir.clone(), ..Default::default() });
    let get = |path: &str| browser().get(&format!("{site}{path}")).call();

    // Before the first release: no versions, no files.
    let v: serde_json::Value = get("/api/version").unwrap().into_json().unwrap();
    assert_eq!(v, serde_json::json!({}));
    assert_eq!(status(get("/files/BrixoPlayer.exe")), 404);

    // After tools/release.ps1 has uploaded.
    std::fs::write(dir.join("BrixoPlayer.exe"), b"MZ pretend program").unwrap();
    std::fs::write(dir.join("versions.json"), r#"{"player": {"version": "2026.09.26.0100", "file": "BrixoPlayer.exe", "bytes": 18}}"#).unwrap();
    let v: serde_json::Value = get("/api/version").unwrap().into_json().unwrap();
    assert_eq!(v["player"]["version"], "2026.09.26.0100");
    let file = get("/files/BrixoPlayer.exe").unwrap();
    assert!(file.header("content-disposition").unwrap().contains("attachment"));
    let mut body = Vec::new();
    std::io::Read::read_to_end(&mut file.into_reader(), &mut body).unwrap();
    assert_eq!(body, b"MZ pretend program");

    // Nothing outside the downloads folder.
    for sneaky in ["/files/..%2Fversions.json", "/files/%2E%2E%2F%2E%2E%2Fetc%2Fpasswd", "/files/.hidden", "/files/a%5Cb"] {
        assert_eq!(status(get(sneaky)), 404, "{sneaky}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn saved_data_is_kept_per_game_and_account_between_visits() {
    let (site, app) = start_site();
    let me = browser();
    me.post(&format!("{site}/api/signup")).send_json(serde_json::json!({"username": "Saver", "password": "abcdefgh"})).unwrap();
    // A game that counts your visits and keeps the count.
    let mut dm = brixo_core::DataModel::new();
    let root = dm.root();
    let s = dm.create(brixo_core::Class::Script, "Visits", root).unwrap();
    dm.script_mut(s).unwrap().source = "on player_joined(p)\n    p.visits = (load(p, \"visits\") or 0) + 1\n    save(p, \"visits\", p.visits)\nend\n".into();
    let r: serde_json::Value = me
        .post(&format!("{site}/api/games"))
        .send_json(serde_json::json!({"name": "Visit Counter", "data": dm.to_json().unwrap()}))
        .unwrap()
        .into_json()
        .unwrap();
    let game_id = r["id"].as_i64().unwrap();
    let visits = |c: &NetClient| match c.world.get(c.me?)?.attributes.get("visits") {
        Some(brixo_core::Attribute::Num(n)) => Some(*n),
        _ => None,
    };
    for expected in [1.0, 2.0] {
        let pass: serde_json::Value = me.post(&format!("{site}/api/games/{game_id}/play")).call().unwrap().into_json().unwrap();
        let mut c = NetClient::connect_with_ticket(pass["server"].as_str().unwrap(), pass["ticket"].as_str().unwrap()).unwrap();
        wait_until(&mut c, "the visit count", |c| visits(c).is_some());
        assert_eq!(visits(&c), Some(expected));
        // Leave, and the game server closes (saving on the way out).
        drop(c);
        std::thread::sleep(Duration::from_millis(300));
        app.servers.reap(Duration::ZERO);
        assert!(!app.servers.is_running(game_id));
    }
    // It's in the website's database, under this game and account.
    let user = app.db.user_by_name("Saver").unwrap().unwrap();
    let saved = app.db.player_save(game_id, user.id).unwrap().unwrap();
    assert_eq!(saved, "{\"visits\":2.0}");
}

#[test]
fn admins_ban_accounts_and_take_games_down() {
    let (site, app) = start_site();
    let boss = browser();
    boss.post(&format!("{site}/api/signup")).send_json(serde_json::json!({"username": "Boss", "password": "abcdefgh"})).unwrap();
    let troll = browser();
    troll.post(&format!("{site}/api/signup")).send_json(serde_json::json!({"username": "Troll", "password": "abcdefgh"})).unwrap();
    let game = brixo_core::DataModel::new().to_json().unwrap();
    let r: serde_json::Value = troll.post(&format!("{site}/api/games")).send_json(serde_json::json!({"name": "Bad Game", "data": game})).unwrap().into_json().unwrap();
    let bad_game = r["id"].as_i64().unwrap();

    // Not an admin yet: the admin API says no.
    assert_eq!(status(boss.get(&format!("{site}/api/admin")).call()), 403);
    assert!(app.db.set_admin("Boss", true).unwrap());
    let me: serde_json::Value = boss.get(&format!("{site}/api/me")).call().unwrap().into_json().unwrap();
    assert_eq!(me["admin"], true);
    let overview: serde_json::Value = boss.get(&format!("{site}/api/admin")).call().unwrap().into_json().unwrap();
    assert!(overview["users"].as_array().unwrap().iter().any(|u| u["username"] == "Troll"));
    assert!(overview["games"].as_array().unwrap().iter().any(|g| g["id"] == bad_game));
    // Regular people can't use it.
    assert_eq!(status(troll.post(&format!("{site}/api/admin/ban")).send_json(serde_json::json!({"username": "Boss", "banned": true}))), 403);

    // Take the game down: gone from the catalog and unplayable.
    let listed = |id: i64| {
        let games: serde_json::Value = browser().get(&format!("{site}/api/games")).call().unwrap().into_json().unwrap();
        games.as_array().unwrap().iter().any(|g| g["id"] == id)
    };
    assert!(listed(bad_game));
    boss.post(&format!("{site}/api/admin/hide")).send_json(serde_json::json!({"id": bad_game, "hidden": true})).unwrap();
    assert!(!listed(bad_game));
    assert_eq!(status(boss.post(&format!("{site}/api/games/{bad_game}/play")).call()), 404);
    assert_eq!(status(browser().get(&format!("{site}/api/games/{bad_game}")).call()), 404);
    boss.post(&format!("{site}/api/admin/hide")).send_json(serde_json::json!({"id": bad_game, "hidden": false})).unwrap();
    assert!(listed(bad_game), "put back");

    // Ban the troll while they're in a game: sent out, logged out, can't
    // log back in, and their profile and games are hidden.
    let tycoon = { let g: serde_json::Value = browser().get(&format!("{site}/api/games")).call().unwrap().into_json().unwrap();
        g.as_array().unwrap().iter().find(|g| g["name"] == "Coin Tycoon").unwrap()["id"].as_i64().unwrap() };
    let pass: serde_json::Value = troll.post(&format!("{site}/api/games/{tycoon}/play")).call().unwrap().into_json().unwrap();
    let mut c = NetClient::connect_with_ticket(pass["server"].as_str().unwrap(), pass["ticket"].as_str().unwrap()).unwrap();
    wait_until(&mut c, "joined", |c| c.me.is_some());
    assert_eq!(status(boss.post(&format!("{site}/api/admin/ban")).send_json(serde_json::json!({"username": "Boss", "banned": true}))), 400, "not yourself");
    boss.post(&format!("{site}/api/admin/ban")).send_json(serde_json::json!({"username": "Troll", "banned": true})).unwrap();
    wait_until(&mut c, "sent out of the game", |c| !c.connected);
    assert_eq!(status(troll.get(&format!("{site}/api/me")).call()), 401, "logged out");
    let login = browser().post(&format!("{site}/api/login")).send_json(serde_json::json!({"username": "Troll", "password": "abcdefgh"}));
    assert_eq!(status(login), 403);
    assert_eq!(status(browser().get(&format!("{site}/api/users/Troll")).call()), 404);
    assert!(!listed(bad_game), "a banned account's games are hidden too");
    // Unban: all back.
    boss.post(&format!("{site}/api/admin/ban")).send_json(serde_json::json!({"username": "Troll", "banned": false})).unwrap();
    assert!(listed(bad_game));
    assert_eq!(status(browser().post(&format!("{site}/api/login")).send_json(serde_json::json!({"username": "Troll", "password": "abcdefgh"}))), 200);
}

#[test]
fn admins_make_reset_links_and_people_change_their_own_password() {
    let (site, app) = start_site();
    let boss = browser();
    boss.post(&format!("{site}/api/signup")).send_json(serde_json::json!({"username": "Boss", "password": "abcdefgh"})).unwrap();
    app.db.set_admin("Boss", true).unwrap();
    let kid = browser();
    kid.post(&format!("{site}/api/signup")).send_json(serde_json::json!({"username": "Forgetful", "password": "oldpassword"})).unwrap();
    let login = |pw: &str| status(browser().post(&format!("{site}/api/login")).send_json(serde_json::json!({"username": "Forgetful", "password": pw})));

    // Only admins make links.
    assert_eq!(status(kid.post(&format!("{site}/api/admin/reset")).send_json(serde_json::json!({"username": "Forgetful"}))), 403);
    assert_eq!(status(boss.post(&format!("{site}/api/admin/reset")).send_json(serde_json::json!({"username": "Nobody"}))), 404);
    let link = |who: &str| -> String {
        let r: serde_json::Value = boss.post(&format!("{site}/api/admin/reset")).send_json(serde_json::json!({"username": who})).unwrap().into_json().unwrap();
        r["token"].as_str().unwrap().to_string()
    };
    let first = link("forgetful");
    let token = link("Forgetful");
    // A new link replaces the old one.
    assert_eq!(status(browser().post(&format!("{site}/api/reset/check")).send_json(serde_json::json!({"token": first}))), 400);
    let who: serde_json::Value = browser().post(&format!("{site}/api/reset/check")).send_json(serde_json::json!({"token": token})).unwrap().into_json().unwrap();
    assert_eq!(who["username"], "Forgetful");

    // Using it: too short is refused (and the link still works), then it works once.
    let fresh = browser();
    assert_eq!(status(fresh.post(&format!("{site}/api/reset")).send_json(serde_json::json!({"token": token, "password": "short"}))), 400);
    assert_eq!(status(fresh.post(&format!("{site}/api/reset")).send_json(serde_json::json!({"token": token, "password": "brandnew123"}))), 200);
    let me: serde_json::Value = fresh.get(&format!("{site}/api/me")).call().unwrap().into_json().unwrap();
    assert_eq!(me["username"], "Forgetful", "logged in by the reset");
    assert_eq!(status(kid.get(&format!("{site}/api/me")).call()), 401, "other logins are logged out");
    assert_eq!(status(fresh.post(&format!("{site}/api/reset")).send_json(serde_json::json!({"token": token, "password": "again12345"}))), 400, "used up");
    assert_eq!(login("oldpassword"), 400);
    assert_eq!(login("brandnew123"), 200);

    // Changing your own password needs the current one, and keeps you logged in here only.
    let other = browser();
    other.post(&format!("{site}/api/login")).send_json(serde_json::json!({"username": "Forgetful", "password": "brandnew123"})).unwrap();
    assert_eq!(status(fresh.put(&format!("{site}/api/me/password")).send_json(serde_json::json!({"current": "wrong", "password": "another123"}))), 400);
    assert_eq!(status(fresh.put(&format!("{site}/api/me/password")).send_json(serde_json::json!({"current": "brandnew123", "password": "another123"}))), 204);
    assert_eq!(status(fresh.get(&format!("{site}/api/me")).call()), 200, "still logged in here");
    assert_eq!(status(other.get(&format!("{site}/api/me")).call()), 401, "but not elsewhere");
    assert_eq!(login("another123"), 200);
    assert_eq!(status(browser().put(&format!("{site}/api/me/password")).send_json(serde_json::json!({"current": "x", "password": "another123"}))), 401);
    // The page itself is there.
    assert_eq!(status(browser().get(&format!("{site}/reset")).call()), 200);
}
