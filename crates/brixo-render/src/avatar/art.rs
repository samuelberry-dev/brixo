//! The avatar's pictures, drawn in code: faces, the shading printed on
//! clothes, and t-shirt pictures. Clothes and t-shirts are chunky pixel art
//! (32x32, each pixel shown 2x2) to match the faces and the bricks.
//!
//! Clothes are grey: the game multiplies them by the player's shirt or
//! pants colour, so 1.0 (white) is the colour itself and darker greys are
//! stripes, seams and pockets. Faces and t-shirts are in full colour, with
//! see-through backgrounds.

use brixo_core::{Face, Pants, Shirt, TShirt};
use glam::Vec2;

/// A pixel: RGBA.
pub(crate) type Px = [u8; 4];
const CLEAR: Px = [0, 0, 0, 0];

// --- faces -------------------------------------------------------------------

const INK: [u8; 3] = [20, 20, 20];
const PINK: [u8; 3] = [235, 110, 140];
const RED: [u8; 3] = [210, 30, 50];
const WHITE: [u8; 3] = [250, 250, 250];

/// One pixel of a face (64x64, y down), or None where the head shows.
pub(crate) fn face_pixel(face: Face, x: f32, y: f32) -> Option<[u8; 3]> {
    let p = Vec2::new(x, y);
    let ellipse = |cx: f32, cy: f32, rx: f32, ry: f32| ((x - cx) / rx).powi(2) + ((y - cy) / ry).powi(2) <= 1.0;
    let ring = |cx: f32, cy: f32, r: f32, w: f32| (p.distance(Vec2::new(cx, cy)) - r).abs() <= w / 2.0;
    let line = |a: (f32, f32), b: (f32, f32), w: f32| segment_distance(p, a.into(), b.into()) <= w / 2.0;
    let curve = |a: (f32, f32), c: (f32, f32), b: (f32, f32), w: f32| {
        let (a, c, b) = (Vec2::from(a), Vec2::from(c), Vec2::from(b));
        let at = |t: f32| a * (1.0 - t) * (1.0 - t) + c * 2.0 * t * (1.0 - t) + b * t * t;
        (0..16).any(|i| segment_distance(p, at(i as f32 / 16.0), at((i + 1) as f32 / 16.0)) <= w / 2.0)
    };
    // A little heart centred on (cx, cy), about 2r across.
    let heart = |cx: f32, cy: f32, r: f32| {
        let (u, v) = ((x - cx) / r, (cy - y) / r + 0.25);
        let a = u * u + v * v - 1.0;
        a * a * a - u * u * v * v * v <= 0.0
    };
    let eyes = || ellipse(21.0, 25.0, 4.0, 7.0) || ellipse(43.0, 25.0, 4.0, 7.0);
    let ink = |b: bool| if b { Some(INK) } else { None };
    match face {
        Face::Smile => ink(eyes() || curve((18.0, 41.0), (32.0, 52.0), (46.0, 41.0), 3.2)),
        Face::Happy => ink(
            line((15.0, 28.0), (21.0, 21.0), 3.2)
                || line((21.0, 21.0), (27.0, 28.0), 3.2)
                || line((37.0, 28.0), (43.0, 21.0), 3.2)
                || line((43.0, 21.0), (49.0, 28.0), 3.2)
                || (ellipse(32.0, 40.0, 11.0, 9.0) && y >= 40.0),
        ),
        Face::Surprised => ink(ring(21.0, 25.0, 5.0, 2.6) || ring(43.0, 25.0, 5.0, 2.6) || ellipse(32.0, 45.0, 3.5, 5.0)),
        Face::Determined => ink(
            ellipse(21.0, 27.0, 3.5, 5.0)
                || ellipse(43.0, 27.0, 3.5, 5.0)
                || line((14.0, 16.0), (27.0, 20.0), 3.2)
                || line((50.0, 16.0), (37.0, 20.0), 3.2)
                || line((24.0, 43.0), (40.0, 43.0), 3.2),
        ),
        Face::Wink => ink(
            ellipse(21.0, 25.0, 4.0, 7.0)
                || curve((37.0, 27.0), (43.0, 21.0), (49.0, 27.0), 3.2)
                || curve((18.0, 40.0), (32.0, 52.0), (46.0, 40.0), 3.2),
        ),
        Face::Grin => {
            let mouth = ellipse(32.0, 39.0, 14.0, 12.0) && y >= 39.0;
            let teeth = mouth && (40.0..44.5).contains(&y) && x > 20.0 && x < 44.0;
            if teeth {
                Some(WHITE)
            } else {
                ink(eyes() || mouth)
            }
        }
        Face::Silly => {
            let tongue = ellipse(37.0, 48.0, 5.0, 6.0) && y > 44.0;
            if tongue && !line((37.0, 45.0), (37.0, 51.0), 1.2) {
                return Some(PINK);
            }
            ink(ellipse(21.0, 25.0, 4.0, 7.0)
                || ring(43.0, 25.0, 5.0, 2.6)
                || ellipse(43.0, 25.0, 2.0, 2.0)
                || curve((18.0, 40.0), (32.0, 50.0), (46.0, 40.0), 3.2)
                || (tongue && line((37.0, 45.0), (37.0, 51.0), 1.2)))
        }
        Face::Sleepy => ink(
            curve((15.0, 25.0), (21.0, 30.0), (27.0, 25.0), 3.0)
                || curve((37.0, 25.0), (43.0, 30.0), (49.0, 25.0), 3.0)
                || ring(32.0, 44.0, 3.0, 2.4)
                // A little "z" up by the forehead.
                || line((47.0, 8.0), (54.0, 8.0), 2.0)
                || line((54.0, 8.0), (47.0, 15.0), 2.0)
                || line((47.0, 15.0), (54.0, 15.0), 2.0),
        ),
        Face::Angry => ink(
            ellipse(21.0, 28.0, 3.5, 5.0)
                || ellipse(43.0, 28.0, 3.5, 5.0)
                || line((13.0, 17.0), (28.0, 23.0), 3.4)
                || line((51.0, 17.0), (36.0, 23.0), 3.4)
                || curve((20.0, 48.0), (32.0, 38.0), (44.0, 48.0), 3.2),
        ),
        Face::Smirk => ink(
            ellipse(21.0, 26.0, 3.5, 6.0)
                || ellipse(43.0, 26.0, 3.5, 6.0)
                || line((14.0, 17.0), (27.0, 18.0), 2.6)
                || curve((22.0, 44.0), (36.0, 46.0), (46.0, 38.0), 3.2),
        ),
        Face::Cat => {
            let blush = ellipse(13.0, 36.0, 4.5, 2.5) || ellipse(51.0, 36.0, 4.5, 2.5);
            let drawn = eyes()
                || curve((22.0, 40.0), (27.0, 46.0), (32.0, 40.0), 2.8)
                || curve((32.0, 40.0), (37.0, 46.0), (42.0, 40.0), 2.8)
                || line((3.0, 38.0), (13.0, 40.0), 1.6)
                || line((3.0, 44.0), (13.0, 42.0), 1.6)
                || line((61.0, 38.0), (51.0, 40.0), 1.6)
                || line((61.0, 44.0), (51.0, 42.0), 1.6);
            if drawn {
                Some(INK)
            } else if blush {
                Some(PINK)
            } else {
                None
            }
        }
        Face::HeartEyes => {
            if heart(21.0, 25.0, 7.0) || heart(43.0, 25.0, 7.0) {
                Some(RED)
            } else {
                ink(ellipse(32.0, 40.0, 11.0, 9.0) && y >= 40.0)
            }
        }
        Face::Worried => ink(
            ellipse(21.0, 27.0, 3.5, 5.5)
                || ellipse(43.0, 27.0, 3.5, 5.5)
                || line((14.0, 20.0), (26.0, 15.0), 2.8)
                || line((50.0, 20.0), (38.0, 15.0), 2.8)
                || curve((20.0, 46.0), (26.0, 41.0), (32.0, 46.0), 2.8)
                || curve((32.0, 46.0), (38.0, 51.0), (44.0, 46.0), 2.8),
        ),
        Face::Laugh => ink(
            line((14.0, 20.0), (25.0, 26.0), 3.2)
                || line((25.0, 26.0), (14.0, 32.0), 3.2)
                || line((50.0, 20.0), (39.0, 26.0), 3.2)
                || line((39.0, 26.0), (50.0, 32.0), 3.2)
                || (ellipse(32.0, 38.0, 14.0, 13.0) && y >= 38.0),
        ),
    }
}

pub(crate) fn segment_distance(p: Vec2, a: Vec2, b: Vec2) -> f32 {
    let ab = b - a;
    let t = ((p - a).dot(ab) / ab.length_squared().max(1e-6)).clamp(0.0, 1.0);
    p.distance(a + ab * t)
}

// --- clothes -----------------------------------------------------------------

/// Which side of a shirt a picture is for: the front of the torso has the
/// collar and pockets; everything else (back, sides, sleeves) is plainer.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Side {
    Around,
    Front,
}

/// A little hash for pixel noise.
fn noise(x: u32, y: u32, seed: u32) -> f32 {
    let mut h = x.wrapping_mul(374_761_393) ^ y.wrapping_mul(668_265_263) ^ seed.wrapping_mul(2_246_822_519);
    h = (h ^ (h >> 13)).wrapping_mul(1_274_126_177);
    ((h ^ (h >> 16)) & 0xffff) as f32 / 65535.0
}

/// Plaid: light and dark bands crossing both ways.
fn plaid(x: i32, y: i32) -> f32 {
    let band = |v: i32| match v.rem_euclid(12) {
        0..=3 => 0.7,
        6 => 0.82,
        _ => 1.0,
    };
    let (a, b) = (band(x), band(y));
    if a < 1.0 && b < 1.0 { a * b * 0.95 } else { a.min(b) }
}

/// Camouflage blobs in three shades (tiles every 32 pixels).
fn camo(x: i32, y: i32) -> f32 {
    let blob = |s: u32, cell: i32| {
        let (gx, gy) = (x.div_euclid(cell), y.div_euclid(cell));
        let (fx, fy) = (x.rem_euclid(cell) as f32 / cell as f32, y.rem_euclid(cell) as f32 / cell as f32);
        let n = (32 / cell) as u32;
        let at = |i: i32, j: i32| noise(i.rem_euclid(n as i32) as u32, j.rem_euclid(n as i32) as u32, s);
        let top = at(gx, gy) + (at(gx + 1, gy) - at(gx, gy)) * fx;
        let bottom = at(gx, gy + 1) + (at(gx + 1, gy + 1) - at(gx, gy + 1)) * fx;
        top + (bottom - top) * fy
    };
    let a = blob(41, 8);
    let b = blob(42, 4);
    if a > 0.62 {
        0.55
    } else if b > 0.6 {
        0.76
    } else {
        1.0
    }
}

/// One pixel (32x32, y down) of a shirt's shading.
pub(crate) fn shirt_pixel(shirt: Shirt, side: Side, x: i32, y: i32) -> f32 {
    let front = side == Side::Front;
    let dx = (x - 16).abs() as f32 + if x < 16 { 0.5 } else { -0.5 };
    // A round neck: a dark curve at the top middle.
    let crew = |depth: f32| -> bool {
        let r = (dx * dx + (y as f32 + 3.0 - depth).powi(2) * 2.2).sqrt();
        front && (6.2..7.6).contains(&r) && (y as f32) < depth + 2.0
    };
    let hem = y >= 30;
    let cuff = y >= 28;
    match shirt {
        Shirt::None => 1.0,
        Shirt::Tee => {
            if crew(3.0) || hem {
                0.78
            } else {
                1.0
            }
        }
        Shirt::Tank => {
            // A deep scoop, and wide armholes at the sides.
            let scoop = front && y < 9 && dx < 8.0 - y as f32 * 0.25 && dx > 6.5 - y as f32 * 0.3;
            if scoop || hem {
                0.78
            } else {
                1.0
            }
        }
        Shirt::LongSleeve => {
            if crew(3.0) || cuff {
                0.8
            } else {
                1.0
            }
        }
        Shirt::Striped => {
            let stripe = (y / 4) % 2 == 1;
            if crew(3.0) {
                0.5
            } else if stripe {
                0.62
            } else {
                1.0
            }
        }
        Shirt::Polo => {
            if front {
                let collar = y < 7 && dx < 9.0 - y as f32 && dx >= 1.0;
                let edge = y < 8 && (dx - (9.0 - y as f32)).abs() < 1.0;
                let placket = dx < 1.5 && y < 13;
                let button = dx < 1.0 && (y == 9 || y == 12);
                if button {
                    0.45
                } else if edge {
                    0.6
                } else if collar {
                    0.9
                } else if placket {
                    0.82
                } else if hem {
                    0.78
                } else {
                    1.0
                }
            } else if hem {
                0.78
            } else {
                1.0
            }
        }
        Shirt::Hoodie => {
            if front {
                let pocket = (18..28).contains(&y) && dx < 10.0 - (27 - y).max(0) as f32 * 0.0;
                let pocket_edge = pocket && (y == 18 || dx >= 9.0 || y == 27);
                let slit = (19..27).contains(&y) && (dx - 9.0).abs() < 1.0 && y < 24;
                let strings = (dx - 3.0).abs() < 0.8 && (3..12).contains(&y);
                let hood = crew(3.0) || (y < 3 && dx < 8.0);
                if strings {
                    0.9
                } else if hood || pocket_edge || slit {
                    0.62
                } else if cuff {
                    0.8
                } else {
                    1.0
                }
            } else if cuff {
                0.8
            } else {
                1.0
            }
        }
        Shirt::Flannel => {
            let base = plaid(x, y);
            if front && (dx < 0.8 || (dx < 2.0 && y % 6 == 3)) {
                base * 0.55
            } else if front && y < 5 && dx < 6.0 - y as f32 {
                base * 0.8
            } else {
                base
            }
        }
        Shirt::Jacket => {
            if front {
                // Open down the middle over a dark top, with lapels.
                let gap = dx < 4.0 - (y as f32 * 0.06).min(2.5);
                let lapel = y < 14 && (dx - (4.5 + y as f32 * 0.3)).abs() < 1.2;
                let zip = (dx - 4.0).abs() < 0.6 && y >= 12;
                let pocket = (20..22).contains(&y) && (7.0..13.0).contains(&dx);
                if gap {
                    0.28
                } else if lapel || zip {
                    0.62
                } else if pocket || cuff {
                    0.75
                } else {
                    1.0
                }
            } else if cuff {
                0.75
            } else {
                1.0
            }
        }
        Shirt::Sweater => {
            // Knit rows, a band of diamonds across the chest, ribbed cuffs.
            let rib = y % 3 == 2;
            let band = (10..16).contains(&y);
            let diamond = band && ((x.rem_euclid(6) - 3).abs() + (y - 13).abs()) <= 2;
            if crew(3.0) {
                0.7
            } else if diamond {
                0.55
            } else if band && (y == 10 || y == 15) {
                0.7
            } else if cuff && x % 2 == 0 {
                0.78
            } else if rib {
                0.9
            } else {
                1.0
            }
        }
        Shirt::Jersey => {
            // A V-neck, stripes round the sleeves, and a big 7 on the front.
            let vee = front && y < 8 && (dx - (6.0 - y as f32 * 0.7)).abs() < 1.2;
            let stripe = !front && (22..26).contains(&y);
            let seven = front && {
                let (u, v) = (x - 11, y - 11);
                ((0..11).contains(&u) && (0..3).contains(&v)) || ((0..16).contains(&v) && (u - (9 - v * 5 / 8)).abs() <= 1 && v >= 2)
            };
            if vee || seven {
                0.45
            } else if stripe || hem {
                0.6
            } else {
                1.0
            }
        }
        Shirt::Camo => {
            if crew(3.0) {
                0.45
            } else {
                camo(x, y)
            }
        }
    }
}

/// One pixel (32x32) of pants' shading. The same picture goes on every side
/// of each leg.
pub(crate) fn pants_pixel(pants: Pants, x: i32, y: i32) -> f32 {
    let hem = y >= 30;
    match pants {
        Pants::None => 1.0,
        Pants::Plain => {
            if hem || y < 2 {
                0.8
            } else {
                1.0
            }
        }
        Pants::Jeans => {
            // Denim twill, a stitched seam down each side, and a turned-up hem.
            let twill = if (x + y).rem_euclid(3) == 0 { 0.9 } else { 1.0 };
            let seam = (x == 1 || x == 30) && y % 3 != 0;
            let waist = y < 3;
            if hem || waist {
                0.72
            } else if seam {
                0.7
            } else {
                twill * (0.95 + noise(x as u32, y as u32, 5) * 0.05)
            }
        }
        Pants::Shorts => {
            if y >= 28 || y < 2 {
                0.8
            } else {
                1.0
            }
        }
        Pants::Cargo => {
            // A big pocket with a flap on each side.
            let pocket = (12..24).contains(&y) && (6..26).contains(&x);
            let flap = (12..15).contains(&y) && (5..27).contains(&x);
            let edge = pocket && (x == 6 || x == 25 || y == 23);
            if flap && y == 14 {
                0.6
            } else if flap {
                0.85
            } else if edge {
                0.65
            } else if hem || y < 2 {
                0.8
            } else {
                1.0
            }
        }
        Pants::Track => {
            // Two stripes down the sides.
            if (x <= 2 || x >= 29) && x != 1 && x != 30 {
                0.5
            } else if x == 1 || x == 30 {
                1.0
            } else if hem || y < 2 {
                0.78
            } else {
                1.0
            }
        }
        Pants::Plaid => {
            if y >= 29 {
                0.62
            } else {
                plaid(x, y)
            }
        }
    }
}

// --- t-shirt pictures -------------------------------------------------------------

const OUTLINE: [u8; 3] = [28, 24, 30];

/// One pixel of a t-shirt picture (32x32, y down).
pub(crate) fn tshirt_pixel(t: TShirt, x: i32, y: i32) -> Px {
    let fill = |x: i32, y: i32| tshirt_fill(t, x, y);
    if let Some(c) = fill(x, y) {
        return [c[0], c[1], c[2], 255];
    }
    // Everything gets a dark outline, like a sticker.
    let near = [(-1, 0), (1, 0), (0, -1), (0, 1)].iter().any(|(dx, dy)| fill(x + dx, y + dy).is_some());
    if near { [OUTLINE[0], OUTLINE[1], OUTLINE[2], 255] } else { CLEAR }
}

fn tshirt_fill(t: TShirt, x: i32, y: i32) -> Option<[u8; 3]> {
    if !(0..32).contains(&x) || !(0..32).contains(&y) {
        return None;
    }
    let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
    let p = Vec2::new(fx, fy);
    let circle = |cx: f32, cy: f32, r: f32| p.distance(Vec2::new(cx, cy)) <= r;
    let rect = |x0: i32, y0: i32, x1: i32, y1: i32| x >= x0 && x < x1 && y >= y0 && y < y1;
    // Inside a triangle (any winding).
    let tri = |a: (f32, f32), b: (f32, f32), c: (f32, f32)| {
        let s = |p1: (f32, f32), p2: (f32, f32)| (fx - p2.0) * (p1.1 - p2.1) - (p1.0 - p2.0) * (fy - p2.1);
        let (d1, d2, d3) = (s(a, b), s(b, c), s(c, a));
        !((d1 < 0.0 || d2 < 0.0 || d3 < 0.0) && (d1 > 0.0 || d2 > 0.0 || d3 > 0.0))
    };
    let heart = |cx: f32, cy: f32, r: f32| {
        let (u, v) = ((fx - cx) / r, (cy - fy) / r + 0.25);
        let a = u * u + v * v - 1.0;
        a * a * a - u * u * v * v * v <= 0.0
    };
    match t {
        TShirt::Brick => {
            let body = rect(4, 13, 28, 27);
            let studs = rect(7, 9, 13, 13) || rect(19, 9, 25, 13);
            if studs {
                Some(if y == 9 { [240, 100, 90] } else { [196, 40, 28] })
            } else if body {
                Some(if y == 13 || x == 4 { [236, 88, 76] } else if y >= 25 || x == 27 { [140, 26, 20] } else { [196, 40, 28] })
            } else {
                None
            }
        }
        TShirt::Smiley => {
            if !circle(16.0, 16.0, 13.0) {
                return None;
            }
            let eye = circle(11.5, 12.5, 2.0) || circle(20.5, 12.5, 2.0);
            let r = p.distance(Vec2::new(16.0, 15.0));
            let mouth = (7.0..9.0).contains(&r) && fy > 18.0;
            Some(if eye || mouth { OUTLINE } else if circle(12.0, 10.0, 3.0) { [255, 240, 140] } else { [250, 205, 40] })
        }
        TShirt::Heart => {
            if !heart(16.0, 16.0, 12.0) {
                return None;
            }
            Some(if circle(10.0, 11.0, 2.5) { [255, 150, 160] } else { [214, 30, 60] })
        }
        TShirt::Star => {
            // A five-pointed star: inside if the angle-dependent radius fits.
            let d = p - Vec2::new(16.0, 17.0);
            let a = d.y.atan2(d.x) + std::f32::consts::FRAC_PI_2;
            let k = (a * 5.0 / std::f32::consts::TAU).rem_euclid(1.0);
            let edge = 13.5 * (1.0 - 0.55 * (1.0 - (k - 0.5).abs() * 2.0));
            if d.length() > edge {
                return None;
            }
            Some(if d.length() < edge * 0.45 && d.x < 0.0 && d.y < 0.0 { [255, 245, 170] } else { [250, 196, 30] })
        }
        TShirt::Flame => {
            let outer = |cx: f32, w: f32, top: f32| fy > top && fy < 29.0 && (fx - cx).abs() < w * ((fy - top) / (29.0 - top)).sqrt();
            let big = outer(16.0, 11.0, 3.0) || outer(9.0, 6.0, 12.0) || outer(23.0, 6.0, 10.0);
            if !big {
                return None;
            }
            let inner = outer(16.0, 6.0, 12.0) || outer(10.0, 3.0, 19.0) || outer(22.0, 3.0, 18.0);
            let core = outer(16.0, 3.0, 19.0);
            Some(if core { [255, 240, 140] } else if inner { [255, 170, 30] } else { [228, 60, 20] })
        }
        TShirt::Lightning => {
            let bolt = tri((19.0, 2.0), (7.0, 18.0), (17.0, 18.0)) || tri((15.0, 14.0), (25.0, 14.0), (12.0, 30.0));
            if bolt {
                Some(if fx < 14.0 && fy < 16.0 { [255, 250, 180] } else { [252, 212, 40] })
            } else {
                None
            }
        }
        TShirt::Rocket => {
            let nose = tri((16.0, 1.0), (10.5, 10.0), (21.5, 10.0));
            let body = rect(11, 10, 21, 24);
            let window = circle(16.0, 15.0, 2.6);
            let fins = tri((11.0, 17.0), (5.0, 26.0), (11.0, 24.0)) || tri((21.0, 17.0), (27.0, 26.0), (21.0, 24.0));
            let flame = tri((12.0, 24.0), (20.0, 24.0), (16.0, 31.0));
            if window {
                Some(if circle(15.2, 14.2, 1.0) { [220, 240, 255] } else { [70, 150, 220] })
            } else if nose || fins {
                Some([210, 40, 40])
            } else if body {
                Some(if x == 11 { [255, 255, 255] } else if x == 20 { [190, 195, 205] } else { [236, 238, 242] })
            } else if flame {
                Some(if fy < 27.0 { [255, 220, 90] } else { [250, 130, 30] })
            } else {
                None
            }
        }
        TShirt::Pizza => {
            let slice = tri((3.0, 5.0), (29.0, 5.0), (16.0, 30.0));
            if !slice {
                return None;
            }
            let crust = fy < 9.0;
            let pepperoni = circle(11.0, 13.0, 2.5) || circle(20.0, 12.0, 2.5) || circle(16.0, 20.0, 2.2);
            Some(if crust { [200, 130, 60] } else if pepperoni { [190, 40, 35] } else { [252, 212, 90] })
        }
        TShirt::Rainbow => {
            let r = p.distance(Vec2::new(16.0, 24.0));
            let colors = [[220, 40, 40], [245, 140, 30], [250, 215, 40], [70, 180, 70], [50, 120, 220], [130, 70, 190]];
            let cloud = circle(5.0, 24.0, 3.5) || circle(8.5, 22.5, 3.0) || circle(27.0, 24.0, 3.5) || circle(23.5, 22.5, 3.0);
            if cloud && fy < 27.5 {
                return Some([250, 250, 252]);
            }
            if fy > 24.0 || !(6.0..15.0).contains(&r) {
                return None;
            }
            Some(colors[((15.0 - r) / 1.5) as usize % 6])
        }
        TShirt::Ghost => {
            let head = circle(16.0, 13.0, 9.0) && fy <= 13.0;
            let body = rect(7, 13, 25, 26);
            let tails = fy >= 26.0 && fy < 29.0 && (x - 7).rem_euclid(6) < 3 && x < 25;
            if !(head || body || tails) {
                return None;
            }
            let eye = circle(12.5, 13.0, 1.8) || circle(19.5, 13.0, 1.8);
            let mouth = circle(16.0, 19.5, 1.6);
            Some(if eye || mouth { OUTLINE } else { [245, 246, 250] })
        }
        TShirt::NumberOne => {
            // "#1" in chunky gold.
            let hash = (rect(4, 9, 16, 12) || rect(4, 17, 16, 20)) || (rect(6, 6, 9, 24) || rect(11, 6, 14, 24));
            let one = rect(21, 5, 25, 26) || rect(18, 8, 21, 11) || rect(17, 23, 29, 26);
            if hash || one {
                Some(if y <= 6 || (y < 10 && x < 21 && one) { [255, 240, 150] } else { [245, 190, 30] })
            } else {
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_picture_draws_something() {
        for &f in Face::ALL {
            let n = (0..64 * 64).filter(|k| face_pixel(f, (k % 64) as f32 + 0.5, (k / 64) as f32 + 0.5).is_some()).count();
            assert!(n > 60 && n < 900, "{f:?} has {n} pixels");
        }
        for &t in TShirt::ALL {
            let n = (0..32 * 32).filter(|k| tshirt_pixel(t, k % 32, k / 32)[3] == 255).count();
            assert!(n > 80 && n < 900, "{t:?} has {n} pixels");
        }
        for &s in Shirt::ALL {
            for side in [Side::Around, Side::Front] {
                let dark = (0..32 * 32).filter(|k| shirt_pixel(s, side, k % 32, k / 32) < 0.99).count();
                assert!(s == Shirt::None || dark > 0, "{:?} is blank", s);
                assert!(dark < 900, "{s:?} is nearly all dark");
            }
        }
        for &p in Pants::ALL {
            let dark = (0..32 * 32).filter(|k| pants_pixel(p, k % 32, k / 32) < 0.99).count();
            assert!(p == Pants::None || dark > 0, "{p:?} is blank");
        }
    }
}
