//! Updating on launch, so nobody ever plays an old Brixo.
//!
//! When the downloaded Player or Studio starts, it asks the website which
//! version is current (`/api/version`). If it's behind, it downloads the new
//! one, checks it (size and SHA-256 from versions.json), swaps it in, and
//! starts it again with the same arguments, so Play on the website still
//! lands you in the game you clicked. The app shows what's happening on its
//! own screen meanwhile (see `Updater::state`).
//!
//! If anything goes wrong (offline, the website's down, a file can't be
//! replaced), it carries on with the version it is: an update that fails
//! must never stop anyone playing. Your own cargo builds ("dev") never
//! update, and a just-updated copy (`--just-updated`) doesn't check again,
//! so a mismatch on the website can't make it loop.

use crate::install::{self, App, VERSION};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Added to the arguments of the relaunched copy.
pub const JUST_UPDATED: &str = "--just-updated";

/// How long the check may take before we stop waiting and play anyway.
const CHECK_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Clone, Debug, PartialEq)]
pub enum State {
    Checking,
    /// Bytes so far, of how many.
    Downloading { done: u64, total: u64 },
    Installing,
    /// The new version is in place: relaunch it (see `relaunch`).
    Ready { version: String },
    /// Nothing to do: carry on.
    UpToDate,
    /// Couldn't update (why): carry on with this version.
    Failed(String),
}

impl State {
    /// Still working: keep showing the update screen.
    pub fn busy(&self) -> bool {
        matches!(self, State::Checking | State::Downloading { .. } | State::Installing)
    }

    /// What to show on the update screen.
    pub fn label(&self) -> String {
        match self {
            State::Checking => "Checking for updates...".into(),
            State::Downloading { done, total } if *total > 0 => format!("Updating... {}%", done * 100 / total),
            State::Downloading { .. } => "Updating...".into(),
            State::Installing => "Installing the update...".into(),
            State::Ready { .. } => "Starting...".into(),
            State::UpToDate => String::new(),
            State::Failed(_) => String::new(),
        }
    }

    /// 0 to 1, for a progress bar.
    pub fn fraction(&self) -> f32 {
        match self {
            State::Checking => 0.0,
            State::Downloading { done, total } if *total > 0 => (*done as f32 / *total as f32) * 0.9,
            State::Downloading { .. } => 0.1,
            State::Installing | State::Ready { .. } => 1.0,
            _ => 0.0,
        }
    }
}

pub struct Updater {
    app: App,
    state: Arc<Mutex<State>>,
}

impl Updater {
    /// Starts checking in the background, or None when this copy doesn't
    /// update itself (a cargo build, or just updated).
    pub fn start(app: App) -> Option<Updater> {
        let args: Vec<String> = std::env::args().skip(1).collect();
        if VERSION == "dev" || args.iter().any(|a| a == JUST_UPDATED) || std::env::var_os("BRIXO_NO_UPDATE").is_some() {
            return None;
        }
        let target = self_target(app)?;
        let state = Arc::new(Mutex::new(State::Checking));
        let out = state.clone();
        std::thread::Builder::new()
            .name("updater".into())
            .spawn(move || {
                let result = run(app, &target, &out);
                let mut s = out.lock().unwrap();
                *s = match result {
                    Ok(Some(version)) => State::Ready { version },
                    Ok(None) => State::UpToDate,
                    Err(e) => {
                        eprintln!("update: {e}");
                        State::Failed(e)
                    }
                };
            })
            .ok()?;
        Some(Updater { app, state })
    }

    pub fn state(&self) -> State {
        self.state.lock().unwrap().clone()
    }

    /// Starts the new version with this one's arguments (plus any link that
    /// arrived since, e.g. macOS's Play link), then the caller should exit.
    pub fn relaunch(&self, link: Option<&str>) -> Result<(), String> {
        let mut args: Vec<String> = std::env::args().skip(1).filter(|a| a != JUST_UPDATED).collect();
        if let Some(link) = link {
            if !args.iter().any(|a| a == link) {
                args.insert(0, link.to_string());
            }
        }
        args.push(JUST_UPDATED.to_string());
        let target = self_target(self.app).ok_or("no install to start")?;
        launch(&target, &args)
    }
}

/// For apps without their own update screen (Studio): checks before the
/// app's window opens, and if there's an update, shows the installing
/// window while it downloads, then starts the new version. True when this
/// process should exit (the new version, or a fresh copy, is starting).
pub fn update_before_start(app: App) -> bool {
    let Some(updater) = Updater::start(app) else { return false };
    let waited = std::time::Instant::now();
    while updater.state() == State::Checking && waited.elapsed() < Duration::from_secs(8) {
        std::thread::sleep(Duration::from_millis(30));
    }
    if !updater.state().busy() && !matches!(updater.state(), State::Ready { .. }) {
        return false; // up to date, or couldn't check: carry on
    }
    if updater.state() == State::Checking {
        return false; // the check hung: carry on (it may still finish for next time)
    }
    // An update is coming: show it (this uses up the process's one window).
    let state = updater.state.clone();
    let step = crate::installer::Step::new(format!("Updating {}...", app.title()), move || loop {
        let s = state.lock().unwrap().clone();
        match s {
            State::Ready { .. } => return Ok(()),
            State::Failed(e) => return Err(e),
            State::UpToDate => return Ok(()),
            _ => std::thread::sleep(Duration::from_millis(50)),
        }
    });
    let _ = crate::installer::run(app.title(), vec![step], "Ready!", false);
    let relaunched = matches!(updater.state(), State::Ready { .. }) && updater.relaunch(None).is_ok();
    if !relaunched {
        // Carry on with this version, in a fresh process (this one's window
        // loop is spent).
        if let Ok(me) = std::env::current_exe() {
            let args: Vec<String> = std::env::args().skip(1).collect();
            let _ = std::process::Command::new(me).args(args).env("BRIXO_NO_UPDATE", "1").spawn();
        }
    }
    true
}

/// Where this copy lives, if it's one that updates itself: the installed
/// Windows exe, or the Mac app bundle it's running from.
fn self_target(app: App) -> Option<PathBuf> {
    if cfg!(target_os = "macos") {
        let exe = std::env::current_exe().ok()?;
        return exe.ancestors().find(|p| p.extension().is_some_and(|e| e == "app")).map(Path::to_path_buf);
    }
    if install::is_installed_copy(app) {
        return install::installed_exe(app);
    }
    None
}

/// Checks, and if there's a newer version, downloads and installs it.
/// Some(version) once it's in place.
fn run(app: App, target: &Path, state: &Arc<Mutex<State>>) -> Result<Option<String>, String> {
    let agent = ureq::AgentBuilder::new().timeout_connect(CHECK_TIMEOUT).timeout_read(Duration::from_secs(30)).build();
    let site = install::site();
    let versions: serde_json::Value = agent
        .get(&format!("{site}/api/version"))
        .timeout(CHECK_TIMEOUT)
        .call()
        .map_err(|e| format!("couldn't check for updates: {e}"))?
        .into_json()
        .map_err(|e| format!("couldn't read the version: {e}"))?;
    let Some(latest) = install::newer_version(&versions, app) else { return Ok(None) };
    let entry = &versions[app.key()];
    let file = entry["file"].as_str().ok_or("versions.json has no file")?;
    let bytes = entry["bytes"].as_u64().unwrap_or(0);
    let sha = entry["sha256"].as_str().map(str::to_ascii_lowercase);

    *state.lock().unwrap() = State::Downloading { done: 0, total: bytes };
    let res = agent.get(&format!("{site}/files/{file}")).call().map_err(|e| format!("couldn't download the update: {e}"))?;
    let download = download_path(app, target);
    let _ = std::fs::remove_file(&download);
    {
        use sha2::Digest;
        use std::io::{Read, Write};
        let mut reader = res.into_reader();
        let mut out = std::fs::File::create(&download).map_err(|e| format!("couldn't save the update: {e}"))?;
        let mut hasher = sha2::Sha256::new();
        let mut buf = vec![0u8; 64 * 1024];
        let mut done = 0u64;
        loop {
            let n = reader.read(&mut buf).map_err(|e| format!("the download broke off: {e}"))?;
            if n == 0 {
                break;
            }
            out.write_all(&buf[..n]).map_err(|e| format!("couldn't save the update: {e}"))?;
            hasher.update(&buf[..n]);
            done += n as u64;
            *state.lock().unwrap() = State::Downloading { done, total: bytes };
        }
        out.flush().map_err(|e| e.to_string())?;
        let got = format!("{:x}", hasher.finalize());
        if bytes > 0 && done != bytes {
            let _ = std::fs::remove_file(&download);
            return Err(format!("the download was {done} bytes, not {bytes}"));
        }
        if let Some(want) = sha {
            if got != want {
                let _ = std::fs::remove_file(&download);
                return Err("the download didn't match its checksum".into());
            }
        }
    }
    *state.lock().unwrap() = State::Installing;
    install_update(app, &download, target)?;
    Ok(Some(latest))
}

/// Where the download goes while it's checked: next to the install (same
/// disk, so the swap is a rename), or the temp folder on a Mac.
fn download_path(app: App, target: &Path) -> PathBuf {
    if cfg!(target_os = "macos") {
        return std::env::temp_dir().join(format!("{}-update.dmg", app.exe_name().trim_end_matches(".exe")));
    }
    target.with_extension("update.exe")
}

/// Windows: the running program can't be overwritten but can be renamed,
/// so it steps aside (`.old.exe`, deleted next time) and the new one takes
/// its name.
#[cfg(not(target_os = "macos"))]
fn install_update(_app: App, download: &Path, target: &Path) -> Result<(), String> {
    let old = target.with_extension("old.exe");
    let _ = std::fs::remove_file(&old);
    std::fs::rename(target, &old).map_err(|e| format!("couldn't move the old version aside: {e}"))?;
    if let Err(e) = std::fs::rename(download, target) {
        // Put the old one back, so there's still something to start.
        let _ = std::fs::rename(&old, target);
        return Err(format!("couldn't put the update in place: {e}"));
    }
    Ok(())
}

/// Mac: open the .dmg, copy the app out beside the old one, then swap the
/// two (a running app's bundle can be renamed), and tidy up.
#[cfg(target_os = "macos")]
fn install_update(app: App, download: &Path, target: &Path) -> Result<(), String> {
    use std::process::Command;
    let mount = std::env::temp_dir().join(format!("brixo-update-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&mount);
    let status = Command::new("hdiutil")
        .args(["attach", "-nobrowse", "-readonly", "-noautoopen", "-quiet", "-mountpoint"])
        .arg(&mount)
        .arg(download)
        .status()
        .map_err(|e| format!("couldn't open the update: {e}"))?;
    if !status.success() {
        return Err("couldn't open the update's disk image".into());
    }
    let detach = || {
        let _ = Command::new("hdiutil").args(["detach", "-quiet", "-force"]).arg(&mount).status();
        let _ = std::fs::remove_dir(&mount);
        let _ = std::fs::remove_file(download);
    };
    let source = mount.join(app.mac_bundle());
    let fresh = target.with_extension("app-new");
    let old = target.with_extension("app-old");
    let _ = std::fs::remove_dir_all(&fresh);
    let _ = std::fs::remove_dir_all(&old);
    let copied = Command::new("ditto").arg(&source).arg(&fresh).status();
    detach();
    if !copied.is_ok_and(|s| s.success()) {
        let _ = std::fs::remove_dir_all(&fresh);
        return Err(format!("couldn't copy {} out of the update", app.mac_bundle()));
    }
    std::fs::rename(target, &old).map_err(|e| {
        let _ = std::fs::remove_dir_all(&fresh);
        format!("couldn't move the old version aside: {e}")
    })?;
    if let Err(e) = std::fs::rename(&fresh, target) {
        let _ = std::fs::rename(&old, target);
        return Err(format!("couldn't put the update in place: {e}"));
    }
    let _ = std::fs::remove_dir_all(&old);
    Ok(())
}

#[cfg(target_os = "macos")]
fn launch(bundle: &Path, args: &[String]) -> Result<(), String> {
    std::process::Command::new("open")
        .arg("-n")
        .arg(bundle)
        .arg("--args")
        .args(args)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("couldn't start the new version: {e}"))
}

#[cfg(not(target_os = "macos"))]
fn launch(exe: &Path, args: &[String]) -> Result<(), String> {
    std::process::Command::new(exe).args(args).spawn().map(|_| ()).map_err(|e| format!("couldn't start the new version: {e}"))
}

/// Deletes what an earlier update left behind (the old Windows exe can only
/// go once it's no longer running). Call on startup.
pub fn tidy(app: App) {
    if let Some(t) = install::installed_exe(app) {
        let _ = std::fs::remove_file(t.with_extension("old.exe"));
        let _ = std::fs::remove_file(t.with_extension("update.exe"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn states_say_what_is_happening() {
        assert!(State::Checking.busy());
        assert!(State::Downloading { done: 5, total: 10 }.busy());
        assert!(!State::UpToDate.busy());
        assert!(!State::Failed("offline".into()).busy());
        assert_eq!(State::Downloading { done: 5, total: 10 }.label(), "Updating... 50%");
        assert!((State::Downloading { done: 5, total: 10 }.fraction() - 0.45).abs() < 1e-6);
    }

    #[test]
    fn cargo_builds_never_update() {
        // Tests are "dev" builds.
        assert_eq!(VERSION, "dev");
        assert!(Updater::start(App::Player).is_none());
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn the_swap_keeps_the_old_one_if_it_fails() {
        let dir = std::env::temp_dir().join(format!("brixo-update-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let target = dir.join("BrixoPlayer.exe");
        std::fs::write(&target, "old").unwrap();
        let download = download_path(App::Player, &target);
        std::fs::write(&download, "new").unwrap();
        install_update(App::Player, &download, &target).unwrap();
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "new");
        assert_eq!(std::fs::read_to_string(target.with_extension("old.exe")).unwrap(), "old");
        // A missing download: the old version stays where it was.
        std::fs::write(&target, "current").unwrap();
        assert!(install_update(App::Player, &dir.join("nothing.exe"), &target).is_err());
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "current");
        let _ = std::fs::remove_dir_all(&dir);
    }
    /// A tiny website: /api/version and /files/BrixoPlayer.exe.
    #[cfg(not(target_os = "macos"))]
    fn serve(versions: String, file: Vec<u8>) -> String {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let mut stream = stream;
                let mut buf = [0u8; 2048];
                let n = stream.read(&mut buf).unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]).to_string();
                let (kind, body) = if req.starts_with("GET /api/version") { ("application/json", versions.clone().into_bytes()) } else { ("application/octet-stream", file.clone()) };
                let head = format!("HTTP/1.1 200 OK\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
                let _ = stream.write_all(head.as_bytes());
                let _ = stream.write_all(&body);
            }
        });
        format!("http://{addr}")
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn downloads_checks_and_swaps_in_a_newer_version() {
        use sha2::Digest;
        let dir = std::env::temp_dir().join(format!("brixo-update-run-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let target = dir.join("BrixoPlayer.exe");
        let new = b"the new player".to_vec();
        let sha = format!("{:x}", sha2::Sha256::digest(&new));

        // A bad checksum: refused, the old one untouched.
        std::fs::write(&target, "old").unwrap();
        let bad = serde_json::json!({"player": {"version": "2099.01.01.0000", "file": "BrixoPlayer.exe", "bytes": new.len(), "sha256": "00"}});
        unsafe { std::env::set_var("BRIXO_SITE", serve(bad.to_string(), new.clone())) };
        let state = Arc::new(Mutex::new(State::Checking));
        assert!(run(App::Player, &target, &state).unwrap_err().contains("checksum"));
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "old");

        // A good one: swapped in.
        let good = serde_json::json!({"player": {"version": "2099.01.01.0000", "file": "BrixoPlayer.exe", "bytes": new.len(), "sha256": sha}});
        unsafe { std::env::set_var("BRIXO_SITE", serve(good.to_string(), new.clone())) };
        assert_eq!(run(App::Player, &target, &state).unwrap().as_deref(), Some("2099.01.01.0000"));
        assert_eq!(std::fs::read(&target).unwrap(), new);

        // Same version as us: nothing to do.
        let same = serde_json::json!({"player": {"version": VERSION, "file": "BrixoPlayer.exe"}});
        unsafe { std::env::set_var("BRIXO_SITE", serve(same.to_string(), Vec::new())) };
        assert_eq!(run(App::Player, &target, &state).unwrap(), None);
        unsafe { std::env::remove_var("BRIXO_SITE") };
        let _ = std::fs::remove_dir_all(&dir);
    }
}

