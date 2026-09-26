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
Release builds hide the console (`windows_subsystem`). Player and Studio check
`/api/version` in the background and show "A new ... is out" (never for "dev"
builds). Installed Studio saves `scene.brixo` in `~/Brixo`, finds Player next
to itself or installed, and publishes to `install::site()` (BRIXO_SITE
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
  play them through `brixo_client::Audio`.

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
  abs min max sqrt random(lo, hi)`.
- Host functions: `find destroy clone time players create play_sound play_music
  stop_music explode`. `explode(position, radius[, power])` knocks out players
  in range (and returns them), throws loose parts, and shows a fireball.
- Events: `on touched(other)`, `on clicked(player)`, `on activated(player)`,
  `on player_joined(p)`, `on player_left(p)`, `on died(p)`, `every N seconds`.
- Players also have `look` (facing direction, `{x, y = 0, z}`) and `swinging`.
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
  `tools/trailer-music.mp3`. Needs ffmpeg (and Playwright for the website
  clip). Everything lands in `tools/footage/` (git-ignored). Explosion
  moments come from the player's sound logs, refined by finding the fireball
  in the footage; walks wait for real arrival (BRIXO_POSITION_FILE), so it
  works however fast the computer is.
- **The Flagfall trailer** (`tools/film_flagfall.py`, then
  `tools/edit_flagfall.py`): films establishing shots and a whole bots-only
  match with a camera that follows the action, logging every pickup, drop,
  return and capture (and whether it was on camera) to
  `footage/ff_events.json`; the editor picks moments from that log and cuts
  them to `tools/flagfall-music.wav` (Fuzzeke, "Wild Fight"; the beat map is
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
  (`brixo-samples gear`) is the test map.
- **Test hooks** (inert unless set):
  - `BRIXO_SOUND_LOG`: log sounds with timestamps (first line: `# epoch <unix time>`).
  - `BRIXO_GOTO_FILE`: the player walks toward "x z" in that file.
  - `BRIXO_ACTION_FILE`: lines another program appends ("equip 6", "use", "jump").
  - `BRIXO_POSITION_FILE`: the player writes "x y z" there every frame.
  - `BRIXO_CAMERA_FILE`: "x y z lx ly lz" places the camera (position, look-at).
  - `BRIXO_CINEMATIC`: no interface at all.
  - `BRIXO_WATCH_FILE` (player): every player (name, position, facing,
    knocked out, team, flag carried) and every beacon part, each frame. The
    Flagfall director follows the action with it.
  - `BRIXO_TIME_SCALE` (server and the bots example): run the game slower
    than real time (e.g. 0.25), so a slow machine renders every moment;
    speed the footage back up afterwards.
  - `BRIXO_WINDOW_SIZE` ("1920x1080", borderless at the top-left) and
    `BRIXO_WINDOW_TITLE`: for screen recorders (player and studio).
  - `BRIXO_STUDIO_DEMO` (`build` / `code`) with `BRIXO_DEMO_EVENTS`: the studio
    performs the trailer's studio shots itself and writes when and where.
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

**Studio editing:** drag parts over surfaces in the 3D view (a left-drag
starting on a selected part, away from the gizmo: it rests on whatever's under
the mouse, snapped to whole studs); the Explorer has drag-and-drop to move
things between folders and models, rename in place (double-click or F2) and a
right-click menu; Ctrl+C / Ctrl+X / Ctrl+V go through the system clipboard
(`DataModel::to_clipboard` / `paste_clipboard`), so things copy between
games; the script editor has syntax colouring, line numbers, the error line
shaded, and autocomplete (arrows to choose, Tab to accept, Escape to hide).
The testable parts live in `brixo-studio/src/editing.rs`.

**Studio:** Explorer (Ctrl-click multi-select), Properties (parts, GUI, materials,
velocity), Move/Rotate/Scale gizmos with snapping, group move/rotate, undo/redo,
copy/paste/duplicate, group/ungroup (Ctrl+G/U), F to focus, Add Part/GUI/Tool,
script editor with live syntax check, Output panel, Play/Stop (F5), Players count
(local server + a window per player), Publish, Open from library.

**Avatar:** blocky body, round head, 4 pixel-art face decals, random curated
palettes, limb animations (walk, jump, hold, swing), first/third person, camera
collision.

**Gameplay systems:** GUI (TextLabel/TextButton/Frame, per-player or shared,
labels attached to parts), tools with hotbar (1-9) and `on activated`, chat
(filtered, rate-limited, bubbles), custom fields, sounds and music.

**Platform:** website (sign up, log in, avatar editor, catalog with live player
counts, Play), `brixo://` links registered with the OS, ticketed game servers
started on demand, Studio publishing (log in once, then Publish).

**Apps:** studio, player (opened by Play: HUD, pause, F9 console, chat, mouse
lock; no menus), dedicated LAN server.

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

**Sky:** a per-pixel shader sky (gradient, sun disc, flat two-tone clouds) and
distance haze toward the horizon colour.

**Sample games:** Spire Wars (a Doomspire-style remake: four team towers of
breakable bricks that collapse, six weapons (sword, rocket launcher,
superball, slingshot, trowel, timebomb), auto-balanced teams, timed rounds with
team scores and a map rebuild, a custom horn Sound) and Coin Tycoon (4 plots, droppers, conveyor, furnace, upgrader,
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

---

## 8. Known gaps / limits

- The website runs on `localhost`: not deployed, no HTTPS, no email or password
  reset, no rate limiting, no moderation tools. Game servers only listen
  locally, so friends on other computers can't join website games yet.
- `--register-protocol` is verified on Linux; the Windows registry version is
  written but untested.
- No saved player data (deliberately deferred).
- No client prediction; JSON over TCP every tick (fine for a few players).
- No client scripts or RemoteEvents (server-driven GUI covers current needs).
- Chat input and mouse lock exist in the player app only, not studio Play.
- A held tool doesn't follow the arm's swing. Animations are poses, not blended.
- Studio: no Explorer drag-and-drop reparenting, rename-in-place, or box select;
  no terrain, meshes, unions, lighting settings, or hinges/motors.
- GUI positions are absolute (not relative to a parent Frame); no image GUI.
- Scripts can't create scripts; behaviour is cloned from templates.
- The chat word filter is a tiny starter list.
- Every waiting script is an OS thread (a bytecode VM would fix scale limits).
- Rovik sandboxing isn't hardened for untrusted creators yet.
- In Coin Tycoon, a player leaving doesn't free their plot.

---

## 9. Roadmap (agreed direction)

1. **Taking the platform online** (plan before coding): hosting the website and
   game servers, HTTPS, internet play with client prediction and a compact
   protocol, an installer that registers `brixo://`, and a trust-and-safety
   design for young players (age rules, moderation, reporting) before anyone
   outside friends signs up.
2. **Second sample game:** Brick Obby (moving platforms, checkpoints,
   leaderboard) to stress different systems.
3. **Creator tools:** Explorer drag-drop/rename/box-select, hinges and motors,
   lighting settings, a Rovik reference and tutorials, client scripts when a game
   needs them.
4. **Polish:** tool follows the swing, animation blending, more faces and
   accessories (hat, face, back slots), chat in studio Play.

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
