//! Brixo's sounds and music, synthesized in code like an old game console:
//! square and triangle waves, noise, and simple envelopes. Nothing to load,
//! and it all matches the retro look.

use std::collections::HashMap;
use std::f32::consts::TAU;
use std::sync::Arc;

/// Samples per second (mono).
pub const RATE: u32 = 44_100;

// --- building blocks ---------------------------------------------------------

#[derive(Clone, Copy)]
enum Wave {
    /// A square wave with this duty cycle (0.5 = hollow, 0.25 = nasal).
    Square(f32),
    Triangle,
    Sine,
    Noise,
}

fn osc(wave: Wave, phase: f32, seed: &mut u32) -> f32 {
    let p = phase.fract();
    match wave {
        Wave::Square(duty) => if p < duty { 1.0 } else { -1.0 },
        Wave::Triangle => 1.0 - 4.0 * (p - 0.5).abs(),
        Wave::Sine => (p * TAU).sin(),
        Wave::Noise => {
            *seed ^= *seed << 13;
            *seed ^= *seed >> 17;
            *seed ^= *seed << 5;
            (*seed as f32 / u32::MAX as f32) * 2.0 - 1.0
        }
    }
}

/// One note: `from`..`to` Hz (a slide when they differ), `secs` long, with
/// a short attack and a decay to silence.
fn tone(wave: Wave, from: f32, to: f32, secs: f32, volume: f32) -> Vec<f32> {
    let n = (secs * RATE as f32) as usize;
    let mut out = Vec::with_capacity(n);
    let (mut phase, mut seed) = (0.0f32, 0x9E37_79B9u32);
    let attack = (0.004 * RATE as f32) as usize;
    for i in 0..n {
        let t = i as f32 / n as f32;
        let freq = from + (to - from) * t;
        phase += freq / RATE as f32;
        let env = if i < attack { i as f32 / attack as f32 } else { (1.0 - t).powf(1.6) };
        out.push(osc(wave, phase, &mut seed) * env * volume);
    }
    out
}

/// Adds `src` into `dst` starting `at` seconds in (growing `dst` if needed).
fn mix(dst: &mut Vec<f32>, src: &[f32], at: f32) {
    let start = (at * RATE as f32) as usize;
    if dst.len() < start + src.len() {
        dst.resize(start + src.len(), 0.0);
    }
    for (i, s) in src.iter().enumerate() {
        dst[start + i] += s;
    }
}

fn hz(midi: i32) -> f32 {
    440.0 * 2f32.powf((midi - 69) as f32 / 12.0)
}

// --- sound effects -------------------------------------------------------------

/// A sound effect by name (see `brixo_core::SOUNDS`); None if unknown.
pub fn sound(name: &str) -> Option<Vec<f32>> {
    let sq = Wave::Square(0.5);
    let mut out = Vec::new();
    match name {
        // The classic two-note coin.
        "coin" => {
            mix(&mut out, &tone(sq, hz(83), hz(83), 0.07, 0.35), 0.0);
            mix(&mut out, &tone(sq, hz(88), hz(88), 0.3, 0.35), 0.07);
        }
        // A cash register: a rattle, then two bright bells.
        "cash" => {
            mix(&mut out, &tone(Wave::Noise, 0.0, 0.0, 0.06, 0.25), 0.0);
            mix(&mut out, &tone(Wave::Sine, hz(96), hz(96), 0.45, 0.35), 0.05);
            mix(&mut out, &tone(Wave::Sine, hz(100), hz(100), 0.5, 0.3), 0.12);
            mix(&mut out, &tone(Wave::Square(0.25), hz(84), hz(84), 0.2, 0.12), 0.12);
        }
        // A rising "you got it" arpeggio.
        "buy" => {
            for (i, m) in [72, 76, 79, 84].iter().enumerate() {
                mix(&mut out, &tone(Wave::Square(0.25), hz(*m), hz(*m), 0.09, 0.3), i as f32 * 0.07);
            }
            mix(&mut out, &tone(Wave::Triangle, hz(84), hz(84), 0.4, 0.4), 0.28);
        }
        "click" => mix(&mut out, &tone(sq, 1400.0, 1400.0, 0.03, 0.25), 0.0),
        "error" => {
            mix(&mut out, &tone(sq, hz(57), hz(57), 0.12, 0.3), 0.0);
            mix(&mut out, &tone(sq, hz(52), hz(52), 0.2, 0.3), 0.13);
        }
        "pop" => mix(&mut out, &tone(Wave::Sine, 900.0, 250.0, 0.09, 0.5), 0.0),
        "jump" => mix(&mut out, &tone(Wave::Square(0.25), 280.0, 720.0, 0.16, 0.3), 0.0),
        "hit" => {
            mix(&mut out, &tone(Wave::Noise, 0.0, 0.0, 0.12, 0.4), 0.0);
            mix(&mut out, &tone(Wave::Sine, 160.0, 60.0, 0.15, 0.6), 0.0);
        }
        "win" => {
            for (i, m) in [72, 76, 79].iter().enumerate() {
                mix(&mut out, &tone(Wave::Square(0.5), hz(*m), hz(*m), 0.14, 0.28), i as f32 * 0.13);
            }
            mix(&mut out, &tone(Wave::Square(0.5), hz(84), hz(84), 0.7, 0.3), 0.39);
            mix(&mut out, &tone(Wave::Triangle, hz(60), hz(60), 0.9, 0.45), 0.39);
        }
        "whoosh" => mix(&mut out, &tone(Wave::Noise, 0.0, 0.0, 0.35, 0.22), 0.0),
        // Falling apart: a wobbly drop in pitch and a little crunch.
        "death" => {
            mix(&mut out, &tone(Wave::Square(0.5), 620.0, 140.0, 0.38, 0.32), 0.0);
            mix(&mut out, &tone(Wave::Noise, 0.0, 0.0, 0.1, 0.18), 0.0);
            mix(&mut out, &tone(Wave::Triangle, 310.0, 70.0, 0.42, 0.3), 0.02);
        }
        // An explosion: a roar of noise over a deep thump.
        "boom" => {
            // (Kept under full volume even where all three peak together.)
            mix(&mut out, &tone(Wave::Noise, 0.0, 0.0, 0.8, 0.36), 0.0);
            mix(&mut out, &tone(Wave::Sine, 110.0, 30.0, 0.9, 0.5), 0.0);
            mix(&mut out, &tone(Wave::Square(0.5), 70.0, 35.0, 0.35, 0.12), 0.0);
        }
        // Slingshot: a quick plucked-band twang, rising then settling.
        "twang" => {
            mix(&mut out, &tone(Wave::Triangle, 180.0, 520.0, 0.12, 0.5), 0.0);
            mix(&mut out, &tone(Wave::Square(0.5), 90.0, 60.0, 0.05, 0.15), 0.0);
        }
        // Superball: a hollow, springy knock, pitched a little differently
        // each time it's called would need a seed, so this is one fixed bonk.
        "bonk" => {
            mix(&mut out, &tone(Wave::Sine, 300.0, 140.0, 0.12, 0.55), 0.0);
            mix(&mut out, &tone(Wave::Triangle, 600.0, 200.0, 0.05, 0.2), 0.0);
        }
        // Paintball: a short wet splat, mostly noise with a low thump.
        "splat" => {
            mix(&mut out, &tone(Wave::Noise, 0.0, 0.0, 0.09, 0.4), 0.0);
            mix(&mut out, &tone(Wave::Sine, 180.0, 90.0, 0.08, 0.3), 0.005);
        }
        // Trowel: a soft, dusty thud as the wall lands.
        "thud" => {
            mix(&mut out, &tone(Wave::Sine, 90.0, 45.0, 0.16, 0.45), 0.0);
            mix(&mut out, &tone(Wave::Noise, 0.0, 0.0, 0.08, 0.2), 0.0);
        }
        _ => return None,
    }
    Some(out)
}

// --- music -----------------------------------------------------------------------

/// A music loop by name (see `brixo_core::MUSIC`); None if unknown. Loops
/// cleanly: play it on repeat.
pub fn music(name: &str) -> Option<Vec<f32>> {
    match name {
        // Bright and relaxed: C, Am, F, G at 112 bpm. For tycoons and hangouts.
        "sunny" => Some(song(112.0, [60, 57, 53, 55], false, 0x5eed)),
        // Minor and driving at 150 bpm. For rounds and chases.
        "rush" => Some(song(150.0, [57, 53, 60, 55], true, 0xbeef)),
        _ => None,
    }
}

/// Eight bars: the chord progression played twice, with a triangle bass,
/// a square-wave melody picked from each chord, and noise drums.
fn song(bpm: f32, roots: [i32; 4], minor: bool, mut seed: u32) -> Vec<f32> {
    let beat = 60.0 / bpm;
    let bars = 8;
    let mut out = vec![0.0; (bars as f32 * 4.0 * beat * RATE as f32) as usize];
    let mut rand = || {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        seed
    };
    let melody_pattern: Vec<u32> = (0..32).map(|_| rand()).collect();
    for bar in 0..bars {
        let root = roots[bar % 4];
        // The chord: root, third, fifth (third is minor on the 2nd chord in
        // major songs, and on the 1st and 2nd in minor songs).
        let minor_chord = if minor { bar % 4 < 2 } else { bar % 4 == 1 };
        let chord = [root, root + if minor_chord { 3 } else { 4 }, root + 7];
        let t0 = bar as f32 * 4.0 * beat;
        for b in 0..4 {
            let t = t0 + b as f32 * beat;
            // Bass: root on the beat, fifth on the off-beat.
            mix(&mut out, &tone(Wave::Triangle, hz(root - 24), hz(root - 24), beat * 0.45, 0.45), t);
            mix(&mut out, &tone(Wave::Triangle, hz(root - 17), hz(root - 17), beat * 0.4, 0.35), t + beat / 2.0);
            // Drums: kick on 1 and 3, snare on 2 and 4, hats every eighth.
            if b % 2 == 0 {
                mix(&mut out, &tone(Wave::Sine, 150.0, 45.0, 0.14, 0.55), t);
            } else {
                mix(&mut out, &tone(Wave::Noise, 0.0, 0.0, 0.12, 0.22), t);
            }
            for h in 0..2 {
                mix(&mut out, &tone(Wave::Noise, 0.0, 0.0, 0.03, 0.08), t + h as f32 * beat / 2.0);
            }
            // Melody: eighth notes from the chord, an octave up, with rests.
            for h in 0..2 {
                let r = melody_pattern[((bar % 4) * 8 + b * 2 + h) % 32];
                if r % 5 == 0 {
                    continue; // a rest now and then keeps it breathing
                }
                let note = chord[(r % 3) as usize] + 12 + if r % 7 == 0 { 12 } else { 0 };
                let len = if r % 3 == 0 { beat } else { beat * 0.45 };
                mix(&mut out, &tone(Wave::Square(0.25), hz(note), hz(note), len, 0.16), t + h as f32 * beat / 2.0);
            }
        }
    }
    let len = out.len();
    out.truncate(len);
    // Keep the mix out of clipping.
    let peak = out.iter().fold(0.0f32, |m, s| m.max(s.abs())).max(1e-6);
    let gain = 0.8 / peak;
    out.iter().map(|s| s * gain).collect()
}

// --- playback --------------------------------------------------------------------

/// Plays sounds and music on this computer's speakers. If there's no audio
/// device, everything is silently skipped.
pub struct Speaker {
    _stream: rodio::OutputStream,
    handle: rodio::OutputStreamHandle,
    music: Option<Playing>,
    cache: HashMap<String, Arc<Vec<f32>>>,
    /// The player's own volume settings, 0 to 1 (1 is Brixo's normal mix).
    sound_level: f32,
    music_level: f32,
    /// Your kart's engine, humming while you drive.
    engine: Option<rodio::Sink>,
}

/// A kart engine's hum: a soft, low purr, a second long (a whole number of
/// every wave in it, so it loops without a click). Round sine partials,
/// not buzzy saws, with a gentle putt-putt and a breath of air in it.
pub fn engine_loop() -> Vec<f32> {
    use std::f32::consts::TAU;
    let n = RATE as usize;
    let overlap = n / 20;
    let mut noise = 0x2545_f491u32;
    let mut air = 0.0f32;
    let mut out: Vec<f32> = (0..n + overlap)
        .map(|i| {
            let t = i as f32 / RATE as f32;
            let sine = |f: f32| (t * f * TAU).sin();
            // The cylinder beat: soft pulses 24 times a second.
            let putt = 0.8 + 0.2 * sine(24.0).max(0.0);
            let body = sine(48.0) * 0.6 + sine(96.0) * 0.25 + sine(144.0) * 0.08 + sine(72.0) * 0.12;
            noise ^= noise << 13;
            noise ^= noise >> 17;
            noise ^= noise << 5;
            let white = noise as f32 / u32::MAX as f32 * 2.0 - 1.0;
            air += (white - air) * 0.02;
            (body * putt + air * 0.6) * 0.45
        })
        .collect();
    // The air doesn't repeat by itself: blend the run-on past the end into
    // the start, so the loop's seam can't click.
    for i in 0..overlap {
        let k = i as f32 / overlap as f32;
        out[i] = out[i] * k + out[n + i] * (1.0 - k);
    }
    out.truncate(n);
    out
}

/// The song that's playing: what it is, when (in its own time) it started,
/// and how long one loop is, so the next song can pick up on the beat.
struct Playing {
    key: String,
    sink: Arc<rodio::Sink>,
    started: std::time::Instant,
    length: Option<std::time::Duration>,
    /// Bumped to call off a fade-in that's still going.
    fading: Arc<std::sync::atomic::AtomicUsize>,
    /// Its volume before the player's music setting.
    base: f32,
}

/// How long switching songs takes: the old one fades out as the new one
/// fades in.
pub const CROSSFADE: std::time::Duration = std::time::Duration::from_millis(900);

/// Where a new song should start so it stays on the beat of the old one:
/// if both loop at the same length (two versions of one song, like a calm
/// and an intense mix), at the same point in the loop; otherwise from the
/// top.
pub fn sync_offset(elapsed: std::time::Duration, old: Option<std::time::Duration>, new: Option<std::time::Duration>) -> std::time::Duration {
    match (old, new) {
        (Some(a), Some(b)) if !b.is_zero() && a.abs_diff(b) < std::time::Duration::from_millis(5) => {
            std::time::Duration::from_secs_f64(elapsed.as_secs_f64() % b.as_secs_f64())
        }
        _ => std::time::Duration::ZERO,
    }
}

/// Ramps a sink's volume from `from` to `to` over the crossfade, on its own
/// thread, then stops it if `to` is silence. Gives up if `guard` changes.
fn fade(sink: Arc<rodio::Sink>, from: f32, to: f32, guard: Option<(Arc<std::sync::atomic::AtomicUsize>, usize)>) {
    std::thread::spawn(move || {
        const STEPS: u32 = 18;
        for i in 1..=STEPS {
            std::thread::sleep(CROSSFADE / STEPS);
            if let Some((g, mine)) = &guard {
                if g.load(std::sync::atomic::Ordering::Relaxed) != *mine {
                    return;
                }
            }
            let t = i as f32 / STEPS as f32;
            sink.set_volume(from + (to - from) * t);
        }
        if to <= 0.0 {
            sink.stop();
        }
    });
}

pub const SOUND_VOLUME: f32 = 0.7;
pub const MUSIC_VOLUME: f32 = 0.35;

impl Speaker {
    pub fn new() -> Option<Speaker> {
        let (stream, handle) = rodio::OutputStream::try_default().ok()?;
        Some(Speaker { _stream: stream, handle, music: None, cache: HashMap::new(), sound_level: 1.0, music_level: 1.0, engine: None })
    }

    /// The player's volume settings for sound effects and music (0 to 1).
    /// The music that's playing changes at once.
    pub fn set_levels(&mut self, sounds: f32, music: f32) {
        self.sound_level = sounds.clamp(0.0, 1.0);
        self.music_level = music.clamp(0.0, 1.0);
        if let Some(m) = &self.music {
            m.fading.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            m.sink.set_volume(m.base * self.music_level);
        }
    }

    fn samples(&mut self, key: &str, make: impl FnOnce() -> Option<Vec<f32>>) -> Option<Arc<Vec<f32>>> {
        if let Some(s) = self.cache.get(key) {
            return Some(s.clone());
        }
        let s = Arc::new(make()?);
        self.cache.insert(key.to_string(), s.clone());
        Some(s)
    }

    /// Your kart's engine: humming at `pitch` (1 is its idle note, 2 an
    /// octave up at full speed), or silent (None).
    pub fn engine(&mut self, pitch: Option<f32>) {
        match pitch {
            None => {
                if let Some(sink) = self.engine.take() {
                    sink.stop();
                }
            }
            Some(pitch) => {
                if self.engine.is_none() {
                    let Ok(sink) = rodio::Sink::try_new(&self.handle) else { return };
                    let buf = rodio::buffer::SamplesBuffer::new(1, RATE, engine_loop());
                    sink.append(rodio::Source::repeat_infinite(buf));
                    self.engine = Some(sink);
                }
                let sink = self.engine.as_ref().unwrap();
                let pitch = pitch.clamp(0.6, 2.0);
                sink.set_speed(pitch);
                // Quiet ticking over, fuller flat out, never shouting.
                let loud = 0.05 + 0.07 * ((pitch - 0.8) / 0.8).clamp(0.0, 1.0);
                sink.set_volume(loud * self.sound_level);
            }
        }
    }

    /// Plays a sound effect once.
    pub fn play(&mut self, name: &str) {
        let Some(s) = self.samples(name, || sound(name)) else { return };
        let buf = rodio::buffer::SamplesBuffer::new(1, RATE, s.to_vec());
        let _ = self.handle.play_raw(rodio::Source::amplify(buf, SOUND_VOLUME * self.sound_level));
    }

    /// Plays an audio file (mp3, wav or ogg) once, at `volume` (0 to 1).
    pub fn play_file(&mut self, bytes: Arc<Vec<u8>>, volume: f32) {
        if let Ok(source) = rodio::Decoder::new(std::io::Cursor::new(bytes.to_vec())) {
            let _ = self.handle.play_raw(rodio::Source::amplify(rodio::Source::convert_samples::<f32>(source), volume * SOUND_VOLUME * self.sound_level));
        }
    }

    /// Fades out whatever's playing. Returns it, to line the next song up.
    fn fade_out_music(&mut self) -> Option<Playing> {
        let old = self.music.take()?;
        old.fading.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        fade(old.sink.clone(), old.sink.volume(), 0.0, None);
        Some(old)
    }

    /// Loops an audio file as music (`key` names it, so asking for the same
    /// one again doesn't restart it). Switching songs crossfades, and two
    /// versions of the same song (the same loop length) stay on the beat.
    pub fn music_file(&mut self, key: &str, bytes: Arc<Vec<u8>>, volume: f32) {
        if self.music.as_ref().map(|m| m.key.as_str()) == Some(key) {
            return;
        }
        let Ok(source) = rodio::Decoder::new(std::io::Cursor::new(bytes.to_vec())) else { return };
        let length = rodio::Source::total_duration(&source);
        let Ok(sink) = rodio::Sink::try_new(&self.handle) else { return };
        let old = self.fade_out_music();
        let offset = match &old {
            Some(o) => sync_offset(o.started.elapsed(), o.length, length),
            None => std::time::Duration::ZERO,
        };
        let looped = rodio::Source::repeat_infinite(rodio::Source::buffered(source));
        sink.append(rodio::Source::skip_duration(looped, offset));
        let base = volume * MUSIC_VOLUME * 2.0;
        let target = base * self.music_level;
        let sink = Arc::new(sink);
        let fading = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        if old.is_some() {
            sink.set_volume(0.0);
            fade(sink.clone(), 0.0, target, Some((fading.clone(), 0)));
        } else {
            sink.set_volume(target);
        }
        let started = std::time::Instant::now().checked_sub(offset).unwrap_or_else(std::time::Instant::now);
        self.music = Some(Playing { key: key.to_string(), sink, started, length, fading, base });
    }

    /// Loops a music track (None stops the music). Asking for the track
    /// that's already playing doesn't restart it.
    pub fn music(&mut self, name: Option<&str>) {
        if name.is_some() && self.music.as_ref().map(|m| m.key.as_str()) == name {
            return;
        }
        let _ = self.fade_out_music();
        let Some(name) = name else { return };
        let Some(s) = self.samples(&format!("music:{name}"), || music(name)) else { return };
        let Ok(sink) = rodio::Sink::try_new(&self.handle) else { return };
        let buf = rodio::buffer::SamplesBuffer::new(1, RATE, s.to_vec());
        sink.append(rodio::Source::repeat_infinite(buf));
        sink.set_volume(MUSIC_VOLUME * self.music_level);
        let fading = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        self.music = Some(Playing { key: name.to_string(), sink: Arc::new(sink), started: std::time::Instant::now(), length: None, fading, base: MUSIC_VOLUME });
    }
}

// --- sounds in the world -----------------------------------------------------------

/// Full volume this close to a sound (studs).
pub const NEAR: f32 = 15.0;
/// Silent this far away (studs).
pub const FAR: f32 = 250.0;

/// How loud a sound at `at` is in each ear, for a listener at `ear` whose
/// right is `right` (a direction of length 1): full volume up close, then
/// fading with distance (halved at twice `NEAR`) to silence at `FAR`; and
/// louder in the ear it's nearer.
pub fn spatial_gains(ear: [f32; 3], right: [f32; 3], at: [f32; 3]) -> [f32; 2] {
    let d = [at[0] - ear[0], at[1] - ear[1], at[2] - ear[2]];
    let dist = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
    if dist >= FAR {
        return [0.0, 0.0];
    }
    let mut loud = if dist <= NEAR { 1.0 } else { NEAR / dist };
    // Fading out over the last fifth, so it doesn't cut off.
    let edge = FAR * 0.8;
    if dist > edge {
        loud *= 1.0 - (dist - edge) / (FAR - edge);
    }
    // Which side: -1 hard left, 1 hard right; less so up close.
    let side = if dist < 0.5 { 0.0 } else { (d[0] * right[0] + d[1] * right[1] + d[2] * right[2]) / dist };
    let near_blend = (dist / (NEAR * 0.5)).min(1.0);
    let pan = side * 0.8 * near_blend;
    // Equal power across the middle, never louder than full in one ear.
    let a = (1.0 + pan) * std::f32::consts::FRAC_PI_4;
    let (l, r) = (a.cos() * std::f32::consts::SQRT_2, a.sin() * std::f32::consts::SQRT_2);
    [loud * l.min(1.0), loud * r.min(1.0)]
}

/// Each ear's volume for a sound in the world, shared with the sound as it
/// plays, so it can follow what made it and turn with the listener.
pub type Ears = Arc<std::sync::Mutex<[f32; 2]>>;

/// Plays `source` with its left and right volumes from `ears` (checked
/// every 10 ms), times `volume`.
fn play_placed<S>(handle: &rodio::OutputStreamHandle, source: S, volume: f32, ears: Ears)
where
    S: rodio::Source<Item = f32> + Send + 'static,
{
    let start = *ears.lock().unwrap();
    let placed = rodio::source::ChannelVolume::new(source, vec![start[0] * volume, start[1] * volume]);
    let placed = rodio::Source::periodic_access(placed, std::time::Duration::from_millis(10), move |s| {
        let g = *ears.lock().unwrap();
        s.set_volume(0, g[0] * volume);
        s.set_volume(1, g[1] * volume);
    });
    let _ = handle.play_raw(placed);
}

impl Speaker {
    /// Plays a sound effect once, from a place in the world (see `Ears`).
    pub fn play_at(&mut self, name: &str, ears: Ears) {
        let Some(s) = self.samples(name, || sound(name)) else { return };
        let buf = rodio::buffer::SamplesBuffer::new(1, RATE, s.to_vec());
        play_placed(&self.handle, buf, SOUND_VOLUME * self.sound_level, ears);
    }

    /// Plays an audio file once, from a place in the world.
    pub fn play_file_at(&mut self, bytes: Arc<Vec<u8>>, volume: f32, ears: Ears) {
        if let Ok(source) = rodio::Decoder::new(std::io::Cursor::new(bytes.to_vec())) {
            let source = rodio::Source::convert_samples::<f32>(source);
            play_placed(&self.handle, source, volume * SOUND_VOLUME * self.sound_level, ears);
        }
    }
}

/// Whether Brixo can play these bytes (an mp3, wav or ogg file).
pub fn decodes(bytes: &[u8]) -> bool {
    rodio::Decoder::new(std::io::Cursor::new(bytes.to_vec())).is_ok()
}

/// Writes mono samples as a 16-bit WAV file.
pub fn write_wav(path: &std::path::Path, samples: &[f32]) -> std::io::Result<()> {
    let mut bytes = Vec::with_capacity(44 + samples.len() * 2);
    let data_len = (samples.len() * 2) as u32;
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data_len).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes()); // PCM
    bytes.extend_from_slice(&1u16.to_le_bytes()); // mono
    bytes.extend_from_slice(&RATE.to_le_bytes());
    bytes.extend_from_slice(&(RATE * 2).to_le_bytes());
    bytes.extend_from_slice(&2u16.to_le_bytes());
    bytes.extend_from_slice(&16u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        bytes.extend_from_slice(&((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes());
    }
    std::fs::write(path, bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_files_are_recognised() {
        let path = std::env::temp_dir().join("brixo-test-sound.wav");
        write_wav(&path, &sound("coin").unwrap()).unwrap();
        assert!(decodes(&std::fs::read(&path).unwrap()), "a wav plays");
        assert!(!decodes(b"definitely not audio"), "junk doesn't");
    }

    #[test]
    fn every_named_sound_and_track_exists_and_is_sane() {
        for name in brixo_core::SOUNDS {
            let s = sound(name).unwrap_or_else(|| panic!("{name} missing"));
            let secs = s.len() as f32 / RATE as f32;
            assert!(secs > 0.02 && secs < 2.0, "{name}: {secs}s");
            assert!(s.iter().all(|x| x.is_finite() && x.abs() <= 1.0), "{name} clips");
            assert!(s.iter().any(|x| x.abs() > 0.05), "{name} is silent");
        }
        for name in ["sunny", "rush"] {
            let m = music(name).unwrap();
            let secs = m.len() as f32 / RATE as f32;
            assert!(secs > 10.0 && secs < 40.0, "{name}: {secs}s");
            assert!(m.iter().all(|x| x.abs() <= 0.81));
        }
        assert!(sound("nope").is_none() && music("nope").is_none());
    }

    #[test]
    fn sounds_fade_with_distance_and_come_from_their_side() {
        let (ear, right) = ([0.0, 0.0, 0.0], [1.0, 0.0, 0.0]);
        let close = spatial_gains(ear, right, [0.0, 0.0, 5.0]);
        assert!((close[0] - 1.0).abs() < 0.01 && (close[1] - 1.0).abs() < 0.01, "right in front, close: full, both ears: {close:?}");
        let mid = spatial_gains(ear, right, [0.0, 0.0, 60.0]);
        assert!(mid[0] < 0.3 && mid[0] > 0.2, "60 studs off: a quarter: {mid:?}");
        assert_eq!(spatial_gains(ear, right, [0.0, 0.0, 300.0]), [0.0, 0.0], "far across the map: silent");
        let edge = spatial_gains(ear, right, [0.0, 0.0, 240.0]);
        assert!(edge[0] < 0.02, "fading out near the edge: {edge:?}");
        let to_right = spatial_gains(ear, right, [40.0, 0.0, 0.0]);
        assert!(to_right[1] > to_right[0] * 3.0, "off to the right: louder on the right: {to_right:?}");
        let to_left = spatial_gains(ear, right, [-40.0, 0.0, 0.0]);
        assert!(to_left[0] > to_left[1] * 3.0, "and the left: {to_left:?}");
        assert!(to_right[1] <= 1.0);
    }

    #[test]
    fn two_mixes_of_one_song_switch_on_the_beat() {
        use std::time::Duration;
        let loop_len = Some(Duration::from_secs_f64(27.43));
        // 40s into the calm mix: the intense one starts 12.57s in.
        let at = sync_offset(Duration::from_secs(40), loop_len, loop_len);
        assert!((at.as_secs_f64() - 12.57).abs() < 1e-6, "{at:?}");
        // A different song starts from the top.
        assert_eq!(sync_offset(Duration::from_secs(40), loop_len, Some(Duration::from_secs(90))), Duration::ZERO);
        assert_eq!(sync_offset(Duration::from_secs(40), None, loop_len), Duration::ZERO);
    }
}
