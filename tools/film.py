"""Films every shot of the Brixo reveal trailer on this computer.

From the repo root, after `cargo build --release --workspace`:

    python tools/film.py        (then)        python tools/edit.py

It starts real Brixo processes (a game server, bot players, Brixo Player,
Brixo Studio, the website) and drives them through Brixo's filming
switches (see BRIXO.md, "Test hooks"): nothing to click, no need to touch
the keyboard or mouse while it runs (about 6 minutes). Every clip is
recorded at 1920x1080 into tools/footage/, with its timeline in
tools/footage/events.json.

Needs ffmpeg on PATH (Windows: winget install Gyan.FFmpeg). The website
clip also needs Playwright (pip install playwright, then
python -m playwright install chromium); without it that clip is skipped
and edit.py uses a fallback.
"""

import json
import math
import os
import shutil
import socket
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
TOOLS = ROOT / "tools"
OUT = TOOLS / "footage"
WINDOWS = os.name == "nt"
EXE = ".exe" if WINDOWS else ""
BIN = ROOT / "target" / os.environ.get("BRIXO_PROFILE", "release")
W, H = 1920, 1080
FPS = int(os.environ.get("BRIXO_FPS", "60"))
TITLE = "Brixo Film"
DISPLAY = os.environ.get("DISPLAY", ":0")  # (Linux only)

EVENTS = {}


def exe(name):
    return str(BIN / f"{name}{EXE}")


def bots_exe():
    return str(BIN / "examples" / f"bots{EXE}")


def base_env(**extra):
    env = dict(os.environ)
    env["BRIXO_WINDOW_SIZE"] = f"{W}x{H}"
    env["BRIXO_WINDOW_TITLE"] = TITLE
    env.update({k: str(v) for k, v in extra.items()})
    return env


def replace_retry(src, dst, attempts=20):
    """os.replace, retrying briefly: on Windows, replacing a file fails if
    another process has it open for reading at that exact instant (unlike
    POSIX, where this is always safe) — and the player reads this file
    every frame, so that's expected to happen now and then."""
    for i in range(attempts):
        try:
            os.replace(src, dst)
            return
        except PermissionError:
            if i == attempts - 1:
                raise
            time.sleep(0.01)


def window_exists(title):
    """Whether a window with this exact title exists right now."""
    if WINDOWS:
        import ctypes

        return ctypes.windll.user32.FindWindowW(None, title) != 0
    return True  # (x11grab records the whole screen; no window to find)


def wait_for_window(title, timeout=40):
    """Waits for a window to appear (a first launch can be slow: shader
    compilation, GPU start-up), instead of guessing with a fixed sleep."""
    start = time.time()
    while time.time() - start < timeout:
        if window_exists(title):
            return
        time.sleep(0.25)
    raise RuntimeError(
        f"The '{title}' window never appeared within {timeout}s. "
        f"See the .log files in {OUT} for what went wrong."
    )


def wait_port(port, timeout=20):
    start = time.time()
    while time.time() - start < timeout:
        try:
            with socket.create_connection(("127.0.0.1", port), timeout=0.5):
                return True
        except OSError:
            time.sleep(0.2)
    return False


def quiet(name):
    return open(OUT / f"{name}.log", "w")


class Recording:
    """Records the Brixo window (Windows) or the screen (Linux) with ffmpeg."""

    def __init__(self, name):
        self.path = OUT / f"{name}.mp4"
        if WINDOWS:
            # Windows' Desktop Duplication capture (ddagrab): it sees what the
            # GPU draws. (gdigrab can't: GPU-drawn windows come out frozen or
            # blank.) The filming window sits at the top-left, 1920x1080.
            grab = ["-f", "lavfi", "-i",
                    f"ddagrab=output_idx=0:framerate={FPS}:video_size={W}x{H}:offset_x=0:offset_y=0"]
            filters = ["-vf", "hwdownload,format=bgra"]
        else:
            # Stamp every frame with the real time it was captured: if the
            # computer can't keep up and frames drop, the footage still lines
            # up with the timeline in events.json.
            grab = ["-use_wallclock_as_timestamps", "1", "-f", "x11grab", "-video_size", f"{W}x{H}",
                    "-framerate", str(FPS), "-i", DISPLAY]
            filters = []
        self.proc = subprocess.Popen(
            ["ffmpeg", "-y", "-loglevel", "error", *grab, *filters, "-c:v", "libx264", "-preset", "fast",
             "-crf", "16", "-pix_fmt", "yuv420p", "-fps_mode", "vfr", str(self.path)],
            stdin=subprocess.PIPE,
        )
        self.t0 = time.time()

    def stop(self):
        # Ask ffmpeg to finish, and give it time: a long clip can take a
        # while to flush, and killing it mid-write leaves a broken file.
        wanted = time.time() - self.t0
        try:
            self.proc.communicate(input=b"q", timeout=180)
        except Exception:
            self.proc.kill()
        # Did it record the whole time? (Windows can interrupt screen capture:
        # the Start menu, a notification, a UAC prompt.)
        got = subprocess.run(["ffprobe", "-v", "error", "-show_entries", "format=duration", "-of", "csv=p=0",
                              str(self.path)], capture_output=True, text=True).stdout.strip()
        try:
            got = float(got)
        except ValueError:
            got = 0.0
        if got < wanted * 0.85:
            print(f"     WARNING: {self.path.name} is {got:.0f}s long but recorded for {wanted:.0f}s: "
                  "the capture stopped early. Keep the Brixo window unobstructed "
                  "(no Start menu, pop-ups or notifications) and run film.py again.")


class Director:
    """Flies the camera, walks the character and uses its weapons through
    Brixo Player's filming switches."""

    def __init__(self, name):
        self.sound_log = OUT / f"{name}.sounds"
        self.position_file = OUT / f"{name}.position"
        self.position_file.unlink(missing_ok=True)
        self.cam_file = OUT / f"{name}.cam"
        self.goto_file = OUT / f"{name}.goto"
        self.action_file = OUT / f"{name}.actions"
        for f in (self.cam_file, self.goto_file, self.action_file):
            f.unlink(missing_ok=True)
        self.action_file.write_text("")
        self.events = {}
        self.t0 = time.time()

    def env(self, cinematic=True):
        extra = {
            "BRIXO_CAMERA_FILE": self.cam_file,
            "BRIXO_GOTO_FILE": self.goto_file,
            "BRIXO_ACTION_FILE": self.action_file,
            "BRIXO_SOUND_LOG": self.sound_log,
            "BRIXO_POSITION_FILE": self.position_file,
        }
        if cinematic:
            extra["BRIXO_CINEMATIC"] = "1"
        return base_env(**extra)

    def start(self, t0):
        self.t0 = t0

    def sounds(self, name):
        """When a sound played, on the recording's timeline (from the
        player's sound log, whose first line says when its clock started)."""
        try:
            lines = self.sound_log.read_text().splitlines()
        except OSError:
            return []
        epoch = None
        times = []
        for line in lines:
            if line.startswith("# epoch"):
                epoch = float(line.split()[2])
            elif epoch is not None and len(line.split()) == 3 and line.split()[2] == name:
                times.append(epoch + float(line.split()[0]) - self.t0)
        return times

    def mark_first(self, key, sound, after, at_least=0.1):
        """Marks the first time `sound` played at least `at_least` seconds
        after the mark `after`: the real moment of an explosion, not a guess
        from the fuse. (A timebomb takes 3 seconds; nothing else that goes
        boom takes that long, so a bot's rocket nearby isn't mistaken for it.)"""
        t = next((t for t in self.sounds(sound) if t > self.events.get(after, 0) + at_least), None)
        if t is not None:
            self.events[key] = t

    def position(self):
        try:
            x, y, z = map(float, self.position_file.read_text().split())
            return x, y, z
        except (OSError, ValueError):
            return None

    def go_wait(self, x, z, timeout=40, close=1.8):
        """Walks there and waits until actually there (however fast or slow
        this computer runs the game)."""
        self.go(x, z)
        start = time.time()
        while time.time() - start < timeout:
            p = self.position()
            if p and math.hypot(p[0] - x, p[2] - z) < close:
                break
            time.sleep(0.05)
        self.stop()

    def wait_ready(self, timeout=90):
        """Waits until the player is in the game and drawing: it writes its
        position every frame once it is. (A window can exist a while before
        its first frame is drawn.)"""
        start = time.time()
        while time.time() - start < timeout:
            if self.position():
                time.sleep(1.0)  # a moment more for the picture to settle
                return
            time.sleep(0.2)
        raise RuntimeError(f"Brixo Player never got into the game within {timeout}s. See the .log files in {OUT}.")

    def drop_bomb(self, mark):
        self.act("equip 6")
        time.sleep(0.4)
        self.act("use")
        self.mark(mark)

    def now(self):
        return time.time() - self.t0

    def mark(self, name):
        self.events[name] = self.now()

    def cam(self, pos, look):
        tmp = Path(str(self.cam_file) + ".tmp")
        tmp.write_text("%f %f %f %f %f %f" % (*pos, *look))
        replace_retry(tmp, self.cam_file)

    def nocam(self):
        self.cam_file.unlink(missing_ok=True)

    def go(self, x, z):
        self.goto_file.write_text(f"{x} {z}")

    def stop(self):
        self.goto_file.unlink(missing_ok=True)

    def act(self, *lines):
        with open(self.action_file, "a") as f:
            for line in lines:
                f.write(line + "\n")

    def wait_until(self, t):
        while self.now() < t:
            time.sleep(0.02)

    @staticmethod
    def ease(u):
        u = min(max(u, 0.0), 1.0)
        return u * u * (3 - 2 * u)

    def orbit(self, t0, t1, center, radius, height, a0, a1, look_h=10):
        while (t := self.now()) < t1:
            a = math.radians(a0 + (a1 - a0) * self.ease((t - t0) / (t1 - t0)))
            self.cam((center[0] + radius * math.cos(a), height, center[1] + radius * math.sin(a)),
                     (center[0], look_h, center[1]))
            time.sleep(1 / 60)

    def dolly(self, t0, t1, p0, p1, l0, l1):
        while (t := self.now()) < t1:
            u = self.ease((t - t0) / (t1 - t0))
            self.cam(tuple(a + (b - a) * u for a, b in zip(p0, p1)),
                     tuple(a + (b - a) * u for a, b in zip(l0, l1)))
            time.sleep(1 / 60)


def start_server(game, port):
    server = subprocess.Popen([exe("brixo-server"), str(game), "--port", str(port)],
                              stdout=quiet(f"server_{port}"), stderr=subprocess.STDOUT)
    if not wait_port(port):
        server.kill()
        sys.exit(f"The game server on port {port} didn't start. See {OUT}/server_{port}.log")
    return server


def join(director, port, cinematic=True):
    player = subprocess.Popen([exe("brixo-player"), "--join", f"127.0.0.1:{port}", "--name", "Cam"],
                              env=director.env(cinematic), stdout=quiet(f"player_{port}"), stderr=subprocess.STDOUT)
    wait_for_window(TITLE)
    director.wait_ready()
    return player


def stop_all(*procs):
    for p in procs:
        try:
            p.kill()
        except Exception:
            pass
    time.sleep(1.5)


# --- the clips ------------------------------------------------------------


def is_frozen(path):
    """Whether a recording shows the same picture the whole way through."""
    out = subprocess.run(["ffmpeg", "-hide_banner", "-i", str(path), "-vf", "freezedetect=n=0.003:d=3",
                          "-map", "0:v:0", "-f", "null", "-"], capture_output=True, text=True).stderr
    return "freeze_start" in out and "freeze_end" not in out


def test_capture(spire):
    """A 5-second test before the real thing: fly the camera and check the
    recording actually moves, so a capture problem shows up now, not after
    seven minutes of filming."""
    print("0/7  Testing the screen capture (5 seconds)")
    d = Director("test")
    player = subprocess.Popen([exe("brixo-player"), str(spire)], env=d.env(), stdout=quiet("player_test"),
                              stderr=subprocess.STDOUT)
    wait_for_window(TITLE)
    d.wait_ready()
    rec = Recording("capture_test")
    d.start(rec.t0)
    d.orbit(0, 5, (0, 0), 120, 50, 0, 90, look_h=5)
    rec.stop()
    stop_all(player)
    if not rec.path.exists() or rec.path.stat().st_size < 1000:
        sys.exit("The screen capture made no video. See the ffmpeg message above.")
    if is_frozen(rec.path):
        sys.exit(
            "The screen capture recorded a frozen picture (the camera was moving).\n"
            "Make sure the Brixo window was visible on your main monitor, and that\n"
            "ffmpeg is recent (ffmpeg -version: 6.1 or newer has ddagrab)."
        )
    print("     capture works")


def film_cinematic(spire):
    """Crane shot, the Green tower demolished from a low angle, 8 bots
    converging on the plaza, a melee, the Red tower bombed from above, and
    a push-in for the title."""
    print("1/7  Spire Wars: cinematic (about 1:40)")
    server = start_server(spire, 4580)
    d = Director("cine")
    player = join(d, 4580)
    rec = Recording("cine")
    d.start(rec.t0)

    d.mark("establish")
    d.orbit(0, 8.5, (0, 0), 165, 62, 200, 245, look_h=8)
    # Across to the Green tower (third person), right up to its east wall.
    d.mark("run")
    d.nocam()
    d.go_wait(-60, 72)
    d.go_wait(-67.5, 75)
    d.drop_bomb("bomb1")
    tb = d.now()
    time.sleep(0.3)
    d.go(-44, 64)
    # A low angle, looking up, as it comes down.
    d.dolly(tb + 0.2, tb + 8.5, (-46, 2.8, 58), (-44, 3.2, 54), (-75, 17, 75), (-75, 12, 75))
    bots = subprocess.Popen([bots_exe(), "127.0.0.1:4580", "8", "80"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    d.mark("bots")
    d.go(0, 0)
    t = d.now()
    d.orbit(t, t + 16, (0, 0), 34, 16, 20, 110, look_h=2)
    d.mark("melee")
    d.nocam()
    d.act("equip 1")
    t = d.now()
    while d.now() < t + 16:
        d.act("use")
        time.sleep(0.7)
    # Across to the Red tower; bomb it; watch from above.
    d.mark("run2")
    d.go_wait(-60, -72)
    d.go_wait(-67.5, -75)
    d.drop_bomb("bomb2")
    tb = d.now()
    time.sleep(0.3)
    d.go(-45, -58)
    d.dolly(tb + 0.2, tb + 9.5, (-40, 30, -38), (-44, 26, -44), (-75, 12, -75), (-75, 8, -75))
    d.go(0, 0)
    d.mark("push")
    t = d.now()
    d.dolly(t, t + 12, (0, 7, -46), (0, 5, -24), (0, 4, 0), (0, 3, 0))
    d.mark("end")
    rec.stop()
    stop_all(player, bots, server)
    d.mark_first("boom1", "boom", "bomb1", at_least=2.5)
    d.mark_first("boom2", "boom", "bomb2", at_least=2.5)
    EVENTS["cine"] = d.events


def film_battle(name, spire, port, goto, seconds):
    """Six bots fight while Cam heads into the thick of it (third person)."""
    print(f"     {name}: a battle from the ground ({seconds}s)")
    server = start_server(spire, port)
    bots = subprocess.Popen([bots_exe(), f"127.0.0.1:{port}", "6", str(seconds + 30)],
                            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    time.sleep(1)
    d = Director(name)
    player = join(d, port)
    rec = Recording(name)
    d.start(rec.t0)
    time.sleep(3)
    d.go(*goto)
    d.wait_until(seconds)
    rec.stop()
    stop_all(player, bots, server)


def film_spire_solo(spire):
    """Single player: across to the Blue tower, a superball, two rockets
    into it, then a timebomb at its base and the whole tower comes down."""
    print("4/7  Spire Wars: rockets and the Blue tower")
    d = Director("solo")
    player = subprocess.Popen([exe("brixo-player"), str(spire)], env=d.env(), stdout=quiet("player_solo"), stderr=subprocess.STDOUT)
    wait_for_window(TITLE)
    d.wait_ready()
    rec = Recording("solo")
    d.start(rec.t0)
    d.mark("start")
    time.sleep(1.5)
    d.mark("jump")
    d.go_wait(-62, -75)
    d.mark("walk")
    d.go_wait(38, -75, timeout=60)
    # A camera over the shoulder, facing the Blue tower, for what follows.
    d.cam((28, 7, -69), (75, 9, -75))
    d.mark("superball")
    d.act("equip 3")
    time.sleep(0.5)
    d.act("use")
    time.sleep(1.6)
    d.act("equip 2")
    time.sleep(0.5)
    d.act("use")
    d.mark("rocket")
    time.sleep(2.0)
    d.act("use")
    time.sleep(2.0)
    # Right up to its west wall, a timebomb, and back to watch it fall.
    d.nocam()
    d.go_wait(67.5, -75)
    d.drop_bomb("bomb")
    time.sleep(0.2)
    d.go(30, -75)
    d.cam((24, 6, -66), (75, 12, -75))
    time.sleep(3.6)
    d.stop()
    d.mark("collapse")
    time.sleep(5)
    d.mark("end")
    rec.stop()
    stop_all(player)
    d.mark_first("rocket_boom", "boom", "rocket")
    d.mark_first("bomb_boom", "boom", "bomb", at_least=2.5)
    EVENTS["solo"] = d.events


def film_tycoon(tycoon):
    """Coin Tycoon: claim a plot and buy the first upgrades, over a fixed
    camera looking into the plot. (The interface stays on: price tags.)"""
    print("5/7  Coin Tycoon")
    d = Director("tycoon")
    player = subprocess.Popen([exe("brixo-player"), str(tycoon)], env=d.env(cinematic=False),
                              stdout=quiet("player_tycoon"), stderr=subprocess.STDOUT)
    wait_for_window(TITLE)
    d.wait_ready()
    rec = Recording("tycoon")
    d.start(rec.t0)
    d.cam((-22, 17, 8), (-22, 2, 44))
    time.sleep(1.5)
    steps = [("walk", (-22, 26), 3.5), ("claimed", None, 1.0), ("topad1", (-34.5, 27.3), 16),
             ("buy1", (-34.5, 32), 2), ("topad2", (-29.5, 27.3), 17), ("buy2", (-29.5, 32), 2),
             ("topad3", (-24.5, 27.3), 20), ("buy3", (-24.5, 32), 2.5), ("look", (-22, 20), 6)]
    for name, target, wait in steps:
        d.mark(name)
        if target:
            d.go(*target)
        time.sleep(wait)
    d.stop()
    d.mark("end")
    rec.stop()
    stop_all(player)
    EVENTS["tycoon"] = d.events


def film_studio(kind):
    """The studio performs its shots by itself (BRIXO_STUDIO_DEMO)."""
    print(f"6/7  Studio: {kind}")
    events_file = OUT / f"studio_{kind}.json"
    events_file.unlink(missing_ok=True)
    studio = subprocess.Popen([exe("brixo-studio")],
                              env=base_env(BRIXO_STUDIO_DEMO=kind, BRIXO_DEMO_EVENTS=events_file),
                              stdout=quiet(f"studio_{kind}"), stderr=subprocess.STDOUT)
    wait_for_window(TITLE)
    rec = Recording(f"studio_{kind}")
    studio.wait(timeout=120)
    rec.stop()
    info = json.loads(events_file.read_text())
    # Line the studio's timeline up with the recording's.
    shift = info["start_epoch"] - rec.t0
    info["events"] = {k: v + shift for k, v in info["events"].items()}
    EVENTS[f"studio_{kind}"] = info


def film_website():
    """Sign up, try faces and colours in the avatar editor, open the catalog:
    a real browser session, recorded by the browser itself."""
    print("7/7  The website")
    try:
        from playwright.sync_api import sync_playwright
    except ImportError:
        print("     (skipped: pip install playwright && python -m playwright install chromium)")
        return
    db = OUT / "film-web.sqlite"
    db.unlink(missing_ok=True)
    web = subprocess.Popen([exe("brixo-web")], env=dict(os.environ, PORT="7431", BRIXO_WEB_DB=str(db)),
                           stdout=quiet("web"), stderr=subprocess.STDOUT)
    if not wait_port(7431):
        web.kill()
        print("     (skipped: the website didn't start)")
        return
    site = "http://127.0.0.1:7431"
    tmp = OUT / "web_tmp"
    shutil.rmtree(tmp, ignore_errors=True)
    with sync_playwright() as p:
        browser = p.chromium.launch()
        ctx = browser.new_context(viewport={"width": W, "height": H}, record_video_dir=str(tmp),
                                  record_video_size={"width": W, "height": H})
        page = ctx.new_page()
        page.goto(site + "/")
        page.wait_for_timeout(1500)
        page.goto(site + "/signup")
        page.wait_for_timeout(600)
        page.type("#username", "BrixoFan", delay=70)
        page.type("#password", "brixo123", delay=50)
        page.wait_for_timeout(300)
        page.click("#form button[type=submit]")
        page.wait_for_url("**/avatar")
        page.wait_for_timeout(900)
        for face in [1, 2, 3, 0, 3]:
            page.locator("#face .face").nth(face).click()
            page.wait_for_timeout(380)
        for i in [0, 1, 2, 4, 6, 1]:
            page.locator("#shirt .swatch").nth(i).click()
            page.wait_for_timeout(300)
        for i in [4, 0]:
            page.locator("#pants .swatch").nth(i).click()
            page.wait_for_timeout(300)
        for i in [1, 3, 5, 3]:
            page.locator("#skin .swatch").nth(i).click()
            page.wait_for_timeout(300)
        page.click("#save")
        page.wait_for_timeout(1200)
        page.goto(site + "/")
        page.wait_for_timeout(2200)
        page.hover("button.play")
        page.wait_for_timeout(1200)
        ctx.close()
        browser.close()
    video = next(tmp.glob("*.webm"))
    shutil.move(str(video), OUT / "website.webm")
    shutil.rmtree(tmp, ignore_errors=True)
    web.kill()


def main():
    if not shutil.which("ffmpeg"):
        sys.exit("ffmpeg isn't installed (Windows: winget install Gyan.FFmpeg)")
    for name in ["brixo-server", "brixo-player", "brixo-studio", "brixo-web", "brixo-samples"]:
        if not Path(exe(name)).exists():
            sys.exit(f"{name} isn't built: cargo build --release --workspace")
    if not Path(bots_exe()).exists():
        sys.exit("The bots aren't built: cargo build --release -p brixo-server --example bots")
    OUT.mkdir(parents=True, exist_ok=True)

    spire, tycoon = OUT / "spire-wars.brixo", OUT / "coin-tycoon.brixo"
    subprocess.run([exe("brixo-samples"), "spire", str(spire)], check=True, stdout=subprocess.DEVNULL)
    subprocess.run([exe("brixo-samples"), "tycoon", str(tycoon)], check=True, stdout=subprocess.DEVNULL)

    print("Filming Brixo. Hands off the keyboard and mouse until it's done.\n")
    test_capture(spire)
    film_cinematic(spire)
    print("2/7  Spire Wars: battles")
    film_battle("battleA", spire, 4581, (0, 0), 55)
    print("3/7")
    film_battle("battleB", spire, 4582, (48, -52), 48)
    film_spire_solo(spire)
    film_tycoon(tycoon)
    film_studio("build")
    film_studio("code")
    film_website()

    (OUT / "events.json").write_text(json.dumps(EVENTS, indent=2))
    print(f"\nDone: footage is in {OUT}\nNow run:  python tools/edit.py")


if __name__ == "__main__":
    main()
