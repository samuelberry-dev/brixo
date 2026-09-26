//! Installing, updating and uninstalling Brixo Player and Studio, the way
//! Roblox does it: the file you download installs itself.
//!
//! Running the downloaded `BrixoPlayer.exe` (or `BrixoStudio.exe`) copies it
//! to `%LOCALAPPDATA%\Brixo\Player`, adds Start Menu and desktop shortcuts
//! and an entry in Windows' installed apps, and (Player) makes Play on the
//! website open it. No admin rights needed: it's all per-user. Downloading
//! a newer one and running it updates in place.
//!
//! Your own builds (`brixo-player.exe` from cargo) never install themselves:
//! only a file named exactly like the download does.

use std::path::{Path, PathBuf};

/// The website Player and Studio talk to (BRIXO_SITE overrides it).
pub const SITE: &str = "https://playbrixo.com";

pub fn site() -> String {
    std::env::var("BRIXO_SITE").ok().filter(|s| !s.trim().is_empty()).unwrap_or_else(|| SITE.to_string())
}

/// This build's version: set by tools/release.ps1 (BRIXO_BUILD) for the
/// downloads, "dev" for builds you make yourself.
pub const VERSION: &str = match option_env!("BRIXO_BUILD") {
    Some(v) => v,
    None => "dev",
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum App {
    Player,
    Studio,
}

impl App {
    pub fn title(self) -> &'static str {
        match self {
            App::Player => "Brixo Player",
            App::Studio => "Brixo Studio",
        }
    }

    /// The downloaded file's name.
    pub fn exe_name(self) -> &'static str {
        match self {
            App::Player => "BrixoPlayer.exe",
            App::Studio => "BrixoStudio.exe",
        }
    }

    fn folder(self) -> &'static str {
        match self {
            App::Player => "Player",
            App::Studio => "Studio",
        }
    }

    /// Its name in the website's versions.json.
    pub fn key(self) -> &'static str {
        match self {
            App::Player => "player",
            App::Studio => "studio",
        }
    }
}

/// Where it installs: `%LOCALAPPDATA%\Brixo\Player` (None off Windows).
pub fn install_dir(app: App) -> Option<PathBuf> {
    let local = std::env::var_os("LOCALAPPDATA")?;
    Some(PathBuf::from(local).join("Brixo").join(app.folder()))
}

pub fn installed_exe(app: App) -> Option<PathBuf> {
    install_dir(app).map(|d| d.join(app.exe_name()))
}

/// True for the downloaded file, not a cargo build. Browsers name a second
/// download "BrixoPlayer (1).exe", so that counts too.
pub fn is_download(app: App, exe: &Path) -> bool {
    let Some(name) = exe.file_name().and_then(|n| n.to_str()) else { return false };
    let name = name.to_ascii_lowercase();
    let want = app.exe_name().to_ascii_lowercase();
    let (Some(stem), Some(want_stem)) = (name.strip_suffix(".exe"), want.strip_suffix(".exe")) else { return false };
    let Some(rest) = stem.strip_prefix(want_stem) else { return false };
    let rest = rest.trim();
    rest.is_empty() || (rest.starts_with('(') && rest.ends_with(')') && rest[1..rest.len() - 1].chars().all(|c| c.is_ascii_digit()))
}

/// True when this program is the installed copy.
pub fn is_installed_copy(app: App) -> bool {
    let (Ok(me), Some(target)) = (std::env::current_exe(), installed_exe(app)) else { return false };
    same_path(&me, &target)
}

fn same_path(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => a.to_string_lossy().eq_ignore_ascii_case(&b.to_string_lossy()),
    }
}

/// What `main` should do after `on_startup`.
#[derive(Debug, PartialEq, Eq)]
pub enum Startup {
    /// Carry on and run.
    Run,
    /// We handed over to the installed copy (or uninstalled): exit now.
    Exit,
}

/// Call first thing in `main`. Handles `--uninstall`, and installs the
/// downloaded file then starts the installed copy (with the same
/// arguments, plus `--installed` if there were none). `after_install` does
/// app-specific setup with the installed exe's path (Player: brixo:// links).
pub fn on_startup(app: App, after_install: impl FnOnce(&Path) -> Result<(), String>) -> Startup {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("--uninstall") {
        match uninstall(app) {
            Ok(()) => message(app.title(), &format!("{} was uninstalled. Your games and saves in your Brixo folder were kept.", app.title())),
            Err(e) => message(app.title(), &format!("Couldn't uninstall {}: {e}", app.title())),
        }
        return Startup::Exit;
    }
    let Ok(me) = std::env::current_exe() else { return Startup::Run };
    let Some(target) = installed_exe(app) else { return Startup::Run };
    if !is_download(app, &me) || same_path(&me, &target) {
        return Startup::Run;
    }
    let installed = install(app, &me, &target).and_then(|()| after_install(&target));
    match installed {
        Ok(()) => {
            let args = if args.is_empty() { vec!["--installed".to_string()] } else { args };
            match std::process::Command::new(&target).args(&args).spawn() {
                Ok(_) => Startup::Exit,
                Err(e) => {
                    message(app.title(), &format!("{} installed, but didn't start: {e}", app.title()));
                    Startup::Exit
                }
            }
        }
        Err(e) => {
            message(app.title(), &format!("Couldn't install {}: {e}\n\nIt will run from where it is this time.", app.title()));
            Startup::Run
        }
    }
}

/// Copies `me` over the installed copy. The installed one may be running
/// (a game open): Windows won't overwrite a running program but will
/// rename it, so the old one steps aside first.
fn install(app: App, me: &Path, target: &Path) -> Result<(), String> {
    let dir = target.parent().ok_or("no install folder")?;
    std::fs::create_dir_all(dir).map_err(|e| format!("couldn't make {}: {e}", dir.display()))?;
    let old = target.with_extension("old.exe");
    let _ = std::fs::remove_file(&old);
    if target.exists() {
        let _ = std::fs::rename(target, &old);
    }
    std::fs::copy(me, target).map_err(|e| format!("couldn't copy to {}: {e}", target.display()))?;
    // The download is marked as "from the internet"; the installed copy
    // doesn't need Windows asking about it again every time it starts.
    let _ = std::fs::remove_file(format!("{}:Zone.Identifier", target.display()));
    add_shortcuts(app, target)
}

#[cfg(windows)]
fn add_shortcuts(app: App, target: &Path) -> Result<(), String> {
    // Shortcuts and the installed-apps entry need Windows' shell, which is
    // easiest to reach from PowerShell. Paths go in through environment
    // variables, so spaces and apostrophes in folder names are safe.
    let script = r#"
$ErrorActionPreference = 'Stop'
$shell = New-Object -ComObject WScript.Shell
$places = @([Environment]::GetFolderPath('Programs'), [Environment]::GetFolderPath('Desktop'))
foreach ($dir in $places) {
    if (-not $dir) { continue }
    $link = $shell.CreateShortcut((Join-Path $dir ($env:BRIXO_TITLE + '.lnk')))
    $link.TargetPath = $env:BRIXO_TARGET
    $link.WorkingDirectory = Split-Path $env:BRIXO_TARGET
    $link.IconLocation = $env:BRIXO_TARGET + ',0'
    $link.Save()
}
$key = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\' + $env:BRIXO_KEY
New-Item -Path $key -Force | Out-Null
Set-ItemProperty -Path $key -Name DisplayName -Value $env:BRIXO_TITLE
Set-ItemProperty -Path $key -Name DisplayVersion -Value $env:BRIXO_VERSION
Set-ItemProperty -Path $key -Name Publisher -Value 'Brixo'
Set-ItemProperty -Path $key -Name DisplayIcon -Value $env:BRIXO_TARGET
Set-ItemProperty -Path $key -Name InstallLocation -Value (Split-Path $env:BRIXO_TARGET)
Set-ItemProperty -Path $key -Name UninstallString -Value ('"' + $env:BRIXO_TARGET + '" --uninstall')
Set-ItemProperty -Path $key -Name NoModify -Value 1 -Type DWord
Set-ItemProperty -Path $key -Name NoRepair -Value 1 -Type DWord
"#;
    powershell(app, target, script)
}

#[cfg(windows)]
fn uninstall(app: App) -> Result<(), String> {
    let target = installed_exe(app).ok_or("no install folder")?;
    let script = r#"
$ErrorActionPreference = 'Continue'
foreach ($dir in @([Environment]::GetFolderPath('Programs'), [Environment]::GetFolderPath('Desktop'))) {
    if ($dir) { Remove-Item -LiteralPath (Join-Path $dir ($env:BRIXO_TITLE + '.lnk')) -ErrorAction SilentlyContinue }
}
Remove-Item -LiteralPath ('HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\' + $env:BRIXO_KEY) -Recurse -ErrorAction SilentlyContinue
if ($env:BRIXO_KEY -eq 'BrixoPlayer') {
    Remove-Item -LiteralPath 'HKCU:\Software\Classes\brixo' -Recurse -ErrorAction SilentlyContinue
}
# The program can't delete itself while it runs (its "uninstalled" message
# is still open): a hidden window keeps trying for two minutes, and stops
# as soon as the folder is gone.
$folder = Split-Path $env:BRIXO_TARGET
$retry = 'for /l %i in (1,1,120) do (ping -n 2 127.0.0.1 >nul & rmdir /s /q "' + $folder + '" 2>nul & if not exist "' + $folder + '" exit)'
Start-Process -WindowStyle Hidden -FilePath cmd.exe -ArgumentList ('/c ' + $retry)
"#;
    powershell(app, &target, script)
}

#[cfg(windows)]
fn powershell(app: App, target: &Path, script: &str) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let key = match app {
        App::Player => "BrixoPlayer",
        App::Studio => "BrixoStudio",
    };
    let out = std::process::Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", script])
        .env("BRIXO_TITLE", app.title())
        .env("BRIXO_TARGET", target)
        .env("BRIXO_KEY", key)
        .env("BRIXO_VERSION", VERSION)
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("couldn't run PowerShell: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

#[cfg(not(windows))]
fn add_shortcuts(_app: App, _target: &Path) -> Result<(), String> {
    Ok(())
}

#[cfg(not(windows))]
fn uninstall(_app: App) -> Result<(), String> {
    Err("uninstalling is only for the Windows download".into())
}

/// A message box (Windows), or a line on the console elsewhere.
#[cfg(windows)]
pub fn message(title: &str, text: &str) {
    #[link(name = "user32")]
    unsafe extern "system" {
        fn MessageBoxW(hwnd: *mut core::ffi::c_void, text: *const u16, caption: *const u16, kind: u32) -> i32;
    }
    let wide = |s: &str| s.encode_utf16().chain(std::iter::once(0)).collect::<Vec<u16>>();
    let (text, title) = (wide(text), wide(title));
    // MB_OK | MB_ICONINFORMATION
    unsafe { MessageBoxW(std::ptr::null_mut(), text.as_ptr(), title.as_ptr(), 0x40) };
}

#[cfg(not(windows))]
pub fn message(title: &str, text: &str) {
    eprintln!("{title}: {text}");
}

/// Opens a web page in the default browser.
pub fn open_url(url: &str) {
    #[cfg(windows)]
    let _ = std::process::Command::new("rundll32.exe").args(["url.dll,FileProtocolHandler", url]).spawn();
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(url).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let _ = std::process::Command::new("xdg-open").arg(url).spawn();
}

/// Checks the website for a newer download in the background. The answer
/// (the newer version, if there is one) lands in the returned slot.
pub fn check_for_update(app: App) -> std::sync::Arc<std::sync::Mutex<Option<String>>> {
    let slot = std::sync::Arc::new(std::sync::Mutex::new(None));
    if VERSION == "dev" {
        return slot; // your own builds don't nag
    }
    let out = slot.clone();
    let url = format!("{}/api/version", site());
    let _ = std::thread::Builder::new().name("update check".into()).spawn(move || {
        let agent = ureq::AgentBuilder::new().timeout(std::time::Duration::from_secs(8)).build();
        let Ok(res) = agent.get(&url).call() else { return };
        let Ok(v) = res.into_json::<serde_json::Value>() else { return };
        if let Some(latest) = newer_version(&v, app) {
            *out.lock().unwrap() = Some(latest);
        }
    });
    slot
}

/// The website's version for `app`, if it isn't the one we are.
pub fn newer_version(versions: &serde_json::Value, app: App) -> Option<String> {
    let latest = versions[app.key()]["version"].as_str()?;
    (latest != VERSION && !latest.is_empty()).then(|| latest.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_download_installs_itself() {
        assert!(is_download(App::Player, Path::new("Downloads/BrixoPlayer.exe")));
        assert!(is_download(App::Player, Path::new("/tmp/brixoplayer.EXE")));
        assert!(!is_download(App::Player, Path::new("target/release/brixo-player.exe")), "cargo builds don't");
        assert!(is_download(App::Player, Path::new("Downloads/BrixoPlayer (1).exe")), "a second download");
        assert!(!is_download(App::Player, Path::new("BrixoPlayerCheat.exe")));
        assert!(!is_download(App::Studio, Path::new("BrixoPlayer.exe")));
    }

    #[test]
    fn update_check_compares_versions() {
        let v = serde_json::json!({"player": {"version": "2026.09.26.1200"}, "studio": {"version": VERSION}});
        assert_eq!(newer_version(&v, App::Player).as_deref(), Some("2026.09.26.1200"));
        assert_eq!(newer_version(&v, App::Studio), None, "same version: nothing to do");
        assert_eq!(newer_version(&serde_json::json!({}), App::Player), None, "no downloads yet");
    }
}
