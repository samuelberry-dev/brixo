//! `brixo://` links: what Play on the website opens. Registering tells the
//! OS to hand those links to this app (run `brixo-player
//! --register-protocol` once after building or installing).

/// What a `brixo://play?server=...&ticket=...` link says.
#[derive(Debug, PartialEq)]
pub struct PlayLink {
    pub server: String,
    pub ticket: String,
    /// The game's name, for the window title and badge (optional).
    pub game: Option<String>,
}

pub fn parse(link: &str) -> Option<PlayLink> {
    let query = link.strip_prefix("brixo://play")?.trim_start_matches('/').strip_prefix('?')?;
    let (mut server, mut ticket, mut game) = (None, None, None);
    for pair in query.split('&') {
        let (k, v) = pair.split_once('=')?;
        match k {
            "server" => server = Some(decode(v)),
            "ticket" => ticket = Some(decode(v)),
            "game" => game = Some(decode(v)),
            _ => {}
        }
    }
    Some(PlayLink { server: server?, ticket: ticket?, game })
}

/// `%3A` -> `:` and friends (links arrive URL-encoded).
fn decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        match (b[i], s.get(i + 1..i + 3).and_then(|h| u8::from_str_radix(h, 16).ok())) {
            (b'%', Some(byte)) => {
                out.push(byte);
                i += 3;
            }
            (c, _) => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Tells the OS to open `brixo://` links with this program.
pub fn register() -> Result<String, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    register_exe(&exe)
}

/// Tells the OS to open `brixo://` links with `exe` (the installer passes
/// the installed copy).
#[cfg(target_os = "windows")]
pub fn register_exe(exe: &std::path::Path) -> Result<String, String> {
    // Under HKEY_CURRENT_USER, so no admin rights are needed.
    use winreg::RegKey;
    use winreg::enums::HKEY_CURRENT_USER;
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (key, _) = hkcu.create_subkey(r"Software\Classes\brixo").map_err(|e| e.to_string())?;
    key.set_value("", &"URL:Brixo").map_err(|e| e.to_string())?;
    key.set_value("URL Protocol", &"").map_err(|e| e.to_string())?;
    let (cmd, _) = hkcu.create_subkey(r"Software\Classes\brixo\shell\open\command").map_err(|e| e.to_string())?;
    cmd.set_value("", &format!("\"{}\" \"%1\"", exe.display())).map_err(|e| e.to_string())?;
    Ok(format!("brixo:// links now open {}", exe.display()))
}

#[cfg(target_os = "linux")]
pub fn register_exe(exe: &std::path::Path) -> Result<String, String> {
    let home = std::env::var_os("HOME").ok_or("no home folder")?;
    let dir = std::path::Path::new(&home).join(".local/share/applications");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let entry = format!(
        "[Desktop Entry]\nType=Application\nName=Brixo Player\nExec=\"{}\" %u\nNoDisplay=true\nMimeType=x-scheme-handler/brixo;\n",
        exe.display()
    );
    std::fs::write(dir.join("brixo-player.desktop"), entry).map_err(|e| e.to_string())?;
    let ok = std::process::Command::new("xdg-mime")
        .args(["default", "brixo-player.desktop", "x-scheme-handler/brixo"])
        .status()
        .is_ok_and(|s| s.success());
    if !ok {
        return Err("couldn't run xdg-mime to register the link type".into());
    }
    Ok(format!("brixo:// links now open {}", exe.display()))
}

#[cfg(not(any(target_os = "windows", target_os = "linux")))]
pub fn register_exe(_exe: &std::path::Path) -> Result<String, String> {
    Err("registering brixo:// links isn't supported on this system yet".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn play_links_parse() {
        let l = parse("brixo://play?server=127.0.0.1%3A4570&ticket=abc123&game=Coin%20Tycoon").unwrap();
        assert_eq!(l, PlayLink { server: "127.0.0.1:4570".into(), ticket: "abc123".into(), game: Some("Coin Tycoon".into()) });
        // Some browsers add a slash before the query.
        assert!(parse("brixo://play/?server=a&ticket=b").is_some());
        assert!(parse("brixo://play?server=a").is_none(), "needs a ticket");
        assert!(parse("https://example.com").is_none());
    }
}
