"""Films a Flagfall match on this computer, for the Flagfall trailer.

From the repo root, after building:

    cargo build --release --workspace
    cargo build --release -p brixo-server --example bots
    python tools/film_flagfall.py        (then)        python tools/edit_flagfall.py

Hands off the keyboard and mouse while it runs (about 10 minutes). It films:

  1. Establishing shots (about 45 s): the camera flies over the map while
     eight bots pour out of their castles.
  2. A whole match, bots only, first to 3 captures (up to about 10 minutes),
     with a camera that follows the action: the flag carrier if someone has a
     flag, someone about to grab one, otherwise the biggest fight. It holds on
     a capture for the fireworks.

While the match runs it writes down every pickup, drop, return and capture
(and whether the camera was on it), so edit_flagfall.py can find the best
moments by itself. Everything lands in tools/footage/ (ff_*.mp4 and
ff_events.json).

Bot matches aren't scripted: if the match comes out dull (few captures on
camera), just run it again.
"""

import json
import math
import subprocess
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from film import (OUT, TITLE, Director, Recording, bots_exe, exe, is_frozen, quiet,  # noqa: E402
                  start_server, stop_all, wait_for_window)

STAND_A = 120.0          # each flag stand is this far from the middle (Red west, Blue east)
MATCH_LIMIT = float(__import__("os").environ.get("FLAGFALL_FILM_SECONDS", "630"))  # 8 minutes + sudden death + margin
AFTER_WIN = 7.0          # keep filming this long after the winning capture


def side(team):
    return -1.0 if team == "Red" else 1.0


def other(team):
    return "Blue" if team == "Red" else "Red"


def stand(team):
    return (side(team) * STAND_A, 0.0, 0.0)


def flat(a, b):
    return math.hypot(a[0] - b[0], a[2] - b[2])


class Watcher(Director):
    """A Director that can also see every player and flag (BRIXO_WATCH_FILE)."""

    def __init__(self, name):
        super().__init__(name)
        self.watch_file = OUT / f"{name}.watch"
        self.watch_file.unlink(missing_ok=True)

    def env(self, cinematic=True):
        e = super().env(cinematic)
        e["BRIXO_WATCH_FILE"] = str(self.watch_file)
        return e

    def watch(self):
        """(players, flags): players are dicts; flags maps team -> cloth position."""
        try:
            lines = self.watch_file.read_text().splitlines()
        except OSError:
            return [], {}
        players, flags = [], {}
        for line in lines:
            f = line.split()
            try:
                if f[0] == "player" and len(f) >= 9:
                    players.append(dict(name=f[1], p=(float(f[2]), float(f[3]), float(f[4])), yaw=float(f[5]),
                                        dead=f[6] == "1", team=f[7], carrying=None if f[8] == "-" else f[8]))
                elif f[0] == "part" and len(f) >= 5 and f[1].endswith("_Flag"):
                    flags[f[1].split("_")[0]] = (float(f[2]), float(f[3]), float(f[4]))
            except (ValueError, IndexError):
                continue
        return players, flags


def load_walls(game):
    """Every solid, visible part of the map as a box (min, max), for
    checking the camera can actually see who it's following."""
    data = json.loads(Path(game).read_text())
    boxes = []
    for inst in data["instances"]:
        props = inst.get("props")
        part = props.get("Part") if isinstance(props, dict) else None
        if not part or not part.get("can_collide", True) or part.get("transparency", 0) > 0.5:
            continue
        p, sz = part["position"], part["size"]
        if p["y"] < -100:
            continue  # hidden templates under the map
        # (Rotation is ignored: a box around the part is close enough here.)
        r = max(sz["x"], sz["z"]) / 2 if abs(part["rotation"]["y"]) > 1 else None
        hx, hz = (r, r) if r else (sz["x"] / 2, sz["z"] / 2)
        boxes.append(((p["x"] - hx, p["y"] - sz["y"] / 2, p["z"] - hz), (p["x"] + hx, p["y"] + sz["y"] / 2, p["z"] + hz)))
    return boxes


def blocked(boxes, a, b):
    """Whether the straight line from a to b passes through any box."""
    d = [b[i] - a[i] for i in range(3)]
    for lo, hi in boxes:
        t0, t1 = 0.03, 0.93
        for i in range(3):
            if abs(d[i]) < 1e-6:
                if a[i] < lo[i] or a[i] > hi[i]:
                    break
                continue
            u, v = (lo[i] - a[i]) / d[i], (hi[i] - a[i]) / d[i]
            if u > v:
                u, v = v, u
            t0, t1 = max(t0, u), min(t1, v)
            if t0 > t1:
                break
        else:
            return True
    return False


class Camera:
    """A smoothed camera that chases someone and keeps them in sight: if a
    wall is in the way, it tries higher and to either side before settling
    for looking straight down."""

    def __init__(self, d, boxes):
        self.d = d
        self.boxes = boxes
        self.pos = None
        self.fwd = (1.0, 0.0)

    def cut(self):
        self.pos = None

    def near(self, x, z, r=34):
        return [b for b in self.boxes if b[0][0] - r < x < b[1][0] + r and b[0][2] - r < z < b[1][2] + r]

    def place(self, want, look, smooth=0.08):
        if self.pos is None or math.dist(self.pos, want) > 45:
            self.pos = want  # a cut, not a swoop across the map
        else:
            self.pos = tuple(a + (b - a) * smooth for a, b in zip(self.pos, want))
        self.d.cam(self.pos, look)

    def follow(self, target, yaw_deg):
        x, y, z = target
        yaw = math.radians(yaw_deg)
        f = (math.sin(yaw), math.cos(yaw))
        # Turn slowly with them, so the picture doesn't whip round.
        k = 0.04
        self.fwd = (self.fwd[0] + (f[0] - self.fwd[0]) * k, self.fwd[1] + (f[1] - self.fwd[1]) * k)
        n = math.hypot(*self.fwd) or 1.0
        fx, fz = self.fwd[0] / n, self.fwd[1] / n
        if y < -5:
            # In the tunnel: low and close, under its roof.
            self.place((x - fx * 6.5, y + 1.4, z - fz * 6.5), (x, y + 0.8, z))
            return
        look = (x, y + 1.5, z)
        boxes = self.near(x, z)
        # Behind and a little to the side; then higher; then off to each
        # side; then from above.
        tries = []
        for back, up, turn in [(11, 6, 0), (10, 11, 0), (9, 11, 55), (9, 11, -55), (8, 16, 0), (8, 16, 110), (8, 16, -110)]:
            c, s_ = math.cos(math.radians(turn)), math.sin(math.radians(turn))
            bx, bz = fx * c - fz * s_, fz * c + fx * s_
            tries.append((x - bx * back + bz * 3, y + up, z - bz * back - bx * 3))
        want = next((t for t in tries if not blocked(boxes, t, look)), (x - fx * 3, y + 24, z - fz * 3))
        self.place(want, look)

    def stand_shot(self, team):
        """A wide view of a flag stand from over the courtyard: for the
        fireworks after a capture."""
        s = side(team)
        self.place((s * 104.0, 17.0, 16.0), (s * STAND_A, 5.0, 0.0), smooth=0.15)


def establishing():
    """Fly-overs while the bots leave their castles: the map, the flag, the
    bridge, the ruins and the tunnel."""
    print("1/2  Establishing shots (about 50 s)")
    game = OUT / "flagfall.brixo"
    server = start_server(game, 4590)
    d = Watcher("ff_establish")
    player = subprocess.Popen([exe("brixo-player"), "--join", "127.0.0.1:4590", "--name", "Cam"],
                              env=d.env(), stdout=quiet("ff_player_establish"), stderr=subprocess.STDOUT)
    wait_for_window(TITLE)
    d.wait_ready()
    rec = Recording("ff_establish")
    d.start(rec.t0)
    bots = subprocess.Popen([bots_exe(), "127.0.0.1:4590", "7", "120"], stdout=quiet("ff_bots_establish"),
                            stderr=subprocess.STDOUT)
    # The whole map, swinging round from behind the Red castle.
    d.mark("aerial")
    d.orbit(0.0, 9.0, (0, 0), 175, 95, 195, 235, look_h=0)
    # Push in on the Blue flag on its stand.
    d.mark("flag")
    t = d.now()
    d.dolly(t, t + 8.0, (98, 16, 20), (108, 10, 11), (120, 5, -1), (120, 5.5, -1))
    # Along the bridge as the first bots meet on it.
    d.mark("bridge")
    t = d.now()
    d.dolly(t, t + 9.0, (-30, 10, -24), (-14, 8, -20), (0, 4, 0), (2, 4, 0))
    # Over the ruins and the stepping stones.
    d.mark("ruins")
    t = d.now()
    d.dolly(t, t + 8.0, (-46, 16, -80), (-30, 12, -74), (0, 0, -45), (4, 0, -45))
    # Down the tunnel stairs by the Red castle, and along underneath.
    d.mark("tunnel")
    t = d.now()
    d.dolly(t, t + 8.0, (-104, 6, 44), (-62, -9.5, 44), (-80, -8, 44), (-20, -10.5, 44))
    # The Red castle from above, bots at the gate.
    d.mark("castle")
    t = d.now()
    d.dolly(t, t + 8.0, (-70, 30, -30), (-84, 24, -22), (-118, 3, 0), (-118, 2, 0))
    d.mark("end")
    rec.stop()
    stop_all(player, bots, server)
    return d.events


def match():
    """A whole bots-only match, filmed by a camera that follows the action."""
    print("2/2  The match (up to about 10 minutes; it ends early when a team wins)")
    game = OUT / "flagfall.brixo"
    server = start_server(game, 4591)
    # Eight bots join first, four a side, and the camera player last: it
    # stands idle in a spawn house, so it mustn't take a bot's place (the
    # first time, Red played three against four and never scored).
    bots = subprocess.Popen([bots_exe(), "127.0.0.1:4591", "8", str(int(MATCH_LIMIT + 90))],
                            stdout=quiet("ff_bots_match"), stderr=subprocess.STDOUT)
    time.sleep(2.5)
    d = Watcher("ff_match")
    player = subprocess.Popen([exe("brixo-player"), "--join", "127.0.0.1:4591", "--name", "Cam"],
                              env=d.env(), stdout=quiet("ff_player_match"), stderr=subprocess.STDOUT)
    wait_for_window(TITLE)
    d.wait_ready()
    rec = Recording("ff_match")
    d.start(rec.t0)
    cam = Camera(d, load_walls(game))
    d.cam((-175, 95, -60), (0, 0, 0))

    events = []           # {t, kind, who, team, flag, on_camera}
    subjects = []         # [t, name, heat, carrying] whenever the subject changes, and every half second
    carrying = {}         # player -> flag they had last time we looked
    flag_state = {"Red": "home", "Blue": "home"}
    dropped_at = {}
    caps = {"Red": 0, "Blue": 0}
    subject, since, hold_until, hold_on = None, -99.0, -1.0, None
    fireworks_team, cam_cut = None, False
    won_at = None
    last_log = -1.0
    last_status = 0.0

    def log(kind, who, team, flag, on):
        events.append(dict(t=round(d.now(), 3), kind=kind, who=who, team=team, flag=flag, on_camera=on))
        print(f"     {d.now():6.1f}s  {kind:8s} {who} ({team}) {'' if on else '(off camera)'}")

    while True:
        now = d.now()
        if now > MATCH_LIMIT or (won_at is not None and now > won_at + AFTER_WIN):
            break
        players, flags = d.watch()
        if not players:
            # Caught the watch file mid-update (Windows can refuse the read
            # for an instant): skip this look rather than lose track of who
            # we're holding on.
            time.sleep(1 / 120)
            continue
        byname = {p["name"]: p for p in players}
        bots_alive = [p for p in players if p["name"] != "Cam" and not p["dead"]]

        # --- what happened since last time ---
        for p in players:
            before, after = carrying.get(p["name"]), p["carrying"]
            if before is None and after is not None:
                log("pickup", p["name"], p["team"], after, subject == p["name"])
                flag_state[after] = "carried"
                hold_on, hold_until = p["name"], now + 1.0
            elif before is not None and after is None:
                if p["dead"]:
                    log("drop", p["name"], p["team"], before, subject == p["name"])
                    flag_state[before] = "dropped"
                    dropped_at[before] = now
                    hold_on, hold_until = p["name"], now + 3.0     # watch them fall
                else:
                    log("capture", p["name"], p["team"], before, subject == p["name"])
                    flag_state[before] = "home"
                    caps[p["team"]] += 1
                    hold_on, hold_until = p["name"], now + 4.5     # the fireworks
                    fireworks_team, cam_cut = p["team"], True
                    if caps[p["team"]] >= 3 and won_at is None:
                        won_at = now
                        events[-1]["winning"] = True
            carrying[p["name"]] = after
        for team, at in flags.items():
            if flag_state.get(team) == "dropped" and flat(at, stand(team)) < 2.5:
                auto = now - dropped_at.get(team, now) > 9.5
                flag_state[team] = "home"
                # Who sent it home? The nearest defender, if it wasn't the timer.
                near = min((p for p in bots_alive if p["team"] == team), default=None,
                           key=lambda p: flat(p["p"], at))
                log("return", "-" if auto or near is None else near["name"], team, team, False)

        # --- who to watch ---
        choice = None
        if hold_on and now < hold_until and hold_on in byname:
            choice = byname[hold_on]
        else:
            if now >= hold_until:
                hold_on = None
            carriers = [p for p in bots_alive if p["carrying"]]
            # Someone closing in on the enemy flag, about to take it.
            raiders = [p for p in bots_alive if not p["carrying"] and p["team"] in ("Red", "Blue")
                       and flag_state.get(other(p["team"])) == "home"
                       and flat(p["p"], stand(other(p["team"]))) < 30]
            current = byname.get(subject)
            if carriers:
                choice = current if current in carriers else min(carriers, key=lambda p: abs(p["p"][0]))
            elif raiders:
                choice = current if current in raiders else min(raiders, key=lambda p: flat(p["p"], stand(other(p["team"]))))
            elif bots_alive:
                def heat(p):
                    return sum(1 for q in bots_alive if q["team"] != p["team"] and math.dist(p["p"], q["p"]) < 16)
                if current and not current["dead"] and now - since < 6:
                    choice = current
                else:
                    choice = max(bots_alive, key=lambda p: (heat(p), -abs(p["p"][0])))
        if choice:
            if choice["name"] != subject or cam_cut:
                subject, since = choice["name"], now
                cam.cut()
                cam_cut = False
            if hold_on and fireworks_team and now < hold_until:
                cam.stand_shot(fireworks_team)
            else:
                fireworks_team = None
                cam.follow(choice["p"], choice["yaw"])
            if now - last_log > 0.5 or since == now:
                busy = sum(1 for q in bots_alive if q["team"] != choice["team"] and math.dist(choice["p"], q["p"]) < 16)
                subjects.append([round(now, 3), subject, busy, choice["carrying"] or "-",
                                 round(choice["p"][0], 1), round(choice["p"][1], 1), round(choice["p"][2], 1)])
                last_log = now
        if now - last_status > 60:
            last_status = now
            print(f"     {now / 60:.0f} min: Red {caps['Red']}  Blue {caps['Blue']}")
        time.sleep(1 / 60)

    d.mark("end")
    rec.stop()
    stop_all(player, bots, server)
    return dict(events=events, subjects=subjects, caps=caps, marks=d.events)


def test_capture():
    """Five seconds of a moving camera, checked before the real thing."""
    print("0/2  Testing the screen capture (5 seconds)")
    d = Director("ff_test")
    player = subprocess.Popen([exe("brixo-player"), str(OUT / "flagfall.brixo")], env=d.env(),
                              stdout=quiet("ff_player_test"), stderr=subprocess.STDOUT)
    wait_for_window(TITLE)
    d.wait_ready()
    rec = Recording("ff_capture_test")
    d.start(rec.t0)
    d.orbit(0, 5, (0, 0), 150, 70, 0, 60, look_h=0)
    rec.stop()
    stop_all(player)
    if not rec.path.exists() or rec.path.stat().st_size < 1000 or is_frozen(rec.path):
        sys.exit("The screen capture didn't work (no video, or a frozen picture). Keep the Brixo window\n"
                 "visible on your main monitor, and make sure ffmpeg is 6.1 or newer.")
    print("     capture works")


def main():
    import shutil
    if not shutil.which("ffmpeg"):
        sys.exit("ffmpeg isn't installed (Windows: winget install Gyan.FFmpeg)")
    for name in ["brixo-server", "brixo-player", "brixo-samples"]:
        if not Path(exe(name)).exists():
            sys.exit(f"{name} isn't built: cargo build --release --workspace")
    if not Path(bots_exe()).exists():
        sys.exit("The bots aren't built: cargo build --release -p brixo-server --example bots")
    OUT.mkdir(parents=True, exist_ok=True)
    subprocess.run([exe("brixo-samples"), "flag", str(OUT / "flagfall.brixo")], check=True, stdout=subprocess.DEVNULL)

    print("Filming Flagfall. Hands off the keyboard and mouse until it's done.\n")
    test_capture()
    info = {"establish": establishing()}
    info["match"] = match()
    (OUT / "ff_events.json").write_text(json.dumps(info, indent=1))
    ev = info["match"]["events"]
    on = [e for e in ev if e["on_camera"]]
    print(f"\nDone. Captures: Red {info['match']['caps']['Red']}, Blue {info['match']['caps']['Blue']}. "
          f"On camera: {sum(e['kind'] == 'capture' for e in on)} captures, {sum(e['kind'] == 'pickup' for e in on)} pickups, "
          f"{sum(e['kind'] == 'drop' for e in on)} drops.")
    if sum(e["kind"] == "capture" for e in on) < 2:
        print("Fewer than 2 captures on camera: the trailer will still cut, but running this again may give a better match.")
    print("Now run:  python tools/edit_flagfall.py")


if __name__ == "__main__":
    main()
