//! A tiny chiptune synthesizer, so sample games can ship original music and
//! sound effects made entirely in code (no licensing questions, and the
//! same bytes every build). Everything renders to a mono 16-bit WAV, the
//! same thing a creator gets by dropping a .wav on the studio.

use std::f32::consts::TAU;

pub const RATE: u32 = 22_050;

/// A buffer of samples to mix voices into.
pub struct Track {
    pub s: Vec<f32>,
}

fn midi_hz(note: f32) -> f32 {
    440.0 * 2f32.powf((note - 69.0) / 12.0)
}

/// A tiny deterministic noise source (the same drums every build).
struct Noise(u32);
impl Noise {
    fn next(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        (self.0 as f32 / u32::MAX as f32) * 2.0 - 1.0
    }
}

#[derive(Clone, Copy)]
pub enum Wave {
    /// A pulse wave with this duty cycle (0.5 is a square).
    Pulse(f32),
    Triangle,
    Saw,
}

fn osc(wave: Wave, phase: f32) -> f32 {
    let p = phase.fract();
    match wave {
        Wave::Pulse(duty) => if p < duty { 1.0 } else { -1.0 },
        Wave::Triangle => 1.0 - 4.0 * (p - 0.5).abs(),
        Wave::Saw => 2.0 * p - 1.0,
    }
}

impl Track {
    pub fn new(seconds: f32) -> Track {
        Track { s: vec![0.0; (seconds * RATE as f32) as usize] }
    }

    /// One note: `start` and `length` in seconds. A short attack, a gentle
    /// decay to `sustain`, and a release, so notes don't click.
    #[allow(clippy::too_many_arguments)]
    pub fn note(&mut self, start: f32, length: f32, midi: f32, wave: Wave, volume: f32, sustain: f32, vibrato: f32) {
        let hz = midi_hz(midi);
        let a = (start * RATE as f32) as usize;
        let n = ((length + 0.04) * RATE as f32) as usize;
        let mut phase = 0.0f32;
        let mut smooth = 0.0f32;
        for i in 0..n {
            let t = i as f32 / RATE as f32;
            let attack = (t / 0.006).min(1.0);
            let decay = sustain + (1.0 - sustain) * (-t * 9.0).exp();
            let release = if t > length { (1.0 - (t - length) / 0.04).max(0.0) } else { 1.0 };
            let wobble = 1.0 + vibrato * (t * 5.5 * TAU).sin() * (t / 0.25).min(1.0);
            phase += hz * wobble / RATE as f32;
            // A one-pole low-pass softens the harsh edges of pulse waves.
            smooth += (osc(wave, phase) - smooth) * 0.35;
            if let Some(s) = self.s.get_mut(a + i) {
                *s += smooth * volume * attack * decay * release;
            }
        }
    }

    pub fn kick(&mut self, start: f32, volume: f32) {
        let a = (start * RATE as f32) as usize;
        let mut phase = 0.0f32;
        for i in 0..(0.22 * RATE as f32) as usize {
            let t = i as f32 / RATE as f32;
            let hz = 45.0 + 120.0 * (-t * 28.0).exp();
            phase += hz / RATE as f32;
            if let Some(s) = self.s.get_mut(a + i) {
                *s += (phase * TAU).sin() * volume * (-t * 14.0).exp();
            }
        }
    }

    pub fn snare(&mut self, start: f32, volume: f32, seed: u32) {
        let a = (start * RATE as f32) as usize;
        let mut noise = Noise(seed | 1);
        for i in 0..(0.18 * RATE as f32) as usize {
            let t = i as f32 / RATE as f32;
            let tone = (t * 185.0 * TAU).sin() * (-t * 30.0).exp();
            if let Some(s) = self.s.get_mut(a + i) {
                *s += (noise.next() * 0.8 * (-t * 17.0).exp() + tone * 0.5) * volume;
            }
        }
    }

    pub fn hat(&mut self, start: f32, volume: f32, open: bool, seed: u32) {
        let a = (start * RATE as f32) as usize;
        let mut noise = Noise(seed | 1);
        let mut last = 0.0;
        let decay = if open { 14.0 } else { 70.0 };
        for i in 0..(0.12 * RATE as f32) as usize {
            let t = i as f32 / RATE as f32;
            let n = noise.next();
            // The difference of successive samples: a crude high-pass.
            let hp = n - last;
            last = n;
            if let Some(s) = self.s.get_mut(a + i) {
                *s += hp * 0.5 * volume * (-t * decay).exp();
            }
        }
    }

    /// A soft electric piano: two sine waves, one bending the other (FM),
    /// bright when struck and mellowing as it rings.
    pub fn piano(&mut self, start: f32, length: f32, midi: f32, volume: f32) {
        let hz = midi_hz(midi);
        let a = (start * RATE as f32) as usize;
        let ring = length.max(0.3) + 1.2;
        let n = (ring * RATE as f32) as usize;
        for i in 0..n {
            let t = i as f32 / RATE as f32;
            let attack = (t / 0.004).min(1.0);
            let body = (-t * 1.6).exp();
            let release = if t > length { (-(t - length) * 5.0).exp() } else { 1.0 };
            let index = 0.3 + 1.8 * (-t * 7.0).exp();
            let m = (t * hz * TAU).sin() * index;
            let v = (t * hz * TAU + m).sin() + 0.25 * (t * hz * 2.0 * TAU).sin() * (-t * 4.0).exp();
            let tremolo = 1.0 + 0.06 * (t * 4.5 * TAU).sin();
            if let Some(s) = self.s.get_mut(a + i) {
                *s += v * volume * attack * body * release * tremolo;
            }
        }
    }

    /// A warm pad: several slightly out-of-tune saw waves, softened,
    /// fading in and out slowly.
    pub fn pad(&mut self, start: f32, length: f32, midi: f32, volume: f32) {
        let a = (start * RATE as f32) as usize;
        let n = ((length + 1.2) * RATE as f32) as usize;
        let detune = [-0.12f32, -0.05, 0.0, 0.06, 0.13];
        let mut phases = [0.0f32; 5];
        let (mut lp1, mut lp2) = (0.0f32, 0.0f32);
        for i in 0..n {
            let t = i as f32 / RATE as f32;
            let attack = (t / 0.9).min(1.0);
            let release = if t > length { (1.0 - (t - length) / 1.2).max(0.0) } else { 1.0 };
            let mut v = 0.0;
            for (k, d) in detune.iter().enumerate() {
                phases[k] += midi_hz(midi + d) / RATE as f32;
                v += osc(Wave::Saw, phases[k]);
            }
            // Two gentle low-pass stages take the buzz off: warm, not bright.
            lp1 += (v / 5.0 - lp1) * 0.09;
            lp2 += (lp1 - lp2) * 0.09;
            if let Some(s) = self.s.get_mut(a + i) {
                *s += lp2 * volume * attack * release;
            }
        }
    }

    /// A bell: a few sine partials at bell-like (not quite harmonic)
    /// ratios, the high ones dying away first.
    pub fn bell(&mut self, start: f32, midi: f32, volume: f32) {
        let hz = midi_hz(midi);
        let a = (start * RATE as f32) as usize;
        let n = (3.0 * RATE as f32) as usize;
        let partials = [(1.0f32, 1.0f32, 1.6f32), (2.0, 0.35, 2.6), (2.76, 0.22, 4.0), (5.4, 0.08, 7.0)];
        for i in 0..n {
            let t = i as f32 / RATE as f32;
            let attack = (t / 0.002).min(1.0);
            let v: f32 = partials.iter().map(|(r, amp, decay)| (t * hz * r * TAU).sin() * amp * (-t * decay).exp()).sum();
            if let Some(s) = self.s.get_mut(a + i) {
                *s += v * volume * attack;
            }
        }
    }

    /// A soft bass: a round sine with a touch of triangle for body.
    pub fn bass(&mut self, start: f32, length: f32, midi: f32, volume: f32) {
        let hz = midi_hz(midi);
        let a = (start * RATE as f32) as usize;
        let n = ((length + 0.08) * RATE as f32) as usize;
        let mut phase = 0.0f32;
        for i in 0..n {
            let t = i as f32 / RATE as f32;
            let attack = (t / 0.01).min(1.0);
            let release = if t > length { (1.0 - (t - length) / 0.08).max(0.0) } else { 1.0 };
            phase += hz / RATE as f32;
            let v = (phase * TAU).sin() * 0.85 + osc(Wave::Triangle, phase) * 0.15;
            if let Some(s) = self.s.get_mut(a + i) {
                *s += v * volume * attack * release * (0.75 + 0.25 * (-t * 3.0).exp());
            }
        }
    }

    /// A finger snap / rim click: a short burst of filtered noise.
    pub fn snap(&mut self, start: f32, volume: f32, seed: u32) {
        let a = (start * RATE as f32) as usize;
        let mut noise = Noise(seed | 1);
        let mut bp = 0.0f32;
        let mut last = 0.0f32;
        for i in 0..(0.12 * RATE as f32) as usize {
            let t = i as f32 / RATE as f32;
            let n = noise.next();
            let hp = n - last;
            last = n;
            bp += (hp - bp) * 0.5;
            let tone = (t * 1700.0 * TAU).sin() * (-t * 80.0).exp();
            if let Some(s) = self.s.get_mut(a + i) {
                *s += (bp * (-t * 45.0).exp() + tone * 0.3) * volume;
            }
        }
    }

    /// Room sound (a small Freeverb): what makes soft instruments feel lush.
    /// `wet` is how much of the echo to mix in.
    pub fn reverb(&mut self, wet: f32) {
        // Delay lengths from Freeverb, halved for this sample rate.
        let combs = [558usize, 594, 638, 678, 711, 745];
        let allpasses = [278usize, 220, 170, 112];
        let (feedback, damp) = (0.83f32, 0.3f32);
        let mut out = vec![0.0f32; self.s.len()];
        for &len in &combs {
            let mut buf = vec![0.0f32; len];
            let (mut pos, mut store) = (0usize, 0.0f32);
            for (i, x) in self.s.iter().enumerate() {
                let y = buf[pos];
                store = y * (1.0 - damp) + store * damp;
                buf[pos] = x * 0.015 + store * feedback;
                pos = (pos + 1) % len;
                out[i] += y;
            }
        }
        for &len in &allpasses {
            let mut buf = vec![0.0f32; len];
            let mut pos = 0usize;
            for v in out.iter_mut() {
                let b = buf[pos];
                let y = -*v + b;
                buf[pos] = *v + b * 0.5;
                pos = (pos + 1) % len;
                *v = y;
            }
        }
        for (s, r) in self.s.iter_mut().zip(out) {
            *s += r * wet * 3.0;
        }
    }

    /// Soft-clips and writes a 16-bit mono WAV file.
    pub fn wav(&self) -> Vec<u8> {
        let peak = self.s.iter().fold(0.0f32, |m, v| m.max(v.abs())).max(1e-6);
        let gain = if peak > 0.9 { 0.9 / peak } else { 1.0 };
        let len = self.s.len();
        let data = (len * 2) as u32;
        let mut out = Vec::with_capacity(44 + len * 2);
        for chunk in [
            &b"RIFF"[..], &(36 + data).to_le_bytes(), b"WAVEfmt ", &16u32.to_le_bytes(), &1u16.to_le_bytes(), &1u16.to_le_bytes(),
            &RATE.to_le_bytes(), &(RATE * 2).to_le_bytes(), &2u16.to_le_bytes(), &16u16.to_le_bytes(), b"data", &data.to_le_bytes(),
        ] {
            out.extend_from_slice(chunk);
        }
        for v in &self.s {
            let v = (v * gain * 1.2).tanh();
            out.extend_from_slice(&((v.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes());
        }
        out
    }
}

// --- Flagfall's music -------------------------------------------------------

const BPM: f32 = 140.0;
const BEAT: f32 = 60.0 / BPM;
/// Eight chords, two bars each: 16 bars, about 27 seconds, then it loops.
/// Am F C G | Am F Dm E, in A minor.
const CHORDS: [(f32, [f32; 3]); 8] = [
    (45.0, [69.0, 72.0, 76.0]),
    (41.0, [65.0, 69.0, 72.0]),
    (48.0, [67.0, 72.0, 76.0]),
    (43.0, [67.0, 71.0, 74.0]),
    (45.0, [69.0, 72.0, 76.0]),
    (41.0, [65.0, 69.0, 72.0]),
    (50.0, [65.0, 69.0, 74.0]),
    (40.0, [68.0, 71.0, 76.0]),
];

/// The tune, per chord: (beat, beats long, note).
const MELODY: [&[(f32, f32, f32)]; 8] = [
    &[(0.0, 1.5, 76.0), (1.5, 0.5, 74.0), (2.0, 1.0, 72.0), (3.0, 1.0, 71.0), (4.0, 2.5, 69.0), (6.5, 0.5, 71.0), (7.0, 1.0, 72.0)],
    &[(0.0, 1.5, 72.0), (1.5, 0.5, 71.0), (2.0, 1.0, 69.0), (3.0, 1.0, 72.0), (4.0, 3.0, 77.0), (7.0, 1.0, 76.0)],
    &[(0.0, 1.5, 76.0), (1.5, 0.5, 74.0), (2.0, 1.0, 72.0), (3.0, 1.0, 67.0), (4.0, 2.0, 72.0), (6.0, 1.0, 74.0), (7.0, 1.0, 76.0)],
    &[(0.0, 3.0, 74.0), (3.0, 1.0, 71.0), (4.0, 2.0, 67.0), (6.0, 1.0, 71.0), (7.0, 1.0, 74.0)],
    &[(0.0, 1.5, 76.0), (1.5, 0.5, 77.0), (2.0, 1.0, 79.0), (3.0, 1.0, 76.0), (4.0, 2.5, 81.0), (6.5, 0.5, 79.0), (7.0, 1.0, 77.0)],
    &[(0.0, 1.5, 77.0), (1.5, 0.5, 76.0), (2.0, 1.0, 74.0), (3.0, 1.0, 72.0), (4.0, 3.0, 69.0), (7.0, 1.0, 72.0)],
    &[(0.0, 1.5, 74.0), (1.5, 0.5, 72.0), (2.0, 1.0, 74.0), (3.0, 1.0, 77.0), (4.0, 2.0, 76.0), (6.0, 1.0, 74.0), (7.0, 1.0, 72.0)],
    &[(0.0, 2.0, 71.0), (2.0, 1.0, 68.0), (3.0, 1.0, 71.0), (4.0, 4.0, 76.0)],
];

/// The length of one loop of either version, in seconds. Both versions are
/// exactly this long, so the player can switch between them mid-song
/// without losing the beat.
pub fn loop_seconds() -> f32 {
    CHORDS.len() as f32 * 8.0 * BEAT
}

/// Flagfall's theme. `intense` is the same song with the drums, the lead
/// and the energy turned up: played while a flag is out of its base.
pub fn flagfall_theme(intense: bool) -> Vec<u8> {
    let mut t = Track::new(loop_seconds());
    for (c, (bass, triad)) in CHORDS.iter().enumerate() {
        let bar0 = c as f32 * 8.0; // in beats
        for eighth in 0..16 {
            let beat = bar0 + eighth as f32 * 0.5;
            let at = beat * BEAT;
            // Bass: driving eighths, jumping the octave on the off-beats.
            let octave = if eighth % 2 == 1 { 12.0 } else { 0.0 };
            let bass_vol = if intense { 0.22 } else { 0.17 };
            t.note(at, BEAT * 0.42, bass + octave, Wave::Triangle, bass_vol, 0.8, 0.0);
            if intense {
                t.note(at, BEAT * 0.3, bass + octave, Wave::Pulse(0.5), 0.05, 0.4, 0.0);
            }
        }
        // Arpeggio: sixteenths up and down the chord.
        let pattern = [0, 1, 2, 1];
        for sixteenth in 0..32 {
            let at = (bar0 + sixteenth as f32 * 0.25) * BEAT;
            let n = triad[pattern[sixteenth % 4]] + if sixteenth % 8 >= 4 { 12.0 } else { 0.0 };
            let vol = if intense { 0.055 } else { 0.045 };
            t.note(at, BEAT * 0.2, n, Wave::Pulse(0.25), vol, 0.3, 0.0);
        }
        // Lead: the tune (intense), or a soft echo of it an octave down.
        for (beat, len, n) in MELODY[c] {
            let at = (bar0 + beat) * BEAT;
            if intense {
                t.note(at, len * BEAT * 0.92, *n, Wave::Pulse(0.5), 0.13, 0.7, 0.006);
                // A quiet detuned double for width.
                t.note(at, len * BEAT * 0.92, n + 0.08, Wave::Saw, 0.035, 0.6, 0.006);
            } else if c % 2 == 0 {
                t.note(at, len * BEAT * 0.9, n - 12.0, Wave::Triangle, 0.1, 0.6, 0.004);
            }
        }
        // Drums.
        for beat in 0..8 {
            let at = (bar0 + beat as f32) * BEAT;
            let seed = (c * 8 + beat) as u32 * 7919 + 13;
            if intense {
                t.kick(at, 0.5);
                if beat % 2 == 1 {
                    t.snare(at, 0.3, seed);
                }
                for k in 0..4 {
                    t.hat(at + k as f32 * BEAT / 4.0, if k % 2 == 0 { 0.16 } else { 0.1 }, false, seed + k);
                }
            } else {
                if matches!(beat, 0 | 4 | 6) {
                    t.kick(at, 0.35);
                }
                t.hat(at + BEAT / 2.0, 0.09, false, seed);
            }
        }
        // A snare fill into every fourth chord change.
        if intense && c % 4 == 3 {
            for k in 0..4 {
                t.snare((bar0 + 7.0 + k as f32 * 0.25) * BEAT, 0.18 + k as f32 * 0.05, 99 + k as u32);
            }
        }
    }
    t.wav()
}

// --- Brixo's theme ------------------------------------------------------------

/// Brixo's own theme, for menus: warm and hopeful in C major at 80 BPM.
/// Soft pads and bells to open, the tune on electric piano, a fuller
/// second half with a light beat, then a calm stretch that can loop.
/// C Em F G | Am F Dm7 Gsus4, one bar each: C is clearly home.
pub fn brixo_theme() -> Vec<u8> {
    const BEAT: f32 = 60.0 / 80.0;
    // (bass, chord notes)
    const CHORDS: [(f32, [f32; 4]); 8] = [
        (36.0, [60.0, 64.0, 67.0, 72.0]),
        (40.0, [59.0, 64.0, 67.0, 71.0]),
        (41.0, [60.0, 65.0, 69.0, 72.0]),
        (43.0, [59.0, 62.0, 67.0, 71.0]),
        (45.0, [60.0, 64.0, 69.0, 72.0]),
        (41.0, [60.0, 65.0, 69.0, 72.0]),
        (38.0, [60.0, 62.0, 65.0, 69.0]),
        (43.0, [60.0, 62.0, 67.0, 72.0]),
    ];
    // The Brixo tune, eight bars: (beat, beats long, note). It climbs like
    // stacking bricks, then settles.
    const TUNE: &[(f32, f32, f32)] = &[
        (0.0, 1.0, 76.0), (1.0, 0.5, 79.0), (1.5, 0.5, 81.0), (2.0, 1.5, 79.0), (3.5, 0.5, 76.0),
        (4.0, 1.0, 74.0), (5.0, 0.5, 76.0), (5.5, 0.5, 79.0), (6.0, 2.0, 71.0),
        (8.0, 1.0, 72.0), (9.0, 0.5, 76.0), (9.5, 0.5, 77.0), (10.0, 1.5, 76.0), (11.5, 0.5, 72.0),
        (12.0, 1.0, 74.0), (13.0, 1.0, 76.0), (14.0, 2.0, 79.0),
        (16.0, 1.0, 81.0), (17.0, 0.5, 79.0), (17.5, 0.5, 76.0), (18.0, 1.5, 79.0), (19.5, 0.5, 81.0),
        (20.0, 1.0, 83.0), (21.0, 1.0, 79.0), (22.0, 2.0, 76.0),
        (24.0, 1.0, 77.0), (25.0, 0.5, 76.0), (25.5, 0.5, 74.0), (26.0, 1.0, 72.0), (27.0, 1.0, 74.0),
        (28.0, 2.0, 74.0), (30.0, 0.5, 72.0), (30.5, 0.5, 74.0), (31.0, 1.0, 79.0),
    ];
    // Sections, in bars: intro 4, tune 8, full 8, calm 8.
    let (intro, tune, full, calm) = (0usize, 4usize, 12usize, 20usize);
    let bars = 28;
    let mut t = Track::new(bars as f32 * 4.0 * BEAT + 4.0);

    for bar in 0..bars {
        let (bass, chord) = CHORDS[bar % 8];
        let at = |beat: f32| (bar as f32 * 4.0 + beat) * BEAT;
        let section = if bar < tune { intro } else if bar < full { tune } else if bar < calm { full } else { calm };

        // The pad under everything (fuller in the intro, where it's alone).
        let pad_vol = if section == intro { 0.09 } else { 0.05 };
        for n in chord {
            t.pad(at(0.0), 4.0 * BEAT, n, pad_vol);
        }
        // The intro's second half brings in a low note, leading into the tune.
        if section == intro && bar >= 2 {
            t.bass(at(0.0), BEAT * 3.8, bass, 0.14);
        }
        // Bells: sparkles on top of each chord (busier in the full part).
        let top = chord[3] + 12.0;
        t.bell(at(0.0), top, match section { s if s == full => 0.08, s if s == intro => 0.09, _ => 0.06 });
        if section != tune {
            t.bell(at(2.0), chord[2] + 12.0, 0.045);
        }
        if section == full {
            t.bell(at(3.0), chord[1] + 24.0, 0.03);
        }

        if section == intro {
            continue;
        }

        // Bass.
        match section {
            s if s == full => {
                for e in 0..8 {
                    let n = if e % 4 == 3 { bass + 7.0 } else { bass };
                    t.bass(at(e as f32 * 0.5), BEAT * 0.4, n, 0.2);
                }
            }
            _ => {
                t.bass(at(0.0), BEAT * 1.8, bass, 0.2);
                t.bass(at(2.0), BEAT * 1.8, bass + if bar % 2 == 0 { 7.0 } else { 12.0 }, 0.16);
            }
        }

        // Electric piano: rolling eighths up and down the chord (quarters
        // in the calm part, so it breathes).
        let order = [0usize, 1, 2, 3, 2, 1, 2, 3];
        for (k, &i) in order.iter().enumerate() {
            let vol = match section { s if s == full => 0.07, s if s == calm => 0.045, _ => 0.06 };
            t.piano(at(k as f32 * 0.5), 0.45 * BEAT, chord[i], vol);
        }
        // Sparkle: a quiet, bright pluck running up the chord in sixteenths,
        // two octaves up (in the tune and full parts).
        if section == tune || section == full {
            for sx in 0..16 {
                let n = chord[[0, 1, 2, 3][sx % 4]] + 24.0;
                let vol = if section == full { 0.022 } else { 0.014 };
                t.note(at(sx as f32 * 0.25), 0.12 * BEAT, n, Wave::Pulse(0.25), vol, 0.2, 0.0);
            }
        }

        // A light beat in the full part: kick, snaps, a soft shaker.
        if section == full {
            let seed = bar as u32 * 97 + 3;
            t.kick(at(0.0), 0.32);
            t.kick(at(2.5), 0.22);
            t.snap(at(1.0), 0.3, seed);
            t.snap(at(3.0), 0.3, seed + 1);
            for sx in 0..8 {
                t.hat(at(sx as f32 * 0.5 + 0.25), 0.05, false, seed + 10 + sx);
            }
        }
    }

    // The tune: electric piano in the tune part; piano doubled by bells an
    // octave up in the full part; just its first half, softly, when calm.
    for &(beat, len, n) in TUNE {
        let tune_at = (tune as f32 * 4.0 + beat) * BEAT;
        t.piano(tune_at, len * BEAT, n, 0.16);
        let full_at = (full as f32 * 4.0 + beat) * BEAT;
        t.piano(full_at, len * BEAT, n, 0.17);
        t.bell(full_at, n + 12.0, 0.05);
        if beat < 16.0 {
            let calm_at = (calm as f32 * 4.0 + beat * 2.0) * BEAT;
            if calm_at < (calm as f32 + 8.0) * 4.0 * BEAT {
                t.bell(calm_at, n, 0.07);
            }
        }
    }
    // It ends on the home chord, ringing out.
    let end = bars as f32 * 4.0 * BEAT;
    for n in [60.0, 64.0, 67.0, 72.0] {
        t.pad(end, 2.0, n, 0.05);
        t.piano(end, 2.0, n, 0.06);
    }
    t.bell(end, 84.0, 0.07);
    t.bass(end, 2.0, 36.0, 0.2);

    t.reverb(0.35);
    t.wav()
}

// --- Spire Wars' music -------------------------------------------------------

/// Spire Wars' theme: a steady march in D minor, slower and heavier than
/// Flagfall's, for building spires and knocking the other team's down.
/// Dm Bb C Am | Dm Bb Gm A, two bars each: 16 bars, about 35 seconds.
pub fn spire_wars_theme() -> Vec<u8> {
    const BEAT: f32 = 60.0 / 110.0;
    const CHORDS: [(f32, [f32; 3]); 8] = [
        (38.0, [62.0, 65.0, 69.0]),
        (34.0, [62.0, 65.0, 70.0]),
        (36.0, [64.0, 67.0, 72.0]),
        (33.0, [64.0, 69.0, 72.0]),
        (38.0, [62.0, 65.0, 69.0]),
        (34.0, [62.0, 65.0, 70.0]),
        (31.0, [62.0, 67.0, 70.0]),
        (33.0, [61.0, 64.0, 69.0]),
    ];
    // The tune: (beat, beats long, note), a bold call that climbs, then answers.
    const TUNE: [&[(f32, f32, f32)]; 8] = [
        &[(0.0, 1.5, 74.0), (1.5, 0.5, 74.0), (2.0, 1.0, 77.0), (3.0, 1.0, 76.0), (4.0, 3.0, 74.0), (7.0, 1.0, 72.0)],
        &[(0.0, 1.5, 70.0), (1.5, 0.5, 72.0), (2.0, 1.0, 74.0), (3.0, 1.0, 77.0), (4.0, 4.0, 74.0)],
        &[(0.0, 1.5, 72.0), (1.5, 0.5, 72.0), (2.0, 1.0, 76.0), (3.0, 1.0, 79.0), (4.0, 2.0, 76.0), (6.0, 2.0, 72.0)],
        &[(0.0, 3.0, 76.0), (3.0, 1.0, 74.0), (4.0, 4.0, 72.0)],
        &[(0.0, 1.5, 74.0), (1.5, 0.5, 74.0), (2.0, 1.0, 77.0), (3.0, 1.0, 81.0), (4.0, 3.0, 79.0), (7.0, 1.0, 77.0)],
        &[(0.0, 1.5, 77.0), (1.5, 0.5, 79.0), (2.0, 1.0, 81.0), (3.0, 1.0, 82.0), (4.0, 4.0, 81.0)],
        &[(0.0, 1.5, 79.0), (1.5, 0.5, 77.0), (2.0, 1.0, 74.0), (3.0, 1.0, 70.0), (4.0, 2.0, 74.0), (6.0, 2.0, 77.0)],
        &[(0.0, 2.0, 76.0), (2.0, 1.0, 73.0), (3.0, 1.0, 76.0), (4.0, 4.0, 81.0)],
    ];
    let mut t = Track::new(CHORDS.len() as f32 * 8.0 * BEAT);
    for (c, (bass, triad)) in CHORDS.iter().enumerate() {
        let bar0 = c as f32 * 8.0;
        for beat in 0..8 {
            let at = (bar0 + beat as f32) * BEAT;
            // Bass: marching quarters, root and fifth.
            let n = if beat % 2 == 0 { *bass } else { bass + 7.0 };
            t.note(at, BEAT * 0.7, n, Wave::Triangle, 0.2, 0.85, 0.0);
            // Chords: short stabs on every beat, like a brass section.
            for (k, note) in triad.iter().enumerate() {
                t.note(at, BEAT * 0.3, *note - 12.0, Wave::Pulse(0.125), 0.035, 0.5, 0.0);
                if beat % 4 == 0 {
                    t.note(at + k as f32 * 0.02, BEAT * 1.6, *note, Wave::Saw, 0.018, 0.7, 0.003);
                }
            }
            // Drums: a march. Kick on 1 and 3, snare on 2 and 4.
            let seed = (c * 8 + beat) as u32 * 6151 + 7;
            if beat % 2 == 0 {
                t.kick(at, 0.45);
            } else {
                t.snare(at, 0.26, seed);
            }
            t.hat(at + BEAT / 2.0, 0.07, false, seed + 1);
        }
        // A marching roll into each second bar.
        for k in 0..4 {
            t.snare((bar0 + 3.0 + k as f32 * 0.25) * BEAT, 0.08 + k as f32 * 0.03, 500 + (c * 4 + k) as u32);
        }
        for (beat, len, n) in TUNE[c] {
            let at = (bar0 + beat) * BEAT;
            t.note(at, len * BEAT * 0.9, *n, Wave::Pulse(0.5), 0.1, 0.75, 0.005);
            t.note(at, len * BEAT * 0.9, n - 12.0, Wave::Triangle, 0.06, 0.7, 0.0);
        }
    }
    t.wav()
}

// --- Flagfall's sound effects --------------------------------------------

/// Your flag's been taken: three urgent rising minor notes.
pub fn flag_taken() -> Vec<u8> {
    let mut t = Track::new(1.0);
    for (i, n) in [69.0, 72.0, 75.0].iter().enumerate() {
        let at = i as f32 * 0.11;
        t.note(at, 0.1, *n, Wave::Pulse(0.5), 0.3, 0.8, 0.0);
        t.note(at, 0.1, n - 12.0, Wave::Saw, 0.15, 0.8, 0.0);
    }
    t.note(0.33, 0.45, 81.0, Wave::Pulse(0.5), 0.3, 0.7, 0.01);
    t.note(0.33, 0.45, 69.0, Wave::Saw, 0.15, 0.7, 0.0);
    t.wav()
}

/// A flag's back home: a bright falling-then-resolving chime.
pub fn flag_returned() -> Vec<u8> {
    let mut t = Track::new(0.9);
    for (i, n) in [84.0, 79.0, 76.0, 84.0].iter().enumerate() {
        t.note(i as f32 * 0.09, 0.3, *n, Wave::Triangle, 0.3, 0.3, 0.0);
    }
    t.wav()
}

/// A capture: a brass-like major fanfare with a drum hit.
pub fn capture_fanfare() -> Vec<u8> {
    let mut t = Track::new(2.2);
    let notes = [(0.0, 0.14, 67.0), (0.14, 0.14, 72.0), (0.28, 0.14, 76.0), (0.42, 0.6, 79.0), (1.02, 0.14, 76.0), (1.16, 0.9, 84.0)];
    for (at, len, n) in notes {
        t.note(at, len, n, Wave::Saw, 0.16, 0.8, 0.005);
        t.note(at, len, n, Wave::Pulse(0.5), 0.1, 0.8, 0.005);
        t.note(at, len, n - 12.0, Wave::Pulse(0.5), 0.07, 0.8, 0.0);
    }
    for at in [0.0, 0.42, 1.16] {
        t.kick(at, 0.5);
        t.snare(at, 0.25, 7);
    }
    t.hat(1.16, 0.3, true, 11);
    t.wav()
}

/// The start of a match: a short rising horn call.
pub fn horn() -> Vec<u8> {
    let mut t = Track::new(1.4);
    for (at, len, n) in [(0.0, 0.16, 60.0), (0.16, 0.16, 64.0), (0.32, 0.16, 67.0), (0.48, 0.7, 72.0)] {
        t.note(at, len, n, Wave::Saw, 0.18, 0.85, 0.004);
        t.note(at, len, n - 12.0, Wave::Pulse(0.5), 0.08, 0.85, 0.0);
    }
    t.wav()
}

/// Match over: a longer victory phrase.
pub fn victory() -> Vec<u8> {
    let mut t = Track::new(3.2);
    let notes = [
        (0.0, 0.2, 72.0), (0.2, 0.2, 72.0), (0.4, 0.2, 72.0), (0.6, 0.6, 76.0),
        (1.2, 0.3, 74.0), (1.5, 0.3, 76.0), (1.8, 1.2, 79.0),
    ];
    for (at, len, n) in notes {
        t.note(at, len, n, Wave::Saw, 0.15, 0.8, 0.006);
        t.note(at, len, n + 0.07, Wave::Pulse(0.5), 0.09, 0.8, 0.006);
    }
    for (at, n) in [(0.0, 48.0), (0.6, 52.0), (1.2, 50.0), (1.8, 55.0)] {
        t.note(at, 0.55, n, Wave::Triangle, 0.25, 0.9, 0.0);
    }
    for k in 0..6 {
        t.kick(k as f32 * 0.3, 0.4);
    }
    t.hat(1.8, 0.3, true, 5);
    t.wav()
}

// --- Brickport Speedway's music -------------------------------------------

/// The lobby: a laid-back groove for hanging round the pits. Soft electric
/// piano on jazzy chords, a round bass, snaps and swung hats. 8 bars at 100
/// BPM, about 19 seconds, then it loops.
pub fn speedway_lobby() -> Vec<u8> {
    const BEAT: f32 = 60.0 / 100.0;
    // (bass, chord) a bar each: Fmaj7 Em7 Dm7 Cmaj7 Bbmaj7 Am7 Gm7 C7sus.
    const BARS: [(f32, [f32; 4]); 8] = [
        (41.0, [57.0, 60.0, 64.0, 65.0]),
        (40.0, [55.0, 59.0, 62.0, 64.0]),
        (38.0, [53.0, 57.0, 60.0, 62.0]),
        (36.0, [52.0, 55.0, 59.0, 64.0]),
        (34.0, [53.0, 57.0, 60.0, 62.0]),
        (33.0, [52.0, 55.0, 57.0, 60.0]),
        (31.0, [50.0, 53.0, 55.0, 58.0]),
        (36.0, [53.0, 55.0, 58.0, 60.0]),
    ];
    // A little tune on top, now and then: (bar, beat, beats, note).
    const TUNE: [(usize, f32, f32, f32); 14] = [
        (2, 0.0, 0.5, 72.0), (2, 0.5, 0.5, 74.0), (2, 1.0, 1.5, 76.0), (2, 2.5, 1.5, 72.0),
        (3, 0.0, 2.0, 71.0), (3, 2.5, 1.0, 67.0),
        (6, 0.0, 0.5, 69.0), (6, 0.5, 0.5, 72.0), (6, 1.0, 1.0, 74.0), (6, 2.0, 2.0, 77.0),
        (7, 0.0, 1.0, 76.0), (7, 1.0, 1.0, 74.0), (7, 2.0, 1.0, 72.0), (7, 3.0, 1.0, 70.0),
    ];
    let mut t = Track::new(BARS.len() as f32 * 4.0 * BEAT);
    let swing = BEAT * 0.12;
    for (b, (bass, chord)) in BARS.iter().enumerate() {
        let bar = b as f32 * 4.0 * BEAT;
        // Piano: a chord on 1, pushed again on the and of 2.
        for (k, n) in chord.iter().enumerate() {
            t.piano(bar + k as f32 * 0.012, BEAT * 1.4, *n, 0.05);
            t.piano(bar + 1.5 * BEAT + swing + k as f32 * 0.012, BEAT * 0.9, *n, 0.035);
        }
        t.bass(bar, BEAT * 1.3, *bass, 0.2);
        t.bass(bar + 1.5 * BEAT + swing, BEAT * 0.4, *bass, 0.14);
        t.bass(bar + 2.5 * BEAT + swing, BEAT * 1.2, bass + 7.0, 0.16);
        for beat in 0..4 {
            let at = bar + beat as f32 * BEAT;
            let seed = (b * 4 + beat) as u32 * 7919 + 3;
            if beat == 0 {
                t.kick(at, 0.32);
            }
            if beat == 2 {
                t.kick(at + BEAT * 0.5 + swing, 0.22);
            }
            if beat % 2 == 1 {
                t.snap(at, 0.22, seed);
            }
            t.hat(at, 0.035, false, seed + 1);
            t.hat(at + BEAT * 0.5 + swing, 0.05, false, seed + 2);
        }
    }
    for (bar, beat, len, n) in TUNE {
        let at = (bar as f32 * 4.0 + beat) * BEAT;
        t.bell(at, n, 0.05);
        t.piano(at, len * BEAT, n, 0.03);
    }
    t.reverb(0.25);
    t.wav()
}

/// Before a race, while the camera flies round the track: a swell of pads
/// and rising bells over drums that build, landing on one big hit and
/// ringing out (about 12.5 seconds, played once).
pub fn speedway_intro() -> Vec<u8> {
    let mut t = Track::new(12.5);
    // C, A minor, F, G: two and a half seconds each, then the big C.
    let chords: [(f32, [f32; 3]); 4] = [(36.0, [60.0, 64.0, 67.0]), (33.0, [60.0, 64.0, 69.0]), (29.0, [60.0, 65.0, 69.0]), (31.0, [62.0, 67.0, 71.0])];
    for (c, (bass, triad)) in chords.iter().enumerate() {
        let at = c as f32 * 2.5;
        for n in triad {
            t.pad(at, 2.4, *n, 0.05);
        }
        t.bass(at, 2.4, *bass + 12.0, 0.14);
        // Bells climbing through the chord, faster each time.
        let steps = 4 + c * 2;
        for k in 0..steps {
            let n = triad[k % 3] + 12.0 * (k / 3) as f32 + 12.0;
            t.bell(at + k as f32 * (2.4 / steps as f32), n, 0.045);
        }
        // Timpani-like thumps, closer together as it builds.
        let hits = 2 + c * 2;
        for k in 0..hits {
            t.kick(at + k as f32 * (2.5 / hits as f32), 0.2 + 0.07 * c as f32);
        }
    }
    // A snare roll rising into the hit.
    for k in 0..32 {
        let at = 8.0 + k as f32 * (2.0 / 32.0);
        t.snare(at, 0.03 + 0.2 * (k as f32 / 32.0), 900 + k);
    }
    // The hit: a brass chord, a deep drum and a cymbal, ringing out.
    for n in [48.0f32, 55.0, 60.0, 64.0, 67.0, 72.0] {
        t.note(10.0, 1.6, n, Wave::Saw, 0.035, 0.6, 0.004);
        t.pad(10.0, 1.5, n, 0.04);
    }
    t.kick(10.0, 0.7);
    t.snare(10.0, 0.3, 77);
    t.hat(10.0, 0.5, true, 91);
    t.bell(10.0, 84.0, 0.08);
    t.reverb(0.35);
    t.wav()
}

/// Racing: 150 BPM, driving drums and a bouncing octave bass under a bright
/// arpeggio, in A minor. `final_lap` is the same song turned up (fuller
/// drums, the tune doubled an octave up) at the same length, so switching
/// keeps the beat. 16 bars, 25.6 seconds, then it loops.
pub fn speedway_race(final_lap: bool) -> Vec<u8> {
    const BEAT: f32 = 60.0 / 150.0;
    // Two bars each: Am F C G Am F G E.
    const CHORDS: [(f32, [f32; 3]); 8] = [
        (45.0, [57.0, 60.0, 64.0]),
        (41.0, [57.0, 60.0, 65.0]),
        (48.0, [55.0, 60.0, 64.0]),
        (43.0, [55.0, 59.0, 62.0]),
        (45.0, [57.0, 60.0, 64.0]),
        (41.0, [57.0, 60.0, 65.0]),
        (43.0, [55.0, 59.0, 62.0]),
        (40.0, [56.0, 59.0, 64.0]),
    ];
    const TUNE: [&[(f32, f32, f32)]; 8] = [
        &[(0.0, 1.0, 69.0), (1.0, 0.5, 72.0), (1.5, 0.5, 74.0), (2.0, 1.5, 76.0), (3.5, 0.5, 74.0), (4.0, 1.0, 72.0), (5.0, 1.0, 69.0), (6.0, 2.0, 67.0)],
        &[(0.0, 1.0, 69.0), (1.0, 0.5, 72.0), (1.5, 0.5, 74.0), (2.0, 2.0, 77.0), (4.0, 1.0, 76.0), (5.0, 1.0, 74.0), (6.0, 2.0, 72.0)],
        &[(0.0, 1.5, 76.0), (1.5, 0.5, 74.0), (2.0, 1.0, 72.0), (3.0, 1.0, 74.0), (4.0, 3.0, 79.0), (7.0, 1.0, 76.0)],
        &[(0.0, 1.0, 74.0), (1.0, 1.0, 71.0), (2.0, 2.0, 74.0), (4.0, 1.0, 79.0), (5.0, 1.0, 78.0), (6.0, 2.0, 74.0)],
        &[(0.0, 1.0, 69.0), (1.0, 0.5, 72.0), (1.5, 0.5, 74.0), (2.0, 1.5, 76.0), (3.5, 0.5, 79.0), (4.0, 1.0, 76.0), (5.0, 1.0, 79.0), (6.0, 2.0, 81.0)],
        &[(0.0, 1.0, 77.0), (1.0, 1.0, 76.0), (2.0, 1.0, 74.0), (3.0, 1.0, 72.0), (4.0, 4.0, 77.0)],
        &[(0.0, 1.0, 79.0), (1.0, 1.0, 77.0), (2.0, 1.0, 76.0), (3.0, 1.0, 74.0), (4.0, 2.0, 76.0), (6.0, 2.0, 74.0)],
        &[(0.0, 1.0, 76.0), (1.0, 1.0, 80.0), (2.0, 2.0, 83.0), (4.0, 4.0, 80.0)],
    ];
    let mut t = Track::new(CHORDS.len() as f32 * 8.0 * BEAT);
    for (c, (root, triad)) in CHORDS.iter().enumerate() {
        let bar0 = c as f32 * 8.0;
        for beat in 0..8 {
            let at = (bar0 + beat as f32) * BEAT;
            let seed = (c * 8 + beat) as u32 * 4099 + 13;
            // Four on the floor, snare on 2 and 4.
            t.kick(at, 0.42);
            if beat % 2 == 1 {
                t.snare(at, 0.27, seed);
            }
            t.hat(at + BEAT * 0.5, if final_lap { 0.12 } else { 0.08 }, final_lap, seed + 1);
            if final_lap {
                t.hat(at + BEAT * 0.25, 0.04, false, seed + 2);
                t.hat(at + BEAT * 0.75, 0.04, false, seed + 3);
            }
            // The bass bounces root and octave in eighths.
            for half in 0..2 {
                let n = if half == 0 { *root - 12.0 } else { *root };
                t.note(at + half as f32 * BEAT * 0.5, BEAT * 0.4, n, Wave::Saw, 0.09, 0.4, 0.0);
                t.bass(at + half as f32 * BEAT * 0.5, BEAT * 0.4, root - 12.0, 0.1);
            }
            // The arpeggio: sixteenths up and down the chord.
            for q in 0..4 {
                let step = (beat * 4 + q) % 6;
                let k = if step < 3 { step } else { 5 - step };
                let lift = if final_lap { 24.0 } else { 12.0 };
                t.note(at + q as f32 * BEAT * 0.25, BEAT * 0.2, triad[k] + lift, Wave::Pulse(0.25), 0.025, 0.3, 0.0);
            }
        }
        // A fill into every other chord.
        if c % 2 == 1 || final_lap {
            for k in 0..4 {
                t.snare((bar0 + 7.0 + k as f32 * 0.25) * BEAT, 0.08 + k as f32 * 0.04, 300 + (c * 4 + k) as u32);
            }
        }
        // The tune: second half only, or all through on the final lap.
        if final_lap || c >= 4 {
            for (beat, len, n) in TUNE[c] {
                let at = (bar0 + beat) * BEAT;
                t.note(at, len * BEAT * 0.92, *n, Wave::Pulse(0.5), 0.06, 0.75, 0.006);
                t.note(at, len * BEAT * 0.92, *n, Wave::Saw, 0.035, 0.75, 0.006);
                if final_lap {
                    t.note(at, len * BEAT * 0.92, n + 12.0, Wave::Pulse(0.25), 0.03, 0.7, 0.006);
                }
            }
        } else {
            // First half: pad chords underneath instead.
            for n in triad {
                t.pad(bar0 * BEAT, 8.0 * BEAT - 0.2, *n, 0.03);
            }
        }
    }
    t.wav()
}

/// One red light: a short low beep.
pub fn light_beep() -> Vec<u8> {
    let mut t = Track::new(0.5);
    t.note(0.0, 0.22, 67.0, Wave::Pulse(0.5), 0.2, 0.9, 0.0);
    t.note(0.0, 0.22, 55.0, Wave::Triangle, 0.15, 0.9, 0.0);
    t.wav()
}

/// Lights out: a high bright beep with a hit.
pub fn go_beep() -> Vec<u8> {
    let mut t = Track::new(1.2);
    for n in [79.0f32, 86.0] {
        t.note(0.0, 0.7, n, Wave::Pulse(0.5), 0.14, 0.9, 0.0);
    }
    t.note(0.0, 0.7, 67.0, Wave::Triangle, 0.15, 0.9, 0.0);
    t.kick(0.0, 0.6);
    t.hat(0.0, 0.35, true, 3);
    t.wav()
}

/// The final lap: a quick climbing call.
pub fn final_lap_sting() -> Vec<u8> {
    let mut t = Track::new(1.6);
    for (k, n) in [69.0f32, 72.0, 76.0, 81.0].iter().enumerate() {
        let at = k as f32 * 0.11;
        let len = if k == 3 { 0.8 } else { 0.1 };
        t.note(at, len, *n, Wave::Saw, 0.14, 0.8, 0.004);
        t.note(at, len, n - 12.0, Wave::Pulse(0.5), 0.08, 0.8, 0.0);
    }
    t.kick(0.33, 0.5);
    t.snare(0.33, 0.3, 21);
    t.hat(0.33, 0.3, true, 22);
    t.wav()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_race_music_switches_to_the_final_lap_on_the_beat() {
        let race = speedway_race(false);
        let last = speedway_race(true);
        assert_eq!(race.len(), last.len());
        for wav in [race, last, speedway_lobby(), speedway_intro()] {
            let samples: Vec<i16> = wav[44..].chunks(2).map(|c| i16::from_le_bytes([c[0], c[1]])).collect();
            assert!(samples.iter().filter(|s| s.unsigned_abs() > 2000).count() > samples.len() / 20, "audible");
            assert!(samples.iter().filter(|s| s.unsigned_abs() >= 32000).count() < samples.len() / 200, "not clipped");
        }
    }

    #[test]
    fn both_versions_of_the_theme_loop_at_the_same_length() {
        let calm = flagfall_theme(false);
        let intense = flagfall_theme(true);
        assert_eq!(calm.len(), intense.len(), "same length, so switching keeps the beat");
        assert_eq!(&calm[..4], b"RIFF");
        // Not silent, and not clipping hard.
        let samples: Vec<i16> = intense[44..].chunks(2).map(|c| i16::from_le_bytes([c[0], c[1]])).collect();
        let loud = samples.iter().filter(|s| s.unsigned_abs() > 3000).count();
        assert!(loud > samples.len() / 10, "audible");
        assert!(samples.iter().filter(|s| s.unsigned_abs() >= 32000).count() < samples.len() / 200, "not clipped");
    }
}
