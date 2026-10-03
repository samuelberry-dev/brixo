//! Film mode (BRIXO_FILM=shot.json): renders a shot frame by frame for a
//! trailer. The game moves exactly 1/fps of a second (times time_scale)
//! each frame however long drawing takes, so a slow computer films as
//! smoothly as a fast one; every frame is saved; the camera flies a path
//! set in the shot; the sounds the game played are logged with their times
//! so they can be mixed in afterwards. The Player quits when the shot's
//! done.
//!
//! A shot:
//! ```json
//! {"fps": 60, "seconds": 4, "warmup": 3, "time_scale": 1, "hud": false,
//!  "out": "footage/spire",
//!  "keys": [{"t": 0, "at": [0, 20, 40], "look": [0, 5, 0]},
//!           {"t": 4, "at": [30, 12, 20], "look": [0, 5, 0], "fov": 60,
//!            "rel": "Bot2", "turn": true}]}
//! ```
//! `out` gets `frames.raw` (BGRA or RGBA frames, one after another, as
//! `info.json` says) and `sounds.jsonl`. Between keys the camera glides
//! (Catmull-Rom). With `rel`, `at` and `look` are offsets from that
//! object's position (a player, part or kart by name), turned with it when
//! `turn` is set, so the camera can ride along.

use std::io::Write;
use std::path::PathBuf;

use brixo_core::DataModel;
use glam::Vec3;
use serde::Deserialize;

#[derive(Deserialize, Clone)]
pub struct Key {
    pub t: f32,
    pub at: [f32; 3],
    pub look: [f32; 3],
    #[serde(default)]
    pub fov: Option<f32>,
    #[serde(default)]
    pub rel: Option<String>,
    #[serde(default)]
    pub turn: bool,
    /// The camera stays put (at) but keeps its eye on this object (look is
    /// an offset from it): a trackside camera panning with a kart.
    #[serde(default)]
    pub look_at: Option<String>,
}

#[derive(Deserialize)]
struct Shot {
    #[serde(default = "sixty")]
    fps: f32,
    seconds: f32,
    #[serde(default)]
    warmup: f32,
    #[serde(default = "one")]
    time_scale: f32,
    #[serde(default)]
    hud: bool,
    out: PathBuf,
    keys: Vec<Key>,
    /// What your character does, by game time (warm-up included): an
    /// action like the BRIXO_ACTION_FILE ones ("equip 2", "use", "jump",
    /// "say hi"), or walking toward a point ("goto": [x, z]; "stop": true).
    #[serde(default)]
    script: Vec<Step>,
}

#[derive(Deserialize, Clone)]
struct Step {
    t: f32,
    #[serde(default)]
    act: Option<String>,
    #[serde(default)]
    goto: Option<[f32; 2]>,
    #[serde(default)]
    stop: bool,
}

fn sixty() -> f32 {
    60.0
}
fn one() -> f32 {
    1.0
}

pub struct Film {
    shot: Shot,
    /// Game seconds run so far.
    pub sim: f32,
    /// Frames saved so far.
    pub frames: u32,
    raw: Option<Box<dyn Write>>,
    /// ffmpeg, when it's there: frames go straight into `clip.mp4`.
    encoder: Option<std::process::Child>,
    sounds: std::fs::File,
    /// Custom sounds already written out, by id.
    dumped: std::collections::HashSet<u64>,
    /// The next step of the script, and where your character's walking.
    next: usize,
    walk: Option<glam::Vec2>,
    track_file: Option<std::fs::File>,
}

pub fn load() -> Option<Film> {
    let path = std::env::var_os("BRIXO_FILM")?;
    let text = std::fs::read_to_string(&path).map_err(|e| eprintln!("film: can't read {path:?}: {e}")).ok()?;
    let shot: Shot = serde_json::from_str(&text).map_err(|e| eprintln!("film: bad shot: {e}")).ok()?;
    std::fs::create_dir_all(&shot.out).ok()?;
    let sounds = std::fs::File::create(shot.out.join("sounds.jsonl")).ok()?;
    Some(Film { shot, sim: 0.0, frames: 0, raw: None, encoder: None, sounds, dumped: Default::default(), next: 0, walk: None, track_file: None })
}

impl Film {
    fn total(&self) -> u32 {
        (self.shot.seconds * self.shot.fps).round() as u32
    }

    /// Still warming up (the game runs, nothing's saved)?
    pub fn warming(&self) -> bool {
        self.sim < self.shot.warmup
    }

    pub fn done(&self) -> bool {
        self.frames >= self.total()
    }

    pub fn hud(&self) -> bool {
        self.shot.hud
    }

    /// Game seconds to run before the next frame. Warm-up runs in bigger
    /// steps (nothing's filmed).
    pub fn dt(&self) -> f32 {
        if self.warming() {
            (self.shot.warmup - self.sim).clamp(0.0, 0.05).max(1.0 / self.shot.fps)
        } else {
            self.shot.time_scale / self.shot.fps
        }
    }

    /// The script's actions that are due now.
    pub fn actions(&mut self) -> Vec<String> {
        let mut out = Vec::new();
        while let Some(step) = self.shot.script.get(self.next).filter(|s| s.t <= self.sim).cloned() {
            self.next += 1;
            if let Some(a) = step.act {
                out.push(a);
            }
            if let Some([x, z]) = step.goto {
                self.walk = Some(glam::Vec2::new(x, z));
            }
            if step.stop {
                self.walk = None;
            }
        }
        out
    }

    /// Where the script has your character walking.
    pub fn walk(&self) -> Option<glam::Vec2> {
        self.walk
    }

    pub fn advance(&mut self, dt: f32) {
        self.sim += dt;
    }

    /// Seconds into the filmed part (the keys' clock).
    fn clip_time(&self) -> f32 {
        self.frames as f32 / self.shot.fps
    }

    /// Where the camera is, looks and how wide, now.
    /// Notes where the first key's `rel` object is, each filmed frame
    /// (track.jsonl), for planning shots.
    pub fn track(&mut self, world: &DataModel) {
        if self.warming() {
            return;
        }
        let Some(name) = self.shot.keys.iter().find_map(|k| k.rel.clone().or(k.look_at.clone())) else { return };
        if let Some((p, yaw)) = anchor(world, &name) {
            let line = serde_json::json!({"t": self.clip_time(), "sim": self.sim, "at": [p.x, p.y, p.z], "yaw": yaw});
            let _ = writeln!(self.track_file.get_or_insert_with(|| std::fs::File::create(self.shot.out.join("track.jsonl")).unwrap()), "{line}");
        }
    }

    pub fn camera(&self, world: &DataModel) -> Option<(Vec3, Vec3, Option<f32>)> {
        let keys = &self.shot.keys;
        if keys.is_empty() {
            return None;
        }
        let t = self.clip_time();
        let place = |k: &Key| -> (Vec3, Vec3) {
            let (at, look) = (Vec3::from(k.at), Vec3::from(k.look));
            match k.rel.as_deref().and_then(|n| anchor(world, n)) {
                Some((pos, yaw)) => {
                    let turn = if k.turn { glam::Quat::from_rotation_y(yaw) } else { glam::Quat::IDENTITY };
                    (pos + turn * at, pos + turn * look)
                }
                None => match k.look_at.as_deref().and_then(|n| anchor(world, n)) {
                    Some((pos, _)) => (at, pos + look),
                    None => (at, look),
                },
            }
        };
        let i = keys.iter().rposition(|k| k.t <= t).unwrap_or(0);
        let j = (i + 1).min(keys.len() - 1);
        let (a, b) = (&keys[i], &keys[j]);
        let u = if j == i || b.t <= a.t { 0.0 } else { ((t - a.t) / (b.t - a.t)).clamp(0.0, 1.0) };
        // Ease in and out of each stretch, and glide through the keys.
        let u = u * u * (3.0 - 2.0 * u);
        let p0 = place(&keys[i.saturating_sub(1)]);
        let (p1, p2) = (place(a), place(b));
        let p3 = place(&keys[(j + 1).min(keys.len() - 1)]);
        let cr = |a: Vec3, b: Vec3, c: Vec3, d: Vec3| {
            0.5 * (2.0 * b + (c - a) * u + (2.0 * a - 5.0 * b + 4.0 * c - d) * u * u + (3.0 * b - a - 3.0 * c + d) * u * u * u)
        };
        let at = cr(p0.0, p1.0, p2.0, p3.0);
        let look = cr(p0.1, p1.1, p2.1, p3.1);
        let fov = match (a.fov, b.fov) {
            (Some(x), Some(y)) => Some(x + (y - x) * u),
            (x, y) => x.or(y),
        };
        Some((at, look, fov))
    }

    /// Keeps the game's output (script errors, prints) in log.txt.
    pub fn log(&mut self, lines: &[brixo_runtime::LogLine]) {
        if lines.is_empty() {
            return;
        }
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(self.shot.out.join("log.txt")) {
            for l in lines {
                let _ = writeln!(f, "{:.2} {}{}: {}", self.sim, if l.is_error { "ERROR " } else { "" }, l.source, l.text);
            }
        }
    }

    /// Notes a sound the game played (at the current time).
    pub fn sound(&mut self, cue: &brixo_runtime::Cue, world: &DataModel) {
        if self.warming() {
            return;
        }
        let t = self.clip_time();
        let (name, at) = match cue {
            brixo_runtime::Cue::Sound(n) => (n.clone(), None),
            brixo_runtime::Cue::SoundAt(n, at) => (n.clone(), Some(at.object.and_then(|o| brixo_core::object_position(world, o)).unwrap_or(at.position))),
            brixo_runtime::Cue::Music(n) => (format!("music:{}", n.clone().unwrap_or_default()), None),
        };
        // Sounds made in the game (a Sound object): keep a copy to mix.
        if let Some(id) = name.trim_start_matches("music:").strip_prefix('#').and_then(|i| i.parse::<u64>().ok()) {
            if self.dumped.insert(id) {
                if let Some((bytes, _)) = brixo_client::world_sound(world, id) {
                    let _ = std::fs::write(self.shot.out.join(format!("sound-{id}.bin")), bytes.as_slice());
                }
            }
        }
        let at = at.map(|p| [p.x, p.y, p.z]);
        let line = serde_json::json!({"t": t, "name": name, "at": at});
        let _ = writeln!(self.sounds, "{line}");
    }

    /// Saves one frame's pixels (rows of `width` pixels, tightly packed).
    pub fn save(&mut self, pixels: &[u8], width: u32, height: u32, bgra: bool) {
        if self.raw.is_none() {
            let format = if bgra { "bgra" } else { "rgba" };
            let info = serde_json::json!({"width": width, "height": height, "fps": self.shot.fps, "format": format});
            let _ = std::fs::write(self.shot.out.join("info.json"), info.to_string());
            // Straight into an MP4 if ffmpeg's there (scaled to 1080p: film
            // bigger for smoother edges), else raw frames.
            let child = std::process::Command::new("ffmpeg")
                .args(["-loglevel", "error", "-y", "-f", "rawvideo", "-pix_fmt", format, "-s", &format!("{width}x{height}")])
                .args(["-r", &self.shot.fps.to_string(), "-i", "-", "-vf", "scale=1920:1080:flags=lanczos"])
                .args(["-c:v", "libx264", "-crf", "12", "-preset", "medium", "-pix_fmt", "yuv420p"])
                .arg(self.shot.out.join("clip.mp4"))
                .stdin(std::process::Stdio::piped())
                .spawn();
            match child {
                Ok(mut c) => {
                    self.raw = c.stdin.take().map(|w| Box::new(w) as Box<dyn Write>);
                    self.encoder = Some(c);
                }
                Err(_) => self.raw = std::fs::File::create(self.shot.out.join("frames.raw")).ok().map(|f| Box::new(f) as Box<dyn Write>),
            }
        }
        if let Some(f) = &mut self.raw {
            let _ = f.write_all(pixels);
        }
        self.frames += 1;
        if self.frames % 30 == 0 {
            eprintln!("film: {} / {} frames", self.frames, self.total());
        }
        if self.done() {
            // Finish the file before the Player quits.
            self.raw = None;
            if let Some(mut c) = self.encoder.take() {
                let _ = c.wait();
            }
        }
    }
}

/// Where an object is and which way it faces (a player, a part, a kart).
fn anchor(world: &DataModel, name: &str) -> Option<(Vec3, f32)> {
    let id = world.find_first(name)?;
    if let Some(p) = world.player(id) {
        let b = p.body.position;
        return Some((Vec3::new(b.x, b.y, b.z), p.body.rotation.y.to_radians()));
    }
    let part = brixo_core::kart_chassis(world, id).or_else(|| world.part(id).map(|_| id)).or_else(|| world.parts_under(id).into_iter().next())?;
    let p = world.part(part)?;
    Some((Vec3::new(p.position.x, p.position.y, p.position.z), p.rotation.y.to_radians()))
}

/// Copies a rendered frame out of the GPU (blocking).
pub fn read_back(device: &wgpu::Device, queue: &wgpu::Queue, texture: &wgpu::Texture) -> Vec<u8> {
    let (w, h) = (texture.width(), texture.height());
    let row = w * 4;
    let padded = row.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("film frame"),
        size: (padded * h) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("film copy") });
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(padded), rows_per_image: Some(h) },
        },
        wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
    );
    queue.submit(Some(encoder.finish()));
    let slice = buffer.slice(..);
    slice.map_async(wgpu::MapMode::Read, |_| {});
    device.poll(wgpu::Maintain::Wait);
    let data = slice.get_mapped_range();
    let mut out = Vec::with_capacity((row * h) as usize);
    for y in 0..h {
        let start = (y * padded) as usize;
        out.extend_from_slice(&data[start..start + row as usize]);
    }
    out
}
