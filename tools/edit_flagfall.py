"""Cuts the Flagfall trailer from tools/footage/ to tools/flagfall-music.wav.

    python tools/edit_flagfall.py

Run film_flagfall.py first. Writes tools/footage/flagfall-trailer.mp4
(1920x1080), plus tools/footage/flagfall-trailer-preview.mp4 (small, to
share) and flagfall-trailer-plan.txt (which moment went where).

The music is Fuzzeke's "Wild Fight" (77.6 s). Every cut sits on its beat map:

     0.95  4.69  8.38  12.08   intro hits       -> four map shots, a line each
    15.67                      drop 1           -> the fighting starts, a cut every 8 beats
    38.10                      drop-out         -> a carrier falls, in slow motion
    40.12 - 46.92              breakdown        -> a flag goes home, a chase
    46.92 - 52.70              build            -> faster and faster cuts of a carrier running home
    52.74 - 54.16              near silence     -> the last frame before the capture, frozen
    54.43                      drop 2           -> that capture: fireworks on the hit
    58.24  62.04  66.05        climax           -> fights, more captures, the winning run
    69.38                      final hit        -> FLAGFALL, over the winning capture
    71.54                      last accent      -> "A BRIXO GAME"; the song fades out

The moments come from ff_events.json, written while filming: every pickup,
drop, return and capture, and who the camera was following.
"""

import json
import shutil
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from edit import BAR, CH, OUT, S, Editor, duration, fonts, sh  # noqa: E402

MUSIC = Path(__file__).resolve().parent / "flagfall-music.wav"
LENGTH = 77.6
MATCH = "ff_match.mp4"
EST = "ff_establish.mp4"

INTRO = [0.952, 4.690, 8.382, 12.080, 15.673]
DROP1, DIP, BREAK, BUILD, HUSH, DROP2 = 15.673, 38.098, 40.124, 46.922, 52.698, 54.433
CLIMAX = [54.433, 58.240, 62.038, 66.049, 69.381]
TITLE, TAG = 69.381, 71.541
EIGHT = 8 * 0.4644  # eight beats at 129 BPM


class Picker:
    """Finds moments in the match footage, never using the same stretch twice."""

    def __init__(self, match):
        self.events = match["events"]
        self.subjects = match["subjects"]
        self.used = []
        self.length = duration(MATCH)

    def free(self, a, b):
        return a >= 0 and b <= self.length - 0.2 and all(b <= u0 or a >= u1 for u0, u1 in self.used)

    def take(self, a, b):
        self.used.append((a, b))

    def events_of(self, kind, on_camera=True):
        return [e for e in self.events if e["kind"] == kind and (e["on_camera"] or not on_camera)]

    def carrying_at(self, t):
        """Whether the camera was on a flag carrier at time t."""
        last = None
        for s in self.subjects:
            if s[0] > t:
                break
            last = s
        return last is not None and last[3] != "-"

    def heat(self, a, b):
        """How much fighting the camera saw from a to b, less for fights in a
        castle courtyard (seen from far above, everyone's tiny)."""
        xs = []
        for s in self.subjects:
            if a <= s[0] <= b:
                inside = len(s) >= 7 and abs(s[4]) > 96 and abs(s[6]) < 30
                xs.append(s[2] * (0.4 if inside else 1.0))
        return sum(xs) / len(xs) if xs else 0.0

    def fight(self, length, prefer_after=0.0):
        """The busiest stretch of fighting that's still unused."""
        best, best_score = None, -1.0
        t = 5.0
        while t + length < self.length - 1:
            if self.free(t - 0.5, t + length + 0.5):
                score = self.heat(t, t + length) + (0.3 if t > prefer_after else 0)
                if score > best_score:
                    best, best_score = t, score
            t += 0.5
        if best is None:
            best = 5.0
        self.take(best, best + length)
        return best

    def carry(self, length):
        """A stretch of someone running with a flag, still unused."""
        t = 5.0
        found = None
        while t + length < self.length - 1:
            if self.free(t, t + length) and all(self.carrying_at(t + k * 0.5) for k in range(int(length * 2) + 1)):
                found = t
                break
            t += 0.5
        if found is None:
            return self.fight(length)
        self.take(found, found + length)
        return found


def scene_cuts(src, a, b):
    """Times (in the file) where the picture changes completely, a to b."""
    a = max(0.0, a)
    out = subprocess.run(["ffmpeg", "-hide_banner", "-ss", f"{a:.3f}", "-t", f"{b - a:.3f}", "-i", src,
                          "-vf", "scale=320:-2,select='gt(scene,0.3)',showinfo", "-an", "-f", "null", "-"],
                         cwd=OUT, capture_output=True, text=True).stderr
    times = []
    for line in out.splitlines():
        if "pts_time:" in line:
            try:
                times.append(a + float(line.split("pts_time:")[1].split()[0]))
            except ValueError:
                pass
    return times


def lag(src, moments, before=5.0, after=0.6):
    """How far behind the log the footage runs. Screen capture takes a
    moment to start after filming does, so something logged at t seconds
    is on screen at about t - lag. The camera cuts at known moments (the
    start of each establishing shot, the fireworks view at each capture):
    find those cuts in the footage and measure."""
    found = []
    for t in moments:
        cuts = [c for c in scene_cuts(src, t - before, t + after) if c <= t + after]
        if cuts:
            found.append(t - max(cuts))
    if not found:
        return 0.0
    found.sort()
    value = found[len(found) // 2]
    return value if -0.5 < value < before else 0.0


def main():
    if not MUSIC.exists():
        sys.exit(f"The music isn't there: {MUSIC}")
    if not (OUT / "ff_events.json").exists() or duration(MATCH) < 10:
        sys.exit("No footage yet: run python tools/film_flagfall.py first.")
    info = json.loads((OUT / "ff_events.json").read_text())
    E, M = info["establish"], info["match"]
    # Line the logs up with the footage.
    est_lag = lag(EST, [E[k] for k in ("flag", "bridge", "ruins", "tunnel", "castle") if k in E])
    match_lag = lag(MATCH, [ev["t"] for ev in M["events"] if ev["kind"] == "capture"])
    print(f"footage runs {est_lag:.2f}s (establishing) and {match_lag:.2f}s (match) behind the log")
    E = {k: v - est_lag for k, v in E.items()}
    for ev in M["events"]:
        ev["t"] -= match_lag
    for sj in M["subjects"]:
        sj[0] -= match_lag
    fonts()
    e = Editor()
    pick = Picker(M)
    plan = []

    def shot(start, end, src, at, what, **kw):
        plan.append(f"{start:6.2f}-{end:6.2f}  {src:18s} @ {at:7.2f}s  {what}")
        e.shot(start, end, src, at, **kw)

    y_low = f"{BAR + CH - round(84 * S)}"

    # --- the moments -----------------------------------------------------------
    caps = pick.events_of("capture")
    if not caps:
        caps = pick.events_of("capture", on_camera=False)
    winning = next((c for c in caps if c.get("winning")), caps[-1] if caps else None)
    # Drop 2 gets the capture with the longest run home before it (the build
    # shows that run), and never the winning one if there's another.
    others = [c for c in caps if c is not winning] or caps
    hero = None
    if others:
        pickups = pick.events_of("pickup", on_camera=False)

        def run_length(c):
            before = [p["t"] for p in pickups if p["who"] == c["who"] and p["t"] < c["t"]]
            return c["t"] - max(before) if before else 0
        hero = max(others, key=run_length)
        pick.take(hero["t"] - 12, hero["t"] + 4)
    if winning and winning is not hero:
        pick.take(winning["t"] - 4, winning["t"] + 7)
    extra_caps = [c for c in caps if c is not hero and c is not winning]
    pickups = [p for p in pick.events_of("pickup") if pick.free(p["t"] - 2.5, p["t"] + 2.0)]
    drops = [d for d in pick.events_of("drop") if pick.free(d["t"] - 1.5, d["t"] + 1.5)]
    returns = [r for r in pick.events_of("return", on_camera=False) if r["who"] != "-"]
    # Keep the first of each for their slots, before any fights are chosen.
    if pickups:
        pick.take(pickups[0]["t"] - 2.5, pickups[0]["t"] + 2.0)
    if drops:
        pick.take(drops[0]["t"] - 1.5, drops[0]["t"] + 1.5)
    print(f"captures on camera {len(caps)}, pickups {len(pickups)}, drops {len(drops)}")

    # --- 0.00 - 15.67: the intro: four map shots ---------------------------------
    e.black(0.0, INTRO[0])
    shot(INTRO[0], INTRO[1], EST, E["aerial"] + 1.0, "the whole map", punch=False, flash=True,
         text="TWO CASTLES.", size=64, ty=y_low)
    shot(INTRO[1], INTRO[2], EST, E["bridge"] + 3.0, "the bridge", text="ONE RIVER.", size=64, ty=y_low)
    mid = (INTRO[2] + INTRO[3]) / 2
    shot(INTRO[2], mid, EST, E["ruins"] + 2.0, "the ruins", text="THREE WAYS ACROSS.", size=64, ty=y_low)
    shot(mid, INTRO[3], EST, E["tunnel"] + 3.0, "the tunnel", punch=False, text="THREE WAYS ACROSS.", size=64, ty=y_low)
    shot(INTRO[3], INTRO[4], EST, E["flag"] + 3.5, "the flag", text="TAKE THEIRS.", size=64, ty=y_low)

    # --- 15.67 - 38.10: drop 1: the fighting, a cut every 8 beats --------------------
    cuts = [DROP1 + k * EIGHT for k in range(6)] + [DIP]
    cuts[-2] = min(cuts[-2], DIP - 1.5)
    slots = list(zip(cuts, cuts[1:]))
    for i, (a, b) in enumerate(slots):
        length = b - a
        if i == 2 and pickups:
            p = pickups.pop(0)
            at = p["t"] - 1.6
            shot(a, b, MATCH, at, f"pickup by {p['who']}")
        elif i in (3, 5):
            shot(a, b, MATCH, pick.carry(length), "running with a flag")
        else:
            shot(a, b, MATCH, pick.fight(length), "a fight", flash=(i == 0), shake=(i == 0))

    # --- 38.10 - 40.12: the drop-out: a carrier falls, slowly ----------------------
    if drops:
        dr = drops.pop(0)
        at = dr["t"] - 0.55
        shot(DIP, BREAK, MATCH, at, f"{dr['who']} drops the flag", speed=0.5, punch=False)
    else:
        shot(DIP, BREAK, MATCH, pick.fight(1.1), "a fight, slowly", speed=0.5, punch=False)

    # --- 40.12 - 46.92: the breakdown ---------------------------------------------
    half = (BREAK + BUILD) / 2
    ret = next((r for r in returns if pick.free(r["t"] - 2.8, r["t"] + 0.6)), None)
    if ret:
        at = ret["t"] - 2.8
        pick.take(at, at + (half - BREAK))
        shot(BREAK, half, MATCH, at, f"{ret['who']} returns the {ret['flag']} flag")
    else:
        shot(BREAK, half, MATCH, pick.carry(half - BREAK), "a chase")
    shot(half, BUILD, MATCH, pick.carry(BUILD - half), "a carrier on the run")

    # --- 46.92 - 54.43: the build, the hush, drop 2: one run home ----------------------
    if hero:
        h = hero["t"]
        # Faster and faster cuts, each one closer to home, as the song builds.
        lengths = [1.857, 1.393, 0.929, 0.696, 0.464]
        before = [11.5, 8.6, 6.3, 4.6, 3.2]   # how long before the capture each cut starts
        t = BUILD
        for length, back in zip(lengths, before):
            shot(t, t + length, MATCH, h - back, f"{hero['who']} runs it home", punch=(length < 1))
            t += length
        shot(t, HUSH, MATCH, h - 1.6, f"{hero['who']} runs it home", punch=True)
        # The hush: the moment before, frozen and dark.
        shot(HUSH, DROP2, MATCH, h - 0.5, "the moment before", speed=0.05, punch=False, dark=True,
             text="BRING IT HOME.", size=72)
        # Drop 2: the capture, fireworks on the hit.
        # (The flash is over by the time the fireworks go up.)
        shot(DROP2, CLIMAX[1], MATCH, h - 0.35, f"{hero['who']} captures", flash=True, shake=True, punch=False)
    else:
        shot(BUILD, HUSH, MATCH, pick.carry(HUSH - BUILD), "a carrier")
        e.black(HUSH, DROP2, [("BRING IT HOME.", 72, "(h-text_h)/2", 0.05)])
        shot(DROP2, CLIMAX[1], MATCH, pick.fight(CLIMAX[1] - DROP2), "a fight", flash=True, shake=True)

    # --- 58.24 - 69.38: the climax ---------------------------------------------------
    shot(CLIMAX[1], CLIMAX[2], MATCH, pick.fight(CLIMAX[2] - CLIMAX[1]), "a fight", flash=True)
    if extra_caps and pick.free(extra_caps[0]["t"] - 2.0, extra_caps[0]["t"] + 2.2):
        c = extra_caps[0]
        pick.take(c["t"] - 2.0, c["t"] + 2.2)
        shot(CLIMAX[2], CLIMAX[3], MATCH, c["t"] - 1.9, f"{c['who']} captures", flash=True)
    elif pickups:
        p = pickups.pop(0)
        shot(CLIMAX[2], CLIMAX[3], MATCH, p["t"] - 2.0, f"pickup by {p['who']}", flash=True)
    else:
        shot(CLIMAX[2], CLIMAX[3], MATCH, pick.carry(CLIMAX[3] - CLIMAX[2]), "a carrier", flash=True)
    if winning:
        w = winning["t"]
        shot(CLIMAX[3], TITLE, MATCH, w - (TITLE - CLIMAX[3]) - 0.1, f"{winning['who']}: the winning run", flash=True)
        # The title over the winning capture's fireworks.
        shot(TITLE, TAG, MATCH, w - 0.1, "FLAGFALL", speed=0.45, flash=True, dark=True, punch=False,
             text="FLAGFALL", size=190)
        shot(TAG, LENGTH, MATCH, w + 0.9, "tagline", speed=0.35, dark=True, punch=False,
             text="FLAGFALL", size=190, sub="A BRIXO GAME")
    else:
        shot(CLIMAX[3], TITLE, MATCH, pick.fight(TITLE - CLIMAX[3]), "a fight", flash=True)
        shot(TITLE, LENGTH, EST, E["aerial"] + 2.0, "FLAGFALL", speed=0.5, dark=True, punch=False,
             text="FLAGFALL", size=190, sub="A BRIXO GAME")

    # --- put it together ---------------------------------------------------------
    (OUT / "cut" / "list.txt").write_text("".join(f"file '{Path(s).name}'\n" for s in e.segs))
    sh(["ffmpeg", "-loglevel", "error", "-y", "-f", "concat", "-safe", "0", "-i", "cut/list.txt",
        "-c", "copy", "cut/video.mp4"])
    shutil.copy(MUSIC, OUT / "flagfall-music.wav")
    sh(["ffmpeg", "-loglevel", "error", "-y", "-i", "cut/video.mp4", "-i", "flagfall-music.wav",
        "-vf", f"fade=t=out:st={LENGTH - 2.4:.2f}:d=2.2", "-c:v", "libx264", "-preset", "slow", "-crf", "17",
        "-c:a", "aac", "-b:a", "256k", "-movflags", "+faststart", "-t", f"{LENGTH}", "flagfall-trailer.mp4"])
    sh(["ffmpeg", "-loglevel", "error", "-y", "-i", "flagfall-trailer.mp4", "-vf", "scale=960:-2",
        "-c:v", "libx264", "-crf", "26", "-c:a", "aac", "-b:a", "128k", "flagfall-trailer-preview.mp4"])
    (OUT / "flagfall-trailer-plan.txt").write_text("\n".join(plan) + "\n")
    print("\n".join(plan))
    print(f"\nDone: {OUT / 'flagfall-trailer.mp4'}")


if __name__ == "__main__":
    main()
