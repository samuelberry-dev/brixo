//! Helpers for editing in the studio: script colouring and autocomplete,
//! and dragging parts over surfaces. Kept apart from the UI so they can be
//! tested on their own.

use std::collections::HashSet;

use brixo_core::{DataModel, InstanceId, PartProps};
use glam::{Quat, Vec3};

// --- Script colouring ---------------------------------------------------------

/// Rovik's keywords (the lexer's list, plus `seconds` from `every N seconds`).
pub const KEYWORDS: &[&str] = &[
    "fn", "end", "if", "then", "elseif", "else", "while", "do", "for", "in", "return", "break", "continue", "and",
    "or", "not", "true", "false", "nil", "on", "every", "seconds",
];

/// Functions Brixo gives scripts, on top of Rovik's own built-ins.
pub const BRIXO_FUNCTIONS: &[&str] =
    &["find", "destroy", "clone", "time", "players", "create", "play_sound", "play_music", "stop_music", "explode"];

/// Fields scripts use on objects (after a `.`).
pub const FIELDS: &[&str] = &[
    "name", "parent", "children", "class", "position", "size", "rotation", "color", "anchored", "can_collide",
    "transparency", "material", "shape", "velocity", "floating", "bounce", "health", "max_health", "walk_speed",
    "jump_power", "face", "look", "swinging", "equipped", "team", "text", "visible", "text_color", "text_size",
    "background", "background_color", "attached_to", "volume", "shirt_color", "pants_color", "skin_color", "x", "y",
    "z", "r", "g", "b",
];

/// What a stretch of script text is, for colouring.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Plain,
    Keyword,
    Function,
    Event,
    Field,
    Str,
    Number,
    Comment,
}

fn builtins() -> impl Iterator<Item = &'static str> {
    rovik::value::Builtin::ALL.iter().map(|(n, _)| *n).chain(BRIXO_FUNCTIONS.iter().copied())
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Splits a script into coloured stretches (byte ranges), each with its kind.
/// Every byte belongs to exactly one stretch, in order.
pub fn tokens(src: &str) -> Vec<(std::ops::Range<usize>, Kind)> {
    let mut out: Vec<(std::ops::Range<usize>, Kind)> = Vec::new();
    let chars: Vec<(usize, char)> = src.char_indices().collect();
    let end_of = |i: usize| chars.get(i).map(|c| c.0).unwrap_or(src.len());
    let mut i = 0;
    let mut after_dot = false;
    let mut after_on = false;
    while i < chars.len() {
        let (start, c) = chars[i];
        let (kind, next) = if c == '-' && chars.get(i + 1).map(|c| c.1) == Some('-') {
            let mut j = i;
            while j < chars.len() && chars[j].1 != '\n' {
                j += 1;
            }
            (Kind::Comment, j)
        } else if c == '"' {
            let mut j = i + 1;
            while j < chars.len() && chars[j].1 != '"' && chars[j].1 != '\n' {
                j += if chars[j].1 == '\\' { 2 } else { 1 };
            }
            (Kind::Str, (j + 1).min(chars.len()))
        } else if c.is_ascii_digit() {
            let mut j = i;
            while j < chars.len() && (chars[j].1.is_ascii_digit() || chars[j].1 == '.') {
                j += 1;
            }
            (Kind::Number, j)
        } else if is_word(c) {
            let mut j = i;
            while j < chars.len() && is_word(chars[j].1) {
                j += 1;
            }
            let word = &src[start..end_of(j)];
            let kind = if after_dot {
                Kind::Field
            } else if after_on && brixo_runtime::EVENTS.contains(&word) {
                Kind::Event
            } else if KEYWORDS.contains(&word) {
                Kind::Keyword
            } else if builtins().any(|b| b == word) {
                Kind::Function
            } else {
                Kind::Plain
            };
            (kind, j)
        } else {
            (Kind::Plain, i + 1)
        };
        let text = &src[start..end_of(next)];
        if !text.trim().is_empty() {
            after_dot = text == ".";
            after_on = text == "on";
        }
        match out.last_mut() {
            Some((range, k)) if *k == kind && kind == Kind::Plain => range.end = end_of(next),
            _ => out.push((start..end_of(next), kind)),
        }
        i = next;
    }
    out
}

/// A layout job for the script editor: coloured, with `error_line`
/// (1-based) shaded.
pub fn highlight(src: &str, error_line: Option<usize>, font: egui::FontId) -> egui::text::LayoutJob {
    use brixo_client::theme;
    let colour = |k: Kind| match k {
        Kind::Plain => theme::site::CODE_TEXT,
        // The same colours as the code in the website's guide.
        Kind::Keyword => egui::Color32::from_rgb(255, 207, 92),
        Kind::Function => egui::Color32::from_rgb(127, 209, 255),
        Kind::Event => egui::Color32::from_rgb(245, 154, 224),
        Kind::Field => egui::Color32::from_rgb(170, 200, 240),
        Kind::Str => egui::Color32::from_rgb(165, 227, 140),
        Kind::Number => egui::Color32::from_rgb(255, 162, 122),
        Kind::Comment => egui::Color32::from_rgb(127, 147, 168),
    };
    // Where the error line starts and ends, in bytes.
    let err = error_line.and_then(|n| {
        let start = if n <= 1 { 0 } else { src.match_indices('\n').nth(n - 2)?.0 + 1 };
        let end = src[start..].find('\n').map(|e| start + e).unwrap_or(src.len());
        Some(start..end)
    });
    let mut job = egui::text::LayoutJob::default();
    for (range, kind) in tokens(src) {
        // Split each stretch where it enters or leaves the error line.
        let mut cuts = vec![range.start, range.end];
        if let Some(e) = &err {
            for p in [e.start, e.end] {
                if p > range.start && p < range.end {
                    cuts.push(p);
                }
            }
        }
        cuts.sort_unstable();
        for w in cuts.windows(2) {
            let (a, b) = (w[0], w[1]);
            let in_error = err.as_ref().is_some_and(|e| a >= e.start && b <= e.end.max(e.start + 1) && a < e.end);
            job.append(
                &src[a..b],
                0.0,
                egui::TextFormat {
                    font_id: font.clone(),
                    color: colour(kind),
                    italics: kind == Kind::Comment,
                    background: if in_error { egui::Color32::from_rgb(90, 30, 34) } else { egui::Color32::TRANSPARENT },
                    ..Default::default()
                },
            );
        }
    }
    job
}

// --- Autocomplete ---------------------------------------------------------------

/// Suggestions for the word being typed at `cursor` (a character index):
/// where that word starts (a character index) and up to 6 names that start
/// with it. After a `.` only fields are offered; after `on `, events.
pub fn completions(src: &str, cursor: usize) -> Option<(usize, Vec<&'static str>)> {
    let chars: Vec<char> = src.chars().collect();
    let cursor = cursor.min(chars.len());
    // Only at the end of a word (not in the middle of one).
    if chars.get(cursor).is_some_and(|c| is_word(*c)) {
        return None;
    }
    let mut start = cursor;
    while start > 0 && is_word(chars[start - 1]) {
        start -= 1;
    }
    let word: String = chars[start..cursor].iter().collect();
    if word.is_empty() || word.chars().next().unwrap().is_ascii_digit() {
        return None;
    }
    // Not inside a comment or a string.
    let line_start = chars[..start].iter().rposition(|c| *c == '\n').map(|p| p + 1).unwrap_or(0);
    let before: String = chars[line_start..start].iter().collect();
    if before.contains("--") || before.matches('"').count() % 2 == 1 {
        return None;
    }
    let after_dot = start > 0 && chars[start - 1] == '.';
    let after_on = before.trim_end().ends_with("on") && before.trim_end().split_whitespace().last() == Some("on");
    let pool: Vec<&'static str> = if after_dot {
        FIELDS.to_vec()
    } else if after_on {
        brixo_runtime::EVENTS.to_vec()
    } else {
        KEYWORDS.iter().copied().chain(builtins()).chain(FIELDS.iter().copied()).collect()
    };
    let mut found: Vec<&'static str> = pool.into_iter().filter(|n| n.starts_with(word.as_str()) && *n != word).collect();
    found.sort_by_key(|n| (n.len(), *n));
    found.dedup();
    found.truncate(6);
    (!found.is_empty()).then_some((start, found))
}

// --- Dragging parts over surfaces ------------------------------------------------

fn part_turn(p: &PartProps) -> Quat {
    Quat::from_euler(glam::EulerRot::YXZ, p.rotation.y.to_radians(), p.rotation.x.to_radians(), p.rotation.z.to_radians())
}

fn v(p: brixo_core::Vec3) -> Vec3 {
    Vec3::new(p.x, p.y, p.z)
}

/// The nearest surface a ray hits, skipping `skip`: where, and which way the
/// surface faces. Rotated parts count as their rotated boxes. If nothing is
/// hit, the ground (y = 0) is used.
pub fn surface_hit(model: &DataModel, origin: Vec3, dir: Vec3, skip: &HashSet<InstanceId>) -> Option<(Vec3, Vec3)> {
    let mut best: Option<(f32, Vec3)> = None;
    for id in model.walk() {
        if skip.contains(&id) {
            continue;
        }
        let Some(p) = model.part(id) else { continue };
        if p.transparency >= 1.0 {
            continue;
        }
        // The ray in the part's own space, against its box.
        let turn = part_turn(p);
        let inv = turn.inverse();
        let o = inv * (origin - v(p.position));
        let d = inv * dir;
        let h = v(p.size) / 2.0;
        let (mut t_near, mut t_far, mut normal) = (f32::NEG_INFINITY, f32::INFINITY, Vec3::ZERO);
        let mut missed = false;
        for axis in 0..3 {
            let (oa, da, ha) = (o[axis], d[axis], h[axis]);
            if da.abs() < 1e-8 {
                if oa.abs() > ha {
                    missed = true;
                    break;
                }
                continue;
            }
            let (mut t1, mut t2) = ((-ha - oa) / da, (ha - oa) / da);
            let mut n = Vec3::ZERO;
            n[axis] = -da.signum();
            if t1 > t2 {
                std::mem::swap(&mut t1, &mut t2);
            }
            if t1 > t_near {
                t_near = t1;
                normal = n;
            }
            t_far = t_far.min(t2);
        }
        if missed || t_near > t_far || t_near < 0.0 {
            continue;
        }
        if best.is_none_or(|(t, _)| t_near < t) {
            best = Some((t_near, turn * normal));
        }
    }
    match best {
        Some((t, n)) => Some((origin + dir * t, n)),
        None if dir.y < -1e-4 => Some((origin + dir * (-origin.y / dir.y), Vec3::Y)),
        None => None,
    }
}

/// Where a box with half-extents `half` goes to rest on a surface at
/// `point` facing `normal`: pushed out along the surface's main axis by
/// half its size that way, centred on the point otherwise. With `snap`,
/// the other two axes land on whole studs.
pub fn rest_on(point: Vec3, normal: Vec3, half: Vec3, snap: bool) -> Vec3 {
    let axis = if normal.x.abs() >= normal.y.abs() && normal.x.abs() >= normal.z.abs() {
        0
    } else if normal.y.abs() >= normal.z.abs() {
        1
    } else {
        2
    };
    let mut c = point;
    for i in 0..3 {
        if i == axis {
            c[i] = point[i] + normal[i].signum() * half[i];
        } else if snap {
            c[i] = c[i].round();
        }
    }
    c
}

/// A part's (or several parts') box in the world, as centre and half-extents.
pub fn world_box(parts: &[PartProps]) -> Option<(Vec3, Vec3)> {
    let mut lo = Vec3::splat(f32::INFINITY);
    let mut hi = Vec3::splat(f32::NEG_INFINITY);
    for p in parts {
        let turn = part_turn(p);
        let h = v(p.size) / 2.0;
        for sx in [-1.0, 1.0] {
            for sy in [-1.0, 1.0] {
                for sz in [-1.0, 1.0] {
                    let corner = v(p.position) + turn * Vec3::new(h.x * sx, h.y * sy, h.z * sz);
                    lo = lo.min(corner);
                    hi = hi.max(corner);
                }
            }
        }
    }
    (lo.x.is_finite()).then(|| ((lo + hi) / 2.0, (hi - lo) / 2.0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use brixo_core::Class;

    fn kinds(src: &str) -> Vec<(String, Kind)> {
        tokens(src).into_iter().map(|(r, k)| (src[r].to_string(), k)).filter(|(t, _)| !t.trim().is_empty()).collect()
    }

    #[test]
    fn scripts_are_coloured_by_what_each_word_is() {
        let got = kinds("on touched(p) -- hi\n  p.health = 5 + max(1, 2)\n  print(\"x\")\nend");
        let want = |t: &str| got.iter().find(|(w, _)| w == t).map(|(_, k)| *k);
        assert_eq!(want("on"), Some(Kind::Keyword));
        assert_eq!(want("touched"), Some(Kind::Event));
        assert_eq!(want("-- hi"), Some(Kind::Comment));
        assert_eq!(want("health"), Some(Kind::Field));
        assert_eq!(want("5"), Some(Kind::Number));
        assert_eq!(want("max"), Some(Kind::Function));
        assert_eq!(want("print"), Some(Kind::Function));
        assert_eq!(want("\"x\""), Some(Kind::Str));
        assert_eq!(want("end"), Some(Kind::Keyword));
        // Every byte is covered, in order, exactly once.
        let src = "a.b = \"é\" -- ü\n1";
        let t = tokens(src);
        assert_eq!(t.first().unwrap().0.start, 0);
        assert_eq!(t.last().unwrap().0.end, src.len());
        assert!(t.windows(2).all(|w| w[0].0.end == w[1].0.start));
    }

    #[test]
    fn the_error_line_is_shaded() {
        let job = highlight("x = 1\ny = (\nz = 3", Some(2), egui::FontId::monospace(13.0));
        let shaded: String = job
            .sections
            .iter()
            .filter(|s| s.format.background != egui::Color32::TRANSPARENT)
            .map(|s| &job.text[s.byte_range.clone()])
            .collect();
        assert_eq!(shaded, "y = (");
    }

    #[test]
    fn autocomplete_suggests_what_fits_where_you_are() {
        let at_end = |s: &str| completions(s, s.chars().count());
        let (start, list) = at_end("pla").unwrap();
        assert_eq!(start, 0);
        assert!(list.contains(&"players") && list.contains(&"play_sound"), "{list:?}");
        let (_, list) = at_end("p.pos").unwrap();
        assert_eq!(list, vec!["position"], "fields after a dot");
        let (_, list) = at_end("on tou").unwrap();
        assert_eq!(list, vec!["touched"], "events after on");
        let (_, list) = at_end("wa").unwrap();
        assert!(list.contains(&"wait") && list.contains(&"walk_speed"));
        assert!(at_end("-- pla").is_none(), "not in comments");
        assert!(at_end("print(\"pla").is_none(), "not in strings");
        assert!(at_end("players").is_none(), "nothing left to add");
        assert!(completions("players", 3).is_none(), "not mid-word");
    }

    #[test]
    fn dragged_parts_rest_on_the_surface_under_the_mouse() {
        let mut dm = DataModel::new();
        let root = dm.root();
        let floor = dm.create(Class::Part, "Floor", root).unwrap();
        {
            let p = dm.part_mut(floor).unwrap();
            p.position = brixo_core::Vec3::new(0.0, 0.5, 0.0);
            p.size = brixo_core::Vec3::new(40.0, 1.0, 40.0);
        }
        let wall = dm.create(Class::Part, "Wall", root).unwrap();
        {
            let p = dm.part_mut(wall).unwrap();
            p.position = brixo_core::Vec3::new(10.0, 5.0, 0.0);
            p.size = brixo_core::Vec3::new(2.0, 10.0, 10.0);
        }
        let dragged = dm.create(Class::Part, "Brick", root).unwrap();
        let skip: HashSet<_> = [dragged].into_iter().collect();

        // Looking straight down at the floor: rests on top of it.
        let (hit, n) = surface_hit(&dm, Vec3::new(3.3, 20.0, -2.6), Vec3::NEG_Y, &skip).unwrap();
        assert!((hit.y - 1.0).abs() < 1e-4 && n.abs_diff_eq(Vec3::Y, 1e-4));
        let c = rest_on(hit, n, Vec3::new(2.0, 0.5, 1.0), true);
        assert_eq!(c, Vec3::new(3.0, 1.5, -3.0), "on top, snapped to whole studs");

        // Looking at the wall from the side: rests against it.
        let (hit, n) = surface_hit(&dm, Vec3::new(-10.0, 4.0, 0.0), Vec3::X, &skip).unwrap();
        assert!((hit.x - 9.0).abs() < 1e-4 && n.abs_diff_eq(Vec3::NEG_X, 1e-4));
        let c = rest_on(hit, n, Vec3::new(2.0, 0.5, 1.0), false);
        assert!((c.x - 7.0).abs() < 1e-4, "against the wall: {c:?}");

        // A rotated slab is hit on its rotated face.
        dm.part_mut(wall).unwrap().rotation = brixo_core::Vec3::new(0.0, 0.0, 90.0);
        let (_, n) = surface_hit(&dm, Vec3::new(10.0, 30.0, 0.0), Vec3::NEG_Y, &skip).unwrap();
        assert!(n.abs_diff_eq(Vec3::Y, 1e-3), "{n:?}");

        // Nothing there: the ground.
        let empty = DataModel::new();
        let (hit, _) = surface_hit(&empty, Vec3::new(0.0, 10.0, 0.0), Vec3::new(0.0, -1.0, 1.0).normalize(), &skip).unwrap();
        assert!(hit.y.abs() < 1e-4 && (hit.z - 10.0).abs() < 1e-3);
    }

    #[test]
    fn a_rotated_part_boxes_as_it_stands() {
        let mut p = PartProps::default();
        p.size = brixo_core::Vec3::new(4.0, 1.0, 2.0);
        p.rotation = brixo_core::Vec3::new(0.0, 90.0, 0.0);
        let (_, half) = world_box(&[p]).unwrap();
        assert!(half.abs_diff_eq(Vec3::new(1.0, 0.5, 2.0), 1e-4), "{half:?}");
    }
}
