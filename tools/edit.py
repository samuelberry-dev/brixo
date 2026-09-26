"""Cuts tools/footage/ into the Brixo reveal trailer, timed to the music.

    python tools/edit.py

Writes tools/footage/brixo-reveal-trailer.mp4 (1920x1080). Every cut, text
slam and flash is placed on the beat map of tools/trailer-music.mp3:

    0.56  opening hit          -> crane shot, "BUILT FROM SCRATCH."
    3.8 - 17.6  the build-up   -> BUILD, SCRIPT, BE ANYONE, PLAY ANYTHING,
                                  TOGETHER, SPIRE WARS (cuts on the accents)
    17.65 the drop             -> the Green tower comes down
    17.7 - 25   full energy    -> the battle
    25.1 - 26.2 the dip        -> slow motion
    26.2 - 30   full again     -> towers fall
    30.0 - 31.1 the drum fill  -> a flash cut on every hit
    31.6 - 33.2 silence        -> BUILD IT. / PLAY IT. / SHARE IT.
    33.2  the final drop       -> the BRIXO title
    36.1  the last hit         -> BUILD - PLAY - SHARE, and the fade
"""

import json
import shutil
import subprocess
import sys
from pathlib import Path

TOOLS = Path(__file__).resolve().parent
OUT = TOOLS / "footage"
MUSIC = TOOLS / "trailer-music.mp3"
W, H, FPS = 1920, 1080, 30
CH = round(W / 2.39)                  # the letterboxed picture's height
BAR = (H - CH) // 2
S = W / 1280                          # text sizes were designed at 1280 wide
GRADE = "eq=contrast=1.1:saturation=1.3:brightness=0.01,vignette=PI/5"


def fonts():
    """Copies a bold and a regular font beside the footage (ffmpeg's filter
    syntax trips over Windows drive letters, so paths stay relative)."""
    candidates = {
        "bold.ttf": [r"C:\Windows\Fonts\arialbd.ttf", "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
                     "/System/Library/Fonts/Supplemental/Arial Bold.ttf"],
        "regular.ttf": [r"C:\Windows\Fonts\arial.ttf", "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
                        "/System/Library/Fonts/Supplemental/Arial.ttf"],
    }
    for name, paths in candidates.items():
        found = next((p for p in paths if Path(p).exists()), None)
        if not found:
            sys.exit(f"Couldn't find a font for {name}")
        shutil.copy(found, OUT / name)


def sh(args):
    subprocess.run(args, cwd=OUT, check=True)


_durations = {}


def duration(src):
    """How long a clip is (0 if it's missing or broken)."""
    if src not in _durations:
        out = subprocess.run(["ffprobe", "-v", "error", "-show_entries", "format=duration", "-of", "csv=p=0", src],
                             cwd=OUT, capture_output=True, text=True).stdout.strip()
        try:
            _durations[src] = float(out)
        except ValueError:
            _durations[src] = 0.0
            print(f"  warning: {src} is missing or broken")
    return _durations[src]


class Editor:
    def __init__(self):
        self.segs = []
        self.n = 0
        (OUT / "cut").mkdir(exist_ok=True)

    def _next(self):
        self.n += 1
        return f"cut/s{self.n:03d}.mp4"

    def _text(self, text, size, y, font="bold.ttf", color="white", start=0.0, fade=0.08):
        if not text:
            return ""
        self.n += 1
        tf = f"cut/t{self.n}.txt"
        (OUT / tf).write_text(text, encoding="utf-8")
        alpha = f"if(lt(t,{start}),0,min(1,(t-{start})/{fade}))"
        return (f",drawtext=fontfile={font}:textfile={tf}:fontsize={round(size * S)}:fontcolor={color}"
                f":borderw={round(4 * S)}:bordercolor=black@0.7:x=(w-text_w)/2:y={y}:alpha='{alpha}'")

    def shot(self, start, end, src, at, speed=1.0, crop=None, text=None, size=96, ty="(h-text_h)/2", sub=None,
             flash=False, punch=True, shake=False, dark=False):
        """One shot, letterboxed and graded, from `src` at `at` seconds.
        Always exactly as long as asked, so every later cut stays on its
        beat: if the footage runs short, its last frame holds; if it's
        missing, the shot is black."""
        length = end - start
        frames = round(length * FPS)
        need = length * speed
        have = duration(src)
        if have <= 0.1:
            print(f"  warning: no footage for the shot at {start:.2f}s ({src}): black instead")
            return self.black(start, end, [(text, size, ty, 0.0)] if text else [])
        if at + need > have:
            print(f"  warning: {src} ends at {have:.1f}s, before the shot at {start:.2f}s needs it: using its last moments")
            at = max(0.0, have - need - 0.1)
        out = self._next()
        vf = f"setpts=(PTS-STARTPTS)/{speed}"
        if crop:
            vf += f",crop={crop}"
        vf += f",scale={W}:{CH}:force_original_aspect_ratio=increase,crop={W}:{CH},fps={FPS}"
        if punch:  # starts pushed in, settles in a few frames
            vf += (f",zoompan=z='max(1,1.09-0.015*on)':x='iw/2-(iw/zoom/2)':y='ih/2-(ih/zoom/2)'"
                   f":d=1:s={W}x{CH}:fps={FPS}")
        if shake:  # a hard shake that dies away
            m = round(20 * S)
            vf += (f",crop={W - 2 * m}:{CH - m}:x='{m}+{round(16 * S)}*sin(n*2.3)*exp(-t*3.5)'"
                   f":y='{m // 2}+{round(10 * S)}*cos(n*3.1)*exp(-t*3.5)',scale={W}:{CH}")
        vf += "," + GRADE
        if dark:
            vf += ",eq=brightness=-0.22:saturation=0.8,boxblur=4:1"
        vf += f",pad={W}:{H}:0:{BAR}:black"
        vf += self._text(text, size, ty)
        if sub:
            vf += self._text(sub, 46, f"{ty}+{round((size + 10) * S)}", color="0xf5cd30", start=0.15)
        if flash:
            vf += ",fade=t=in:st=0:d=0.22:color=white"
        vf += f",tpad=stop_mode=clone:stop_duration={length:.3f},format=yuv420p"
        # Limit the input, not the output: a slowed shot needs less source
        # but lasts longer.
        sh(["ffmpeg", "-loglevel", "error", "-y", "-ss", f"{max(at, 0):.3f}", "-t", f"{length * speed + 0.5:.3f}",
            "-i", src, "-vf", vf, "-frames:v", str(frames), "-an", "-c:v", "libx264", "-preset", "fast",
            "-crf", "17", out])
        self.segs.append(out)

    def black(self, start, end, lines=()):
        length = end - start
        out = self._next()
        vf = f"fps={FPS}"
        for text, size, y, st in lines:
            vf += self._text(text, size, y, start=st, fade=0.12)
        vf += ",format=yuv420p"
        sh(["ffmpeg", "-loglevel", "error", "-y", "-f", "lavfi", "-i", f"color=c=black:s={W}x{H}:r={FPS}:d={length}",
            "-vf", vf, "-frames:v", str(round(length * FPS)), "-c:v", "libx264", "-preset", "fast", "-crf", "17", out])
        self.segs.append(out)

    def finish(self, name):
        (OUT / "cut" / "list.txt").write_text("".join(f"file '{Path(s).name}'\n" for s in self.segs))
        sh(["ffmpeg", "-loglevel", "error", "-y", "-f", "concat", "-safe", "0", "-i", "cut/list.txt",
            "-c", "copy", "cut/video.mp4"])
        shutil.copy(MUSIC, OUT / "music.mp3")
        sh(["ffmpeg", "-loglevel", "error", "-y", "-i", "cut/video.mp4", "-i", "music.mp3",
            "-vf", "fade=t=out:st=39.8:d=2.0", "-c:v", "libx264", "-preset", "slow", "-crf", "17",
            "-c:a", "aac", "-b:a", "192k", "-movflags", "+faststart", "-t", "41.8", name])


def fireball(src, near, before=0.6, after=2.5):
    """When an explosion's fireball first appears on screen in `src`, looking
    around `near` seconds. Sound logs can run ahead of what's drawn (or
    behind), so this finds the actual frame: the first one with a burst of
    bright orange. Returns `near` if it can't find one."""
    start = max(near - before, 0)
    w, h = 64, 36
    raw = subprocess.run(["ffmpeg", "-loglevel", "error", "-ss", f"{start:.3f}", "-t", f"{before + after:.3f}",
                          "-i", src, "-vf", f"fps=30,scale={w}:{h}", "-f", "rawvideo", "-pix_fmt", "rgb24", "-"],
                         cwd=OUT, capture_output=True).stdout
    size = w * h * 3
    for i in range(len(raw) // size):
        frame = raw[i * size:(i + 1) * size]
        orange = sum(1 for p in range(0, size, 3)
                     if frame[p] > 215 and 85 < frame[p + 1] < 215 and frame[p + 2] < 130)
        if orange > w * h * 0.012:
            return start + i / 30
    return near


def rect_crop(r, part=(0.0, 0.0, 1.0, 1.0)):
    """A crop string for part of a screen rectangle [x, y, w, h] (pixels)."""
    x, y, w, h = r
    px, py, pw, ph = part
    return f"{round(w * pw)}:{round(h * ph)}:{round(x + w * px)}:{round(y + h * py)}"


def main():
    events = json.loads((OUT / "events.json").read_text())
    fonts()
    e = Editor()
    C = events["cine"]
    SOLO = events["solo"]
    T = events["tycoon"]
    B = events["studio_build"]["events"]
    K = events["studio_code"]["events"]
    # The code: a wide view of the bottom of the studio ending at the code
    # box (which the studio notes while typing), with the 3D view above it.
    cx, cy, cw, ch = events["studio_code"]["code"]
    code_w = min(1400, W)
    code_h = round(code_w / 2.39)
    code_edit = f"{code_w}:{code_h}:0:{max(0, round(cy + ch - code_h))}"
    code_view = rect_crop(events["studio_code"]["view"])
    build_view = rect_crop(events["studio_build"]["view"])
    HUDLESS = f"{round(W * 0.875)}:{round(H * 0.75)}:0:{round(H * 0.12)}"
    website = (OUT / "website.webm").exists()
    # The explosions: roughly when from the sound logs, then exactly when
    # from the footage itself (the first frame of each fireball).
    boom1 = fireball("cine.mp4", C.get("boom1", C["bomb1"] + 3.1))
    boom2 = fireball("cine.mp4", C.get("boom2", C["bomb2"] + 3.1))
    rocket_boom = fireball("solo.mp4", SOLO.get("rocket_boom", SOLO["rocket"] + 0.62))
    bomb_boom = fireball("solo.mp4", SOLO.get("bomb_boom", SOLO["collapse"] - 0.4))
    print("explosions at %.2f %.2f (cinematic), %.2f %.2f (solo)" % (boom1, boom2, rocket_boom, bomb_boom))
    y_low = f"{BAR + CH - round(80 * S)}"
    y_top = f"{BAR + round(34 * S)}"

    # 0.00 - 17.65: the build-up
    e.black(0.00, 0.56)
    e.shot(0.56, 3.83, "cine.mp4", C["establish"] + 0.4, speed=1.6, flash=True, punch=False,
           text="BUILT FROM SCRATCH.", size=40, ty=y_low)
    e.shot(3.83, 4.69, "studio_build.mp4", B["drag"] - 0.1, crop=build_view, text="BUILD", size=120)
    e.shot(4.69, 5.55, "studio_build.mp4", B["dup"] + 0.3, crop=build_view)
    e.shot(5.55, 7.70, "studio_code.mp4", K["type"] + 0.1, speed=2.6, crop=code_edit, punch=False,
           text="SCRIPT", size=120, ty=f"{BAR + round(30 * S)}")
    e.shot(7.70, 8.99, "studio_code.mp4", K["play"] + 2.6, crop=code_view, text="...AND WATCH IT RUN", size=56,
           ty=f"{BAR + CH - round(90 * S)}")
    if website:
        e.shot(8.99, 9.85, "website.webm", 5.7, crop=f"{round(W * 0.86)}:{round(H * 0.87)}:{round(W * 0.07)}:{round(H * 0.06)}",
               text="BE ANYONE", size=110)
        e.shot(9.85, 10.26, "website.webm", 8.3, crop=f"{round(W * 0.86)}:{round(H * 0.87)}:{round(W * 0.07)}:{round(H * 0.06)}")
    else:
        e.shot(8.99, 10.26, "cine.mp4", C["melee"] + 6.0, text="BE ANYONE", size=110)
    e.shot(10.26, 11.98, "cine.mp4", C["melee"] + 3.0, text="PLAY ANYTHING", size=100)
    e.shot(11.98, 13.70, "tycoon.mp4", T["buy2"] + 0.8, speed=1.3, crop=HUDLESS)
    e.shot(13.70, 15.45, "cine.mp4", C["bots"] + 5.5, speed=1.4, text="TOGETHER", size=110)
    # The bomb ticks under the title; the explosion lands exactly on the drop.
    e.shot(15.45, 17.65, "cine.mp4", boom1 - 0.15 - 2.2, punch=False, text="SPIRE WARS", size=110, ty=y_top)
    # 17.65: THE DROP - the Green tower comes down
    e.shot(17.65, 20.23, "cine.mp4", boom1 - 0.15, speed=0.7, flash=True, shake=True, punch=False)
    # 20.23 - 25.08: the battle
    battle = [("cine.mp4", C["melee"] + 1.0), ("battleB.mp4", 15.0), ("cine.mp4", boom2 - 0.05),
              ("battleA.mp4", 35.6), ("cine.mp4", C["melee"] + 8.0), ("battleB.mp4", 43.6)]
    t = 20.23
    for src, at in battle:
        end = min(t + 0.86, 25.08)
        if end - t < 0.3:
            break
        e.shot(t, end, src, at, shake=(src == "cine.mp4" and at == boom2 - 0.05))
        t = end
    if t < 25.08:
        e.shot(t, 25.08, "cine.mp4", C["melee"] + 12.0)
    # 25.08 - 26.24: the dip: slow motion
    e.shot(25.08, 26.24, "battleB.mp4", 44.15, speed=0.45, punch=False)
    # 26.24 - 29.98: towers fall
    e.shot(26.24, 27.96, "cine.mp4", boom2 - 0.1, speed=0.8, flash=True, shake=True, punch=False)
    e.shot(27.96, 29.98, "solo.mp4", bomb_boom + 0.4, speed=0.85, punch=False)
    # 29.98 - 31.58: the fill: a flash cut on every hit
    fill = [("studio_code.mp4", K["play"] + 6.0, code_view), ("solo.mp4", rocket_boom - 0.05, None),
            ("battleA.mp4", 36.4, None), ("cine.mp4", boom1 + 0.25, None), ("battleB.mp4", 16.2, None)]
    hits = [29.98, 30.26, 30.56, 30.84, 31.11, 31.58]
    for (a, b), (src, at, crop) in zip(zip(hits, hits[1:]), fill):
        e.shot(a, b, src, at, crop=crop, flash=True, shake=True)
    # 31.58 - 33.20: silence
    e.black(31.58, 33.20, [("BUILD IT.", 64, f"(h/2)-{round(120 * S)}", 0.05),
                           ("PLAY IT.", 64, f"(h/2)-{round(30 * S)}", 0.55),
                           ("SHARE IT.", 64, f"(h/2)+{round(60 * S)}", 1.05)])
    # 33.20: THE FINAL DROP - the title; 36.13: the tagline
    e.shot(33.20, 36.13, "cine.mp4", C["push"] + 1.0, speed=0.6, flash=True, dark=True, text="BRIXO", size=190)
    e.shot(36.13, 41.85, "cine.mp4", C["push"] + 2.8, speed=0.5, dark=True, punch=False, text="BRIXO", size=190,
           sub="BUILD  -  PLAY  -  SHARE")

    e.finish("brixo-reveal-trailer.mp4")
    print(f"Done: {OUT / 'brixo-reveal-trailer.mp4'}")


if __name__ == "__main__":
    main()
