# BRIXO

A Roblox-style 3D game platform built from scratch in Rust: a website with
accounts and a game catalog, an engine, a desktop studio, a scripting language
(Rovik), game servers, a player app, audio, and a complete sample game. This file is the handoff: read it before changing anything.

Owner: Sam Berry · Repo: `github.com/samuelberry-dev/brixo` (branch `main`)
Dev machine: Windows, RTX 4070 Ti Super, repo at `C:\Users\Samuel Berry\brixo`

---

## 1. Running it

```
cargo build --workspace                  # build everything
cargo test --workspace                   # all must pass
cargo run -p brixo-web                   # the website: http://127.0.0.1:7420
target\debug\brixo-player --register-protocol   # once: lets Play buttons open the app
cargo run -p brixo-studio                # the editor (Publish uploads to the website)
cargo run -p brixo-samples -- file.brixo # Coin Tycoon as a file, to open in the studio
cargo run -p brixo-server -- "path\to\game.brixo" [--port 4570]  # a LAN test server
cargo run -p brixo-audio --bin brixo-sounds -- out   # export all sounds as WAV
```

**How Play works (the Roblox way):** the website runs the games. Pressing Play
starts (or reuses) that game's server inside `brixo-web`, issues a one-time
ticket (60 s, one game, one use) and opens
`brixo://play?server=...&ticket=...&game=...`. The OS hands that link to Brixo
Player, which joins the server with the ticket: the server asks the website who
the ticket belongs to, and the player appears under their username wearing
their saved avatar. Players never download games; scripts never leave the
server. Empty servers shut down after 60 s. Brixo Player has no menus: opened by
a link it plays that game, and leaving closes it.

The website stores accounts and games in `brixo-web.sqlite` (override with
`BRIXO_WEB_DB`; port with `PORT`). It publishes the samples (Flagfall, Spire
Wars, Coin Tycoon) under a "Brixo" account on start.

Online, three more settings matter: `BRIXO_PUBLIC_HOST` is the address Play
hands to Brixo Player (a domain or IP players can reach; default 127.0.0.1),
`BRIXO_GAME_PORTS` (like `7500-7599`) is the only ports game servers use, so
the firewall opens exactly those (full range = Play answers 503 "busy"), and
`BRIXO_WEB_BIND` is where the website listens (keep 127.0.0.1 behind Caddy).
Three switches harden it for the internet: `BRIXO_SECURE_COOKIES=1` (login
cookie only over HTTPS), `BRIXO_TRUST_PROXY=1` (visitor IPs come from Caddy's
X-Forwarded-For; only behind a proxy, or anyone could fake theirs) and
`BRIXO_INVITE_ONLY=1` (signup needs a code; the pages show the box when
`/api/stats` says `invite_only`, and `/signup?invite=CODE` fills it in).

Admin is the command line, on the same database (safe while the site runs,
WAL + busy timeout): `brixo-web set-password Brixo` (hidden prompt; signs
that account out everywhere), `brixo-web invite 5`, `brixo-web invites`.

Downloads (Roblox-style, Windows): `tools/release.ps1` (run on the Windows
PC: `powershell -ExecutionPolicy Bypass -File tools\release.ps1`) builds
Player and Studio with `BRIXO_BUILD=<yyyy.MM.dd.HHmm>` baked in, renames them
`BrixoPlayer.exe` / `BrixoStudio.exe`, writes `versions.json` and scp's all
three to `/var/www/brixo-downloads` (versions.json last). Caddy serves that
folder at `/files/`; the site reads versions.json for `/api/version` and the
Get Brixo page. The downloaded exe installs itself (`brixo-client/src/install.rs`):
only a file named like the download (or "BrixoPlayer (1).exe") does, never a
cargo build; it copies to `%LOCALAPPDATA%\Brixo\Player`, renaming a running
old copy aside, strips Zone.Identifier, adds Start Menu + desktop shortcuts and
an Apps entry (`--uninstall`) via PowerShell with paths in env vars, registers
brixo:// (Player), then starts the installed copy (`--installed` if no args).
Mac: GitHub builds it (`.github/workflows/mac.yml`, macos-14, both
aarch64 and x86_64, `tools/package-mac.sh` joins them with lipo into
"Brixo Player.app" / "Brixo Studio.app" with an .icns from
`assets/icon-1024.png`, an Info.plist (Player declares the `brixo` URL
scheme), an ad-hoc codesign, and a .dmg with an Applications link).
`release.ps1` starts it with `gh workflow run` (needs the GitHub CLI and
everything pushed), builds Windows meanwhile, waits, downloads the dmgs and
uploads all together; without gh it keeps the site's existing Mac entries.
versions.json keys: player/studio (Windows), player_mac/studio_mac. macOS
sends brixo:// links as an Apple Event, not argv: `brixo-player/src/mac_links.m`
(compiled by build.rs with cc on macOS) catches kAEGetURL and queues links
for Rust; Player checks the queue every frame, so Play also switches games
while it's open. macOS may pass `-psn_...`: flags are never game files.
Packaged-app paths use `install::is_packaged` (installed copy or inside a
.app) and `install::installed_places` (Studio finds Player.app). Not
notarized: first open needs System Settings -> Privacy & Security -> Open
Anyway. None of the Mac code has run on a real Mac yet.

Installing and uninstalling show a window (`brixo-client/src/installer.rs`):
the website's navy stud banner, the B R I X O brick logo, and ten bricks that
drop into sockets as each step finishes (steps run on a thread, each shown at
least 0.45 s, starting only once the window has drawn; "Ready!" holds 1.1 s).
It has its own winit loop, and a process only gets one, so it runs only when
the process then hands over or quits; if install fails it relaunches itself
with BRIXO_SKIP_INSTALL=1 to run from where it is. The icon
(`brixo-client/assets/icon.ico`, red brick with a white B) is embedded by
player/studio `build.rs` via winresource (needs rc.exe from the Build Tools;
without it the build warns and carries on), with ProductName/FileDescription
so Task Manager says "Brixo Player"; windows use `filming::brixo_icon()`.
Release builds hide the console (`windows_subsystem`). Player and Studio check
`/api/version` in the background and show "A new ... is out" (never for "dev"
builds). Studio saves games wherever you choose (File > Save / Save As,
native boxes via `rfd`; they start in `~/Brixo/My Games`, not `~/Brixo/games`,
which is Player's library), finds Player next to itself or installed, and publishes to `install::site()` (BRIXO_SITE
overrides, default https://playbrixo.com).

Built-in protection (`src/limits.rs`, in memory): 10 wrong logins per 15 min
per IP *and* per username (then 429, even for the right password), 5 signups
an hour per IP, 10 bad invite codes per 15 min, 30 Plays a minute and 30
publishes an hour per account. Passwords 8-128 characters, hashed off the async
threads. Names containing admin/moderator/official/staff, or "Brixo" itself,
are refused ("BrixoFan" is fine). Sessions last 30 days. Uploads up to 32 MB
(axum's default 2 MB refused Flagfall). Every response gets nosniff, DENY
framing and same-origin referrers.

The workspace `Cargo.toml` **must** keep:

```toml
[profile.dev.package."*"]
opt-level = 3
```

Without it, physics runs ~20x slower in debug builds.

Developer-only player options: `--join ADDR --name NAME` (the studio's
multiplayer test windows use it) and `brixo-player game.brixo`.

---

## 2. The crates

| Crate | Job |
|---|---|
| `brixo-core` | The data model. "Everything is an Instance": `DataModel` tree of `Instance { id, class, name, parent, children, props, attributes }`. Classes, props (`PartProps`, `PlayerProps`, `GuiProps`, `ScriptProps`), shapes, materials, faces, camera modes, JSON save/load. No engine code. |
| `rovik` | The scripting language: lexer, parser, tree-walking interpreter, friendly errors ("Did you mean...?"), `Host` trait for engine objects. |
| `brixo-runtime` | Runs a game: `Game` (scripts, events, players, tools, chat, sounds), `physics.rs` (Rapier), `host.rs` (what scripts can see and do). Headless; used by the studio, the player and the server. |
| `brixo-render` | wgpu renderer: shape meshes (block/wedge/cylinder/ball), avatar meshes (limb-split), face decals + material textures in one atlas, shadow map, retro lighting, picking. |
| `brixo-client` | Shared by every app that plays games: camera-relative controls, follow camera (collision, first person), games library/publish, GUI + hotbar drawing, chat UI, audio playback wrapper. |
| `brixo-studio` | The editor (winit + egui). One large `main.rs`. |
| `brixo-player` | The player app: library, local play, joining servers, HUD, pause, console, chat, mouse lock. |
| `brixo-server` | Multiplayer: TCP server (lib + bin), protocol, `NetClient`. `start_with_tickets` makes a server that only admits ticket holders, as their account. |
| `brixo-web` | The website (axum 0.7 + SQLite + Argon2): accounts, sessions, avatars, game catalog, publishing, Play (starts game servers in-process, issues tickets), and the HTML/JS pages (`src/web/`, embedded at build time). |
| `brixo-audio` | Synthesized chiptune sound effects and music (no audio files), `rodio` playback, WAV export. |
| `brixo-samples` | Sample games built in code (Coin Tycoon) + a headless full-game test. The website seeds its catalog from here. |

Pinned versions (they must agree with each other): wgpu 24, winit 0.30,
egui / egui-wgpu / egui-winit 0.31, glam 0.33, rapier3d 0.35, rodio 0.19
(`default-features = false`), serde/serde_json. Edition 2024.

---

## 3. How it fits together

- **The world is data.** Everything, including players and GUI, is an Instance in
  one `DataModel`. Rendering, physics, scripts and networking all read it.
- **Play never touches the edited scene.** Play runs on a *copy*; Stop throws it away.
- **Scripts:** each script task is its own OS thread, but only one runs at a time
  (channel handshake), so `wait()` can suspend anywhere. Event handlers run on
  forks of the script's interpreter. Scripts run on a 64 MB stack thread.
- **Physics:** Rapier at a fixed 1/60 s step. Anchored parts are **fixed** bodies
  (not kinematic: kinematic ground made the character controller snag). Loose
  parts are dynamic. Parts in a `Model` are welded (fixed joints, contacts off).
  Anchored parts with a velocity are conveyors. Characters are kinematic capsules
  driven by Rapier's `KinematicCharacterController`, one per player.
- **Multiplayer is server-authoritative.** The server runs the `Game`; clients send
  input/clicks/chat and draw what the server streams: the full world on join and
  on structure changes, per-tick `State` (parts, players, GUI) otherwise.
  Script source is blanked before sending. Inputs are capped server-side.
- **GUI is server-driven.** Server scripts create/modify GUI instances; GUI inside
  a Player is only that player's. Clicks come back as `on clicked(player)`.
  (There are no client scripts or RemoteEvents yet, by design.)
- **Smooth motion at any frame rate.** Physics and the server update 60 times
  a second, but screens draw 144+. `brixo_client::Smoother` keeps the last few
  updates on a steady clock and draws everything one update behind (about
  16 ms), blended between the two around that moment. The player app and studio
  Play draw, aim the camera and place labels from its smoothed copy of the
  world; studio panels still edit the live world. Teleports snap.
- **Animation is derived, not sent.** Physics writes `speed`, `airborne`; tools set
  `swing`; every client computes the same limb pose from those.
- **Sound:** scripts push `SoundEvent`s; the server routes them per player; apps
  play them through `brixo_client::Audio`. **Placed sounds** (`play_sound_at`,
  explosions, deaths) carry a `brixo_core::SoundAt` (the object it follows, and
  where it was when it started, for when the object's gone by the time a player
  hears it; `ToClient::Sound.at`, skipped when None, so old players just hear it
  everywhere). The listener is the camera: `Audio::listen` each frame
  re-resolves every playing placed sound and sets its per-ear volumes
  (`brixo_audio::spatial_gains`: full within `NEAR` 15 studs, then `NEAR/d`,
  fading to silence at `FAR` 250; equal-power pan), which the playing source
  picks up every 10 ms (`ChannelVolume` + `periodic_access`).

### Physics tuning (in `brixo-runtime/src/physics.rs`)

`GRAVITY = 110` (Roblox is 196), `FALL_GRAVITY = 1.4` (falls are heavier than
rises), `JUMP_HOLD_GRAVITY = 0.6` (hold to jump higher), `MAX_FALL_SPEED = 150`,
`COYOTE_TIME = 0.12`, `JUMP_BUFFER = 0.12`. Default `jump_power = 39`,
`walk_speed = 16`. Tap jump is about 7 studs, held about 11.5.

---

## 4. Rovik in one screen

```
-- comments; *** block comments ***
score = 0                         -- no keyword; script-level at top, local in fn
fn add(a, b)
    return a + b
end
if score >= 10 and not done then ... elseif ... else ... end
while x > 0 do x -= 1 end         -- break / continue work
for p in players() do print(p.name) end
on touched(other) ... end         -- events: touched, clicked, activated,
on player_joined(p) ... end       --         player_joined, player_left
every 2 seconds ... end
wait(0.5)
```

- `self` is the object the script is inside (not the script). Most common mistake.
- `on` is a keyword: never use it as a variable or parameter name.
- Missing custom fields read as `nil` (silently!), but typos of real fields of that
  class error with "did you mean". Custom fields: `player.cash = 100`
  (numbers, text, true/false).
- Builtins: `print len str num type push pop insert remove keys wait floor round
  abs min max sqrt sin cos asin acos atan2(y, x) random(lo, hi)`. Angles are in
  radians; part rotations are in degrees (multiply by 57.2958).
- Host functions: `find destroy clone time players create play_sound play_music
  stop_music explode`. `explode(position, radius[, power])` knocks out players
  in range (and returns them), throws loose parts, and shows a fireball.
- Events: `on touched(other)`, `on clicked(player)`, `on activated(player)`,
  `on player_joined(p)`, `on player_left(p)`, `on died(p)`, `on respawned(p)`
  (after they're back at a spawn: move them to a checkpoint here),
  `every N seconds`.
- Setting a player's `velocity` launches them (jump pads, knockback): it's
  turned into a push that fades fast on the ground and slowly in the air
  (`physics.launch`, `Character.push`).
- Players also have `look` (facing direction, `{x, y = 0, z}`), `swinging`, and
  `mouse`: the spot in the world their mouse pointed at when they last clicked
  with a tool (like Roblox's `Mouse.Hit`). Clicking also turns the character to
  face that spot, so `look` points there too. All three are read-only. (An older
  Player that sends no mouse point gets a spot far straight ahead.)
- Loose (unanchored) parts that fall below the world's edge (y < -60, where
  players die) are destroyed. Anchored parts stay, so templates can be kept far
  below the map. A team whose SpawnLocation fell off the world respawns where the
  pad last stood.
- Parts also have `floating` (no gravity) and `bounce` (0 to 1).
- `play_sound` / `play_music` take a built-in name or a Sound object
  (`play_sound(find("Horn"))`); Sounds have `volume`.
- Custom fields with meaning to the engine: `breakable = true` on an anchored
  part (blasts knock it loose; groups cut off from the ground collapse),
  `team` on a SpawnLocation and a player (respawn on your team's pad),
  `grip = "up"` on a Tool (held straight up, chops forward),
  `carried_by = "<player name>"` on a part (it rides along with that player,
  placed by `carry_x/carry_y/carry_z` in their own frame; nil lets go),
  `beacon = true` (+ `beacon_text`) on a part: a marker everyone sees through
  walls, pinned to the screen edge when it's off to one side. `camera.mode` (single player) / `player.camera_mode`.
- Lists are 1-based. `+` joins text. `!=` for not-equal.

---

## 5. Conventions

- **Every feature ships with tests**, including a headless game-level test when it
  touches gameplay. Tests are the spec; don't weaken an assertion to make it pass
  unless the behaviour legitimately changed, and say so in the commit.
- **Verify on screen.** Features aren't done until seen running (in the sandbox
  this meant Xvfb screenshots/videos; locally, just run it).
- **Beginner-first errors.** Every error a creator can hit says what went wrong and
  what to do, in plain words.
- **Comments explain why**, not what. Short doc comments on every public item.
- **Undo:** the studio snapshots the whole scene per edit (200 deep). Every edit
  path must call `editor.history.checkpoint(model)` *before* changing things.
- **Security by default:** validate every client request on the server (button
  ownership, tool ownership, input caps, chat limits).

---

## 6. Gotchas (all hit for real)

- **Never lock the world twice in one expression.** `game.world()` returns a
  `MutexGuard`; `game.world().x(game.world().y())` deadlocks. Bind it once:
  `let w = game.world();`.
- **Never hold the world lock across a frame** when another thread needs it (the
  studio's server view copies the world instead; holding it starved the server).
- **egui focus:** check `response.lost_focus()` *before* calling `request_focus()`
  (chat's Enter didn't send because of this).
- **Rapier character controller:** don't apply gravity while grounded (it snags on
  the floor); decide what's underfoot with a short down-ray, since the grounded
  flag flickers.
- **Sockets:** reader threads hold socket clones; shut sockets down explicitly on
  disconnect or they never close.
- **Linux/X11 has no true cursor lock:** mouse look falls back to
  confine-and-recentre.
- **Shaders are only validated by the GPU at app launch.** A WGSL mistake
  crashes both apps on start. `brixo-render`'s `the_shader_compiles` test runs
  the same checker (naga), so keep it passing, and still launch the app after
  shader changes.
- **Textures have no mipmaps.** Fine detail far away shimmers, so the shader
  fades a material's detail to its average shade by distance. Keep material
  textures made of soft shapes a few pixels wide, not per-pixel noise.
- **`cd dir && program &` backgrounds the `cd` too:** the rest of the script
  stays in the old directory. Use full paths.
- **Windows setup:** config text goes in files, not PowerShell. `cargo new` makes a
  nested `.git`. Downloaded files may need renaming. Run `cargo add` in the right
  crate or use `--package`.
- **axum 0.7 route parameters are `:id`, not `{id}`** (that's 0.8). With the
  wrong syntax the route silently never matches and returns a bare 404.
- **Never `pkill -f` a pattern that also appears in the same shell command**:
  it kills the shell running it. Use `pkill -x program-name`.
- **Tickets expire in 60 s.** When testing Play by hand, open the link quickly.
- **Scripts run wherever they are, including templates in a Storage folder.**
  A template with a self-destructing script destroys itself at game start.
  Guard such scripts with `if self.parent.name != "Storage"` (a real
  ServerStorage-like place where scripts don't run is on the roadmap).
- **`touched` can fire several times in one step** (a projectile landing on
  several bricks). Scripts that destroy `self` should use a `gone` flag.
- **Loose bricks must not start overlapping:** physics pushes them apart and
  walls fall down. Leave a hair of space between stacked parts.
- **Sandbox only:** builds there use `CARGO_PROFILE_DEV_DEBUG=0` or the disk
  fills with debug info.
- **Names are how scripts find things:** two objects with the same name make
  `find` return whichever comes first (a weapon's template and the ball it
  throws were both "Superball Template"). Keep template names unique.
- **Players are never "ground":** the character's ride-along logic ignores
  other players (standing by a teammate who respawned used to drag you
  across the map).
- **The filming kit** (`tools/film.py`, `tools/edit.py`): films every shot of
  the reveal trailer on this computer, hands-off, then cuts it to
  `tools/trailer-music.mp3` (kept on your PC only, git-ignored: not ours to publish). Needs ffmpeg (and Playwright for the website
  clip). Everything lands in `tools/footage/` (git-ignored). Explosion
  moments come from the player's sound logs, refined by finding the fireball
  in the footage; walks wait for real arrival (BRIXO_POSITION_FILE), so it
  works however fast the computer is.
- **The Flagfall trailer** (`tools/film_flagfall.py`, then
  `tools/edit_flagfall.py`): films establishing shots and a whole bots-only
  match with a camera that follows the action, logging every pickup, drop,
  return and capture (and whether it was on camera) to
  `footage/ff_events.json`; the editor picks moments from that log and cuts
  them to `tools/flagfall-music.wav` (git-ignored, on your PC only; Fuzzeke, "Wild Fight"; the beat map is
  in edit_flagfall.py's docstring). A dull match: run the filming again.
- **The double-lock trap (it cost two turns once):** a `game.world()` guard
  lives until the end of its statement, and an `if let` / `match` /
  `while let` keeps it for the whole block. Calling `world()` again inside
  (including across a line break: `game\n.world()`) makes the thread wait on
  itself forever. Bind the first result to a variable. In tests, use
  `game.world_within(timeout)`: it panics at the calling line with an
  explanation instead of hanging.
- **Small fast parts use continuous collision detection** (any loose part
  under a stud thick), or pellets and paintballs skip through players
  between physics steps.
- **Scripts in destroyed objects stop at once.** A script that still has work
  after hitting something (fading a mark) must hide its object first and
  destroy it last.
- **Unique names matter for `find`:** it returns the first match. Flagfall's
  castle banners were once named "Banner", same as the GUI banner, and the
  victory screen tried to colour a part. GUI and script-found things need
  names nothing else uses.
- **Depth runs backwards** in the scene pass (`reversed_depth` in brixo-render:
  near = 1, compare Greater, clear 0) for even precision at any distance, and
  each part gets a tiny per-part depth nudge (`InstanceRaw::layer`, from its
  id) so parts overlapping flush never flicker. `Camera::view_proj` itself is
  unchanged (picking and projections use it); only the uniform is reversed.
  The shadow map still runs the usual way. See-through parts (transparency
  above 0) use a second, blended pipeline with depth writes off, one draw
  per part sorted back to front; they're left out of the shadow pass.
- **Music switching** crossfades (0.9 s), and two songs of the same loop
  length continue from the same point in the loop (`brixo_audio::sync_offset`).
- **Gears:** `brixo_samples::gears` has the standard kit (Sword, Slingshot,
  Rocket Launcher, Superball, Trowel, Paintball Gun). `install(dm, storage)`
  adds the templates; clone `"<Name> Template"` into players. Gear Range
  (`brixo-samples gear`) is the test map. Flagfall and Spire Wars both use it
  (Spire adds its own Timebomb). Everything that shoots aims at `player.mouse`
  and leaves from the gear in the right hand (`aim(p, ahead, up, right)` in its
  helpers); shots ignore the shooter's own gear (`own_gear`). Models are built
  pointing along +Z, the first part in the hand; **cylinders stand along their
  y**, so a barrel or handle lying along the gear is turned 90 degrees on x
  (`rod(...)` in gears.rs). Rockets are long along their y and turned to point
  where they fly (`point_along`). Bots (`flagbots`) aim with some wobble.
- **Test hooks** (inert unless set):
  - `BRIXO_SOUND_LOG`: log sounds with timestamps (first line: `# epoch <unix time>`).
  - `BRIXO_GOTO_FILE`: the player walks toward "x z" in that file.
  - `BRIXO_ACTION_FILE`: lines another program appends ("equip 6", "use", "jump", "say hi", and "menu", "help", "settings" or "leave" to open the Game Menu on that page).
  - `BRIXO_POSITION_FILE`: the player writes "x y z" there every frame.
  - `BRIXO_CAMERA_FILE`: "x y z lx ly lz" places the camera (position, look-at).
  - `BRIXO_STUDIO_CAMERA` (Studio): "x y z lx ly lz" starts the build camera
    there. Used for the Learn guide's Studio screenshots.
  - `BRIXO_CINEMATIC`: no interface at all.
  - `BRIXO_WATCH_FILE` (player): every player (name, position, facing,
    knocked out, team, flag carried) and every beacon part, each frame. The
    Flagfall director follows the action with it.
  - `BRIXO_TIME_SCALE` (server and the bots example): run the game slower
    than real time (e.g. 0.25), so a slow machine renders every moment;
    speed the footage back up afterwards.
  - `BRIXO_WINDOW_SIZE` ("1920x1080", borderless at the top-left) and
    `BRIXO_WINDOW_TITLE`: for screen recorders (player and studio).
  - (The studio's self-filming mode, `BRIXO_STUDIO_DEMO`, and its demo
    world are gone: they were built for the old layout. `tools/film.py`'s
    studio shots need redoing against the new Studio if the trailer is
    ever re-filmed.)
  - `BRIXO_WEB_DB`, `PORT`: the website's database and port.
  - `BRIXO_PUBLIC_HOST`, `BRIXO_GAME_PORTS`, `BRIXO_WEB_BIND`: where players
    reach game servers, which ports they use, where the site listens.
  - `BRIXO_SECURE_COOKIES`, `BRIXO_TRUST_PROXY`, `BRIXO_INVITE_ONLY`: the
    internet switches (see section 1).
  - Page tests: the banner has its own login form, so click
    `#form button[type=submit]`, never the first submit button on the page.
  - Joining by name: "localhost" is ::1 first on Windows, and game servers
    listen on IPv4, so `connect_any` tries every address. It also rejects a
    socket connected to itself (local addr == target): with nothing on that
    port, Windows' in-order local ports can make TCP connect to itself, which
    looks connected but only echoes our own Hello. Tests keep game ports in
    20000-30000, below the OS's outgoing-port range, like 7500-7519 live.

---

## 7. What exists

**Engine:** shapes (block, wedge, cylinder, ball); materials (plastic, wood, brick,
metal, grass, concrete, neon); real see-through parts (alpha-blended, drawn last, farthest first, casting no shadow); shadows; retro lighting;
welded Models; conveyors and script velocity; character with coyote time, jump
buffering, variable jumps; riding moving ground; respawn.

**Look:** one shared Brixo theme (`brixo_client::theme`) for the studio and
player interfaces, matching the website: navy panels, Brixo-blue selection, gold
accents; the Explorer shows a coloured badge per class.

**Studio editing:** left-drag on empty space box-selects (everything
fully inside; Ctrl/Shift adds; Alt picks parts inside Models); Snap has a
step (¼ to 4 studs) and a turn step (5° to 90°); with several things
selected, Properties' **Line up** aligns low sides / middles / high sides
along X, Y or Z, or spaces them evenly (`editing::align`,
`space_evenly`). Drag parts over surfaces in the 3D view (a left-drag
starting on a selected part, away from the gizmo: it rests on whatever's under
the mouse, snapped to whole studs); the Explorer has drag-and-drop to move
things between folders and models, rename in place (double-click or F2) and a
right-click menu; Ctrl+C / Ctrl+X / Ctrl+V go through the system clipboard
(`DataModel::to_clipboard` / `paste_clipboard`), so things copy between
games; scripts open in tabs beside the World tab (double-click a Script,
or Properties > Edit script; Play switches to the World; Ctrl+W closes a
tab); the script editor has syntax colouring, line numbers, the error line
shaded, and autocomplete (arrows to choose, Tab to accept, Escape to hide).
The testable parts live in `brixo-studio/src/editing.rs`.

**Studio:** Explorer (Ctrl-click multi-select), Properties (parts, GUI, materials,
velocity), Move/Rotate/Scale gizmos with snapping, group move/rotate, undo/redo,
copy/paste/duplicate, group/ungroup (Ctrl+G/U), F to focus, Add Part/GUI/Tool,
script tabs with live syntax check, Output panel, Play/Stop (F5), Players count
(local server + a window per player), Publish, File menu (New = a baseplate
and a spawn; Open; Save / Save As with a `*` in the title and "save your
changes first?" before New, Open, a dropped file or closing; Open a sample
game). Studio is dressed like the website (`theme::apply_site`: white boxes,
glossy blue title bars, the stud banner and brick logo, the guide's dark code
box); the Player keeps the dark `theme::apply` for menus over the game.

**Avatar:** blocky body, round head, no shoes (the pants reach the ground; the
`shoes_color` value still exists for scripts and the network, it just isn't
drawn), limb animations (walk, jump, hold, swing), first/third person, camera
collision. Everything worn is in `brixo-core/src/cosmetics.rs`:
- **Body colours:** six parts (head, torso, arms, legs), each its own colour
  from the 32 classic brick colours (`BODY_PALETTE`), like old Roblox's Body
  Colors. `PlayerProps.body_colors` (None for old avatars: skin, torso in the
  shirt colour). Scripts' `skin_color` sets head, arms and legs.
- **Clothes:** a `Shirt` style (sleeves none/short/long) worn in `shirt_color`,
  `Pants` (long or shorts) in `pants_color`, and a `TShirt` picture on the
  chest. Clothes are shells a little bigger than the body parts; their prints
  are grey shading multiplied by the colour (chunky 32x32 pixel art in
  `brixo-render/src/avatar/art.rs`), so any colour works. A script setting
  `shirt_color` on someone shirtless puts a tee on (team colours always show).
- **Faces:** 14 pixel-art faces, in colour (tongues, heart eyes).
- **Accessories** (`Hat`, still "hats" in code and scripts): 29, one per slot
  (`HatSlot`: head, face, neck, back; `MAX_HATS` = 4). Head and face ones are
  part of the head; neck and back ones part of the body.
- Everything the avatar prints lives in one atlas (`avatar::face_atlas`, a
  16-wide grid of 64px cells: white, faces, materials, shirts around/front,
  pants, t-shirts). Meshes say which colour (`Slot`), which picture (`Tex`)
  and who they show on (`Show`).
- Scripts: `player.shirt`, `pants`, `tshirt` (names), `hats` (a list).

**Brix, the Catalog and challenges** (`brixo-web/src/shop.rs`, `db.rs`):
Brix are earned, never bought: 50 to start, 10 for the first visit each day
(`/api/me` gives it), and challenges. A game calls
`complete_challenge(player, "name")`; the runtime asks its `SaveStore`
(the website's `SiteSaves`), which adds unknown names as *waiting* (pay 0,
silent) until an admin approves them with a title, Brix (0-500) and daily or
once (admin page, "Challenges & Brix"). Paid challenges say so in everyone's
chat (a nameless gold line: `Game::take_chat` merges `WorldHost::notices`).
200 Brix a day from challenges at most; 40 challenges per game at most.
The Catalog (`/catalog`, and the Character page) lists every face, shirt,
pants, t-shirt and accessory with a price; a starter set is free, admins own
everything, prices can be changed on the admin page (`prices` table
overrides `shop::default_price`). `PUT /api/avatar` refuses things you don't
own. Up to 5 saved outfits. `brix_log` records every Brix given or spent.
The first start after the Catalog arrived (`meta` "catalog-v1") let everyone
keep what they wore and gave them the welcome Brix. The sample games'
challenges are seeded approved (`api::sample_challenges`).

**Website avatars are 3D:** the site draws characters with a small WebGL
renderer in `app.js` from `web/avatar-model.json` and `web/avatar-atlas.png`,
which are the game's own meshes and pictures. Both are generated: after changing the avatar
or a hat, run `BRIXO_WRITE_MODEL=1 cargo test -p brixo-render web_model`
(PowerShell: `$env:BRIXO_WRITE_MODEL=1; cargo test -p brixo-render web_model;
Remove-Item Env:BRIXO_WRITE_MODEL`). A test fails if it's out of date. The
avatar page's preview can be dragged to turn it around.

**Gameplay systems:** GUI (TextLabel/TextButton/Frame, per-player or shared,
labels attached to parts), tools with hotbar (1-9) and `on activated`, chat
(filtered, rate-limited, bubbles), custom fields, sounds and music.

**Platform:** website (sign up, log in, avatar editor, catalog with live player
counts, Play), `brixo://` links registered with the OS, ticketed game servers
started on demand, Studio publishing (log in once, then Publish).

**Apps:** studio, player (opened by Play: HUD, pause menu, F9 console, chat,
mouse lock), dedicated LAN server.

**Player menus** (`brixo-player/src/menus.rs`): the home screen and the
update notice are dressed like the website (`theme::apply_site`). **In a game
it's all the classic early-2000s look** (`brixo-client/src/classic.rs`,
Roblox 2006-2010 as the reference): Arial-shaped bold text (Liberation Sans,
OFL, bundled in `brixo-client/assets/fonts`, loaded by `apply_site` as the
"classic" font families) with one-pixel black outlines; see-through black
panels with square corners and hard edges; grey Windows-2000 bevelled
buttons. Top left: a toolbar (Menu, Help, Fullscreen; F11 too) and under it
the chat bar ("To chat click here or press "/" key"), with chat lines under
that (names in the classic eight colours, words in white, no box). The
**health bar** is the classic one: a slim upright bar at the right edge,
green over red, draining from the top, "Health" under it in blue
(`classic::health_bar`, also in Studio's Play). The hotbar is square slots,
the one in hand framed in white; the leaderboard is outlined text on
see-through black, yours in yellow. The game's own GUI is drawn square with
a one-pixel edge, buttons bevelled (pushed in while pressed), and text
shrinks to fit its box. Esc (or Menu) opens the **Game Menu**: big grey
buttons, Reset Character and Leave Game each ask "Are you sure?", Settings
(shift lock on Shift, camera speed, music and sound volume) and Help (the
keys). Settings live in `~/Brixo/settings.json` (`settings.rs`), saved once
the menu closes; volumes go to `Speaker::set_levels`.

**Shift lock** (Shift, in Brixo Player): the mouse locks and turns the camera
(the same lock as first person), the camera sits `SHOULDER_OFFSET` (1.75)
studs right (`FollowCamera::shoulder`, sliding across), a crosshair marks the
middle (tools aim there), and the character faces the camera's way while
walking in any direction: `Game::set_facing` / `ToServer::Face { yaw }` ->
`Physics::hold_facing`. Pause frees the mouse.

**Studio's Play matches the Player:** Shift toggles shift lock (same shoulder
camera, crosshair, facing), `/` or Enter opens a chat box (said through
`Game::chat`, so it's filtered and shows as a bubble), and the mouse locks for
first person and shift lock; Esc or leaving the window frees it (a "Mouse
free" pill says so, click the world to lock again). Stopping Play resets all
three. Keys: `Editor::play_key`; the lock: `Editor::update_mouse_lock` (raw
mouse motion via `device_event`, confined-cursor fallback via `CursorMoved`).
**Reset character** is `ToServer::Reset` (health to 0). Servers now skip a
message they don't understand instead of dropping the player, so a newer
Player can talk to an older server; deploy the website before releasing a
Player that sends new messages anyway, since servers from before this don't.

**Saved player data** (`brixo_runtime::saves`): `save(player, "key", value)`
and `load(player, "key")` (nil if nothing's saved). Numbers, text,
true/false and lists/maps of them; parts and players are refused with an
error that says to save the name instead. Limits: 64 KB of JSON per player
per game (`SAVE_LIMIT`), names up to 50 letters; saving nil deletes. Data
is loaded when the player joins (so `load` works in `on player_joined`) and
written every 15 s if changed (`SAVE_EVERY`), after `on player_left`
handlers run, and when the Game is dropped. The store is a `SaveStore`
trait: online it's `SiteSaves` in `brixo-web/src/api.rs` (the `saves`
table, one row per game_id + user_id; the server's `Identity.save_key` is
the account id, passed to `Game::add_player_saved`), in Studio a
`MemoryStore` that lives until Studio closes (so Play, Stop, Play keeps
it; a new or opened game clears it), and a plain local game gets its own
`MemoryStore`. Guide page: `howto-saving`.

**Chat filter** (`brixo_runtime::chat_filter::filter_chat`, also used for
usernames, blurbs and descriptions on the website): hides swearing, slurs,
sexual words and self-harm phrases with #s, and is deliberately loose about
ordinary gaming talk (noob, dumb, loser, hell, damn, shut up, "I'll kill
you" all pass). Three lists: `ANYWHERE` (blocked even inside a word:
fuck, shit...), `WORDS` (whole word plus an `ENDINGS` suffix, so
Scunthorpe, Dickens, assassin, cocktail pass) and `PHRASES` (kill
yourself, kys...). It sees through leetspeak (`plain`), stretched letters
(3+ repeats squeezed, so "shiitake" is fine), stars for letters (same
first and last letter; stars round a word are emphasis: "*sigh*"), and
spelled-out letters ("f u c k", "k y s"). Unit tests list what must pass
and what must be caught: add to both when changing a list.

**Moderation** (no report button yet, on purpose): `users.banned`,
`users.admin`, `games.hidden`. A banned account can't log in (403), its
sessions are deleted, its game tickets are refused, it's kicked from every
running server (`Servers::kick_everywhere` -> `ServerHandle::kick`), and
its games and profile vanish from the site (`LISTED` in `db.rs`). A hidden
game drops out of listings, Play gives 404, and its server is stopped
(`Servers::stop`). Admins get an **Admin** tab (`/admin`, `web/admin.html`:
games and accounts with search, Take down / Put back, Ban / Unban; API
`/api/admin`, `/api/admin/ban`, `/api/admin/hide`). Admins can't ban
themselves or the Brixo account. On the server:
`brixo-admin admin NAME` / `unadmin`, `ban` / `unban NAME`,
`hide` / `show GAME_ID`, `reset NAME` (prints a reset link; `BRIXO_SITE`
sets its address, playbrixo.com by default).

**Website layout** (old Roblox's flow): Home logged in is "My Brixo" (you and
your Brix on the left; Friends Online with Join, Continue Playing from the
`plays` table, Popular, Your Games in the middle; `GET /api/home`). Home
logged out and Games open with a big **featured game** banner (an admin
features one with "feature" on the admin page, else the most popular with a
picture). Profiles have a header card (avatar, online/playing status: the
game and Join only for friends, About Me, stats) over games and friends. The
Friends page splits Playing now / Online / Offline, with requests on top.
Shared helpers in `app.js`: `featureBanner`, `personCard`, `avatarHead`,
`wireJoins`.

**Friends** (website): requests (`friend_requests` table: from, to) and
friendships (`friends`: pairs stored once, lower id first); asking someone
who already asked you makes you friends; up to `MAX_FRIENDS` (200); banned
accounts drop off lists. `users.last_seen` is touched (at most once a
minute) whenever a logged-in request comes in (`api::user`), and within
`ONLINE_SECONDS` (3 min) counts as online. Who's in which game comes from
the running servers (`Servers::who_is_where`, from each server's
`ServerHandle::player_names`, bots left out), shown only to friends.
Joining a friend is just Play on their game (one server per game). API:
`GET /api/friends` (friends with status playing/online/offline and the
game, playing first; requests; sent), `POST /api/friends/add|accept|
decline|remove {username}` (add rate-limited, `FRIEND_ASKS`). `/api/me`
has `friend_requests` (the Friends tab's red badge); `/api/users/:name`
has `friend_count`, up to 9 `friends` and your `relation` (none, friends,
sent, received) for the profile's Add Friend button. Page: `/friends`
(`web/friends.html`, refreshes every 20 s).

**Toolbox** (Studio's, like Roblox Studio's): ready-made things creators
insert. An item is what Studio's copy makes (`DataModel::to_clipboard`
text), so inserting is a paste (`insert_from_toolbox` in Studio: pasted
into the Workspace, moved to stand where the camera looks, parts parked
below y = -100 left alone, e.g. the gear kit's templates). Items live on
the website (`toolbox` table: name, category, description, content,
PNG thumbnail; `GET /api/toolbox`, `/api/toolbox/:id`,
`/api/toolbox/:id/thumbnail`). Only admins add to it: the Admin page's
Toolbox box (paste what you copied in Studio, optional PNG) or
`brixo-admin toolbox` / `toolbox-add NAME CATEGORY FILE [--picture PNG]
[--description TEXT]` / `toolbox-remove ID`; items are checked to paste.
Removing hides an item (a removed built-in stays removed). Built-in items
are in `brixo_samples::toolbox` (Kart, Boost Pad, Swinging/Sliding Door,
Moving Platform, Spinner, Kill Brick, Checkpoint, Jump/Speed Pad,
Teleporter Pair, Coin, Gear Kit), seeded by slug at startup
(`seed_toolbox`) and updated with each release; their scripts only use
`self` (never `find`), so any number of copies work in any game. Their
pictures (`brixo-samples/assets/toolbox`) are rendered by the engine:
`cargo run -p brixo-samples --example toolbox_scenes DIR` writes a scene
and camera per item, then Brixo Player (BRIXO_CAMERA_FILE, BRIXO_CINEMATIC)
and a square crop. Studio's panel (`brixo-studio/src/toolbox.rs`) loads
the list and pictures on a thread; offline it shows the built-in items.
No settings on items: creators edit the scripts.

**Passwords:** no email yet, so resets go through an admin. The Admin
page's **Reset** (or `brixo-admin reset NAME`) makes a one-time link,
`/reset#TOKEN`, good for an hour (`RESET_SECONDS`; the `resets` table, one
per account, a new one replaces the old). The token is after the `#` so it
never reaches server or Caddy logs; `web/reset.html` posts it to
`/api/reset/check` and `/api/reset` (both rate-limited per address,
`BAD_RESETS`). Using it sets the password, logs the account out
everywhere and logs this browser in. Logged-in people change their own
password on their profile (`PUT /api/me/password`, needs the current one;
logs out their other sessions, keeps this one).

**Client prediction** (`brixo_client::Predictor`, Brixo Player online):
your own character runs on a local copy of the physics with the keys as
you press them, so it moves at once instead of a round trip later. The
predictor remembers ~1 s of its path; the server's position should be
somewhere on it. Off the path by more than 3 studs (`SNAP`) means the
server moved you (respawn, teleporter), so it restarts from there;
smaller drift above `DEAD_ZONE` is eased out at `EASE` via
`Physics::nudge` (keeps momentum); when both sides stand still it settles
exactly on the server. Everyone else and every part still come from the
server. ~1 ms a frame on Flagfall. `BRIXO_NO_PREDICT=1` turns it off.
Held tools: the server puts a tool in its holder's hand, but the holder is
drawn smoothed (or, you, predicted ahead), so the tool trailed behind.
`smooth::hold_tools(view, source)` moves each held tool's parts to where
its holder is drawn, keeping where they sit on the holder in the game's
latest state (Player, both backends, and Studio's Play).

**Death:** at 0 health (or falling off the world) the character falls apart
(head, torso, arms and legs scatter and land, computed from `PlayerProps::dead`
so every client matches), everyone hears it, `on died` fires, and they respawn
after `RESPAWN_TIME` (4 s).

**Tools in hand:** `brixo_core::held_arm_angle` is the one definition of the
holding arm (grip and swing). The renderer draws the arm with it and the
runtime places the tool along it, so they always line up. Tools are authored
pointing along +Z with the handle as their first part; the grip is recorded
once, from the tool as built.

**Custom audio:** drop an mp3, wav or ogg on the studio window and it becomes
a `Sound` (the file stored base64 in the game). World updates leave the audio
out; the server sends each player each Sound's file once (`ToClient::Asset`).
Playback uses rodio's decoders; decoded files are cached.

**Camera controls (Player and Studio's Play):** right-drag or Left/Right
arrows turn, Page Up/Down tilt (fn+Up/Down on a Mac), scroll or I/O zoom,
Up/Down walk like W/S (Roblox's keys; `brixo_client::keyboard_look`).
Trackpads: pinch zooms and a two-finger sideways swipe turns
(`trackpad_look`); many trackpad users can't right-drag at all. Building in
Studio: arrows and Page Up/Down steer the fly camera, scroll/pinch fly it.

**Sky:** a per-pixel shader sky (gradient, sun disc, flat two-tone clouds) and
distance haze toward the horizon colour.

**Lighting** (`brixo_core::Lighting`, `brixo_render::Sky`): kept as
Workspace custom fields (`time_of_day`, `brightness`, `fog_start`,
`fog_end`, `fog_color`/`sky_color` as "r,g,b" text), so it saves and
replicates with no protocol change; default values leave no fields.
Scripts get typed access on the Workspace (`find("Workspace").time_of_day
= 19`, colours as `{r, g, b}`, `nil` resets). `Sky::of` works out the
shader's colours on the CPU (tested): the sun follows the clock (rises at
6 in -X, sets at 18 in +X), dusk turns the horizon orange, night is a
dark sky with hashed stars and a dim blue moonlight (kept very low: the
screen is sRGB). **Time 14 reproduces the classic look exactly** (the old
`SUN_DIR`, colours and 0.45 + 0.7 lambert), so screenshots stay valid.
Fog mixes to `fog_color` from start to end, and thick fog hides the
horizon. Studio: select the Workspace for the Lighting section.

**Hinges and motors** (`PartProps.hinge`/`hinge_at`/`motor_speed`/
`swing_to`; `Physics::sync_hinges`): a loose part with a hinge gets a
Rapier revolute joint (generic joint, free AngX turned onto the hinge
line) at `Side::point` on its hinge line. What it hangs on is picked when
the joint is built: of the parts its world box touches, the one nearest
the hinge point, skipping parts welded to it and its Model's other hinged
parts; nothing touching means a fixed anchor body in the world. Contacts
between the two are off (a wheel sits against its car). Motors are
acceleration-based: `swing_to` a position motor (stiffness 300, damping
40), else `motor_speed` a velocity motor, else no motor; hinged bodies get
angular damping 3.0 so free hinges settle. A change to hinge, side or size
(or a script teleport) rebuilds the joint. In a Model, hinged parts aren't
welded, except a hinged **first** part, which carries the rest (a
multi-part door). Anchored parts ignore hinges (Studio and scripts
unanchor when a hinge is turned on). `hinge_angle` (read-only) comes from
`Physics::hinge_angles` via `WorldHost.hinge_angles` each step. Studio
draws an orange line along a selected part's hinge. Remember: all wheels
share one axle direction, so the same `motor_speed` drives straight.

**Leaderboard** (`brixo_client::leaderboard`): `leaderboard("coins",
"wins")` sets the Workspace field `leaderboard` ("coins,wins"); the board
shows those player custom fields (up to 5), sorted by the first, grouped
under teams (`team` field, coloured from its name, with the team's total).
Headings from field names (`best_time` "Best Time"; one or two letters,
plus a plural s, are capitals: `kos` "KOs"). Drawn top-right in Brixo
Player and Studio's Play, shown when a game asks or there are 2+ players;
Tab or the title bar's arrow folds it. Flagfall shows captures/returns/
KOs, Coin Tycoon cash.

**Sample games:** Spire Wars (a Doomspire-style remake: four team towers of
breakable bricks that collapse, six weapons (sword, rocket launcher,
superball, slingshot, trowel, timebomb), auto-balanced teams, timed rounds with
team scores and a map rebuild, a custom horn Sound, and its own march theme from `synth::spire_wars_theme`: all sample-game music is made in code, nothing licensed) and Coin Tycoon (4 plots, droppers, conveyor, furnace, upgrader,
6 upgrades, music and sound), with a full headless playthrough test.
Flagfall (capture the flag, below).

**Flagfall** (`brixo_samples::flagfall`, `brixo-samples flag`): Red vs Blue
across a river. Three routes: the Bridge (middle, exposed; you can wade
under it), the Ruins (south flank: stepping stones through a breach in a
broken wall, all breakable and rebuilt each match), the Tunnel (north
flank: stairs down by each castle, under the river). Castles have a gate
between two towers, two side doors (north faces the tunnel, south the
ruins), stairs up to a wall-walk with battlements, a spawn house, and the
flag stand in the courtyard. Rules (all in the Game script): touch the
enemy flag to take it; carriers are slower (11.5 vs 16); knocked out, the
flag drops where you fell for 10 s; a defender touching it sends it home;
you score at your stand only while your own flag is home; first to 3 or
most after 8 minutes, tie means sudden death. Healing after 5 s out of the
fight; wading the river slows you. Music is composed in code
(`brixo_samples::synth`): a calm and an intense mix of one song, switched
on the beat while a flag is out. Workspace fields `match_seconds` and
`caps_to_win` shorten matches (tests use them). The Game script has two
`every` loops on purpose: the rules loop pauses 8 s for the victory
banner, and the HUD loop keeps the screen current meanwhile.
**Bots:** `brixo_samples::flagbots::FlagBot` reads only the world a client
sees and returns keys. Jobs: attackers on each route (the teams hand out
routes in a different order), one defender per four; everyone switches to
carrying home, returning, chasing the carrier or escorting as needed. The
`bots` server example uses it automatically on a Flagfall server, and
`tests/flagbots.rs` plays a 5-minute bots-only match headless.

**Test Lab** (`brixo_samples::lab::test_lab`, `brixo-samples lab`): every
feature in one map, for testing. Doors and hinges (north: swing door that
opens away from you, saloon doors, drawbridge on a lever between high banks
over "water" that sends you back, trapdoor stage, swinging sign, the old
sliding door), garage (east: two cars, wheels named "Wheel L"/"Wheel R";
buttons over each car set motors, negative speed drives +Z, Stop brakes by
holding each wheel's `hinge_angle`, buttons show only while someone's
within 25 studs), playground (west: spinner, carousel, windmill as a Model
with a hinged first part, swing), gear range (south: the whole kit, targets,
a breakable tower in a Folder so it breaks apart, Explode and Rebuild from
a Storage template 300 studs down), movement (north-east: pads,
teleporters, conveyor, moving platform, lava, ball pit), plaza coins that
respawn, team pads, and on screen the leaderboard (coins, KOs from
`last_hit_by`, saved visits) and a Lab controls panel (cloned per player
from Storage: lighting presets, fog, brightness, sky, day cycle, music).
`tests/lab.rs` drives it all headless. On the site it's published by Brixo
as **admins only** (`games.admin_only`: out of every listing, 404 to
others, played from the Admin page's Play button; any game can be switched
with "admins only" / "make public", `POST /api/admin/admin-only`).

**Karts** (`brixo-runtime/src/kart.rs` shared; `physics/kart.rs` the
engine): a kart is a Model with `kart = true`. Its chassis ("Chassis", else
the first part) is a kinematic box (the bounding box of all its parts)
moved by Rapier's KinematicCharacterController; every other part is posed
from it by `pose_parts`, from `kart_at`/`kart_q` offsets recorded once on
each part (so the server sends only the chassis; clients `pose_all`). Parts
named "Wheel..." roll with `odo`, "Front Wheel..." also steer. Driving is
local input: `move_z` throttle, `move_x` steer (A = +1), jump = drift (hop,
slide, release for a 0.6/1.3 s boost at `DRIFT_MINI`/`DRIFT_SUPER` charge).
Karts write `speed`, `steer`, `odo`, `drift` (0/0.5/1/2), `boosting`,
`spinning` on the Model; scripts set `top_speed` (70) and `locked`. Leaving
a ramp's lip throws it (`climb`, held briefly as it fades, becomes `vy`).
The kart's box is a round cuboid (edges rounded 0.35) so it glides over
small lips; only side-on hits (normal |y| < 0.6; Rapier's `normal1` points
out of the obstacle) count as walls: the speed into the wall is taken off
and that wall is remembered for 0.15 s, so scraping along one is smooth. Script API: `p.kart` (set a
kart/part of one/nil; taken is an error; death clears it), `kart.driver`,
`boost`, `spin_out`, `place_kart` (a kart physics hasn't picked up yet gets
its chassis moved instead), `add_bot`, `on key(p, k)` for E Q F R G Z X C V
B (`ToServer::Key`). What a kart touches, its driver touches. Bots
(`bot = true` Players, `Game.bots: BotDriver`) follow the Workspace's
`racing_line` Folder (parts named 1, 2, 3...), `lane` studs to the left:
they brake for bends by each bend's radius, dodge karts ahead, and back up
when stuck. `Game::set_autopilot` (Player: `BRIXO_AUTOPILOT=1`) drives a
human's kart the same way, for filming. Client: chase camera (FOV and
distance grow with speed), speed dial and drift bar, sparks and flames
(`kart_fx`), engine note; the renderer leaves out characters within 5
studs of the camera. Prediction covers karts: your kart is drawn blended
between the last two physics steps (`Physics::step_fraction`/`steps_run`;
drawing the newest step made fast karts hop on screens faster than 60 Hz),
prediction nudges are capped at 0.25 studs a frame (a big one could push a
kart through a wall), and script boosts and spin-outs reach the prediction through the kart's
`boost_left`/`spin_left` facts (without them, a boost pad made the server
run ahead and the prediction snap forward). The view smoother blends part
rotation too. `SoundProps.data` is an `Arc<str>`: players copy the world
every frame, and copying every song in it took most of the time. Scripts can put a player's camera on a part (`camera_part`,
kept as the part's id in a custom field; `FollowCamera` glides after it
and cuts on jumps over 25 studs). `tests/karts.rs`.

**Brickport Speedway** (`brixo_samples::speedway`, `brixo-samples
speedway`): kart racing for 8, bots filling the grid. The track is a
Catmull-Rom spline through `CONTROL` (31 points, 2.4k studs, sampled every
4), built as road slabs per nearly-straight stretch (flat: banking the
road left lips where pieces met, and karts caught on them), striped walls, curbs,
a bridge crossover with pillars, a canyon jump (a full-width boost strip,
a kicker wedge, a landing wedge; `LIP_Z`/`LAND_Z`) over the river into a
lake, and a barn shortcut (`CUT_FROM`/`CUT_TO`; the script caps
`top_speed` at 40 inside it; no checkpoints in the stretch it skips).
Grandstand with ~120 fans, jumbotron (a label attached to its screen),
start gantry with 5 lights, pits (join here), podium, town, trees, lamps.
The **Race** script: lobby (`lobby_seconds`, 45 by default; everyone on
foot, F gets in/out of the nearest free kart, race karts parked in the pit
boxes, bots standing in the garages; it waits while nobody's in), then
`to_grid` (humans who haven't opted out with the "Race next" button or G,
in random slots; bots to make 8), the flyover (the **Flyover Camera** part
glides through the **Flyover** folder's From/To shots with everyone's
`camera_part` on it, to Intro Music), quiet, five lights with beeps, GO
into Race Music (Final Lap Music + sting when the leader starts the last
lap), 3 laps, results (25 s after the first finisher, or when every human's
done), podium, fireworks, Victory Fanfare, saved `wins`/`best_lap`,
leaderboard; time of day rotates 18.0/21.8/13.5. The **Drift Park**
(`DRIFT`, infield) has 4 practice karts (`practice = true`), always free;
the script puts back any that leave it. Checkpoints are checked by position
every 0.1 s (not touches). Items (boost, homing rocket, spike mine, shield)
from item boxes, used with E; R = back to the last checkpoint; F twice
leaves a race; bots stuck 4 s respawn; bots are rubber-banded against the
best human via `top_speed`. All its music is synthesized in `synth.rs`
(`speedway_*`). Names to avoid for
custom fields: anything one letter off a built-in field (`place` reads as
a typo of `face`; `lane` is exempted). `tests/speedway.rs` races it headless.

**Learn guide** (playbrixo.com/learn, `brixo-web/src/docs.rs`): 40 pages of
Markdown in `crates/brixo-web/docs/` (getting started, Rovik, building,
players and GUI, lighting, 14 how-tos, reference), compiled into the binary
(`pages!`/`images!` lists: a new page or picture must be added there) and
rendered with pulldown-cmark. Routes: `/learn/:slug`, `/learn/img/:file`,
`/learn/search.json` (the sidebar search), `/learn/samples/<name>.brixo`
(Coin Tycoon, Flagfall, Spire Wars, Gear Range, Brickport Speedway, built fresh from
brixo-samples; Studio opens a .brixo dropped on its window). Code-block
markers, all enforced by `tests/docs.rs`: ` ```rovik ` must parse;
` ```rovik run in=class:Name with=class:Name,... ` (underscores for spaces)
must run 3 s in a real game without an error; ` ```rovik broken ` must NOT
parse (error examples); ` ```rovik sketch ` isn't checked (fragments). The
how-tos also have behaviour tests (the kill brick kills, coins count, the
laser hits who you click...), and every link and picture must exist.
Screenshots in `docs/img/` are real (Player with `BRIXO_CAMERA_FILE`, Studio
with `BRIXO_STUDIO_CAMERA`, scenes built from the guide's own recipes); retake
them when the look of the game changes. Change the engine, then run the docs
tests: they catch the guide going stale.

---

## 8. Known gaps / limits

- Website: no email (password resets are admin-made links), no report
  button (admins find problems themselves on `/admin`), no age rules yet.
- JSON over TCP every tick (fine for a few players; a compact protocol
  later). Only your own character is predicted.
- No client scripts or RemoteEvents (server-driven GUI covers current needs).
- A held tool doesn't follow the arm's swing. Animations are poses, not blended.
- Studio: no terrain, meshes or unions. Hinges are the only joint (no
  sliders, springs or ropes); a hinged part picks its partner once, when
  the joint is built.
- GUI positions are absolute (not relative to a parent Frame); no image GUI.
- Scripts can't create scripts; behaviour is cloned from templates.
- The chat filter is a word list: it catches the obvious and the usual
  dodges, not everything (and deliberately not mild trash talk).
- `tests/flagbots.rs` is timing-dependent (script threads), so it asserts
  captures OR returns rather than a capture every run.
- Every waiting script is an OS thread (a bytecode VM would fix scale limits).
- Rovik sandboxing isn't hardened for untrusted creators yet.
- In Coin Tycoon, a player leaving doesn't free their plot.

---

## 9. Roadmap (agreed direction)

1. **Growing the platform** (playbrixo.com is live): server hardening
   (per-game limits on scripts, memory and parts), a report button, age
   rules and email before a wide launch; a compact protocol.
2. **Second sample game:** Brick Obby (moving platforms, checkpoints,
   leaderboard) to stress different systems.
3. **Creator tools:** more joints (sliders, springs), terrain, client
   scripts when a game needs them.
4. **Polish:** tool follows the swing, animation blending, more faces and
   accessories (face, back slots).

---

## 10. Design decisions (and why)

- **Rust + wgpu + Rapier:** real desktop software, one language end to end.
- **The engine is the creative constraint.** Brixo's identity is its look:
  early-2000s retro (hard sun, flat ambient, pixel-art decals, bold palettes).
  Keep new visuals in that style.
- **Beginner-first language** with Lua-style blocks, because the audience is
  young creators; errors are part of the product.
- **Models weld their parts** instead of manual WeldConstraints: simpler for
  beginners; real joints come later for doors and wheels.
- **Server-authoritative everything**, and script source never leaves the server.
- **Sounds are synthesized**, so nothing needs licensing and it matches the look.
