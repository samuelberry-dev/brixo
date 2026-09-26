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

#[cfg(test)]
mod tests {
    use super::*;

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
