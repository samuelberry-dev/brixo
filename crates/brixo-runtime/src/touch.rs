//! Finds which parts overlap, for `on touched`.
//!
//! Each part's rotated box is wrapped in an axis-aligned box, which is fast
//! and good enough for now; exact shapes come with physics.

use std::collections::HashSet;

use brixo_core::{DataModel, InstanceId, PartProps};

/// Boxes must overlap by more than this on every axis. Parts sitting
/// exactly on top of each other (like on a baseplate) don't count.
const TOUCH_EPSILON: f32 = 0.01;

type Mat3 = [[f32; 3]; 3];

fn mul(a: Mat3, b: Mat3) -> Mat3 {
    let mut out = [[0.0; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            out[i][j] = (0..3).map(|k| a[i][k] * b[k][j]).sum();
        }
    }
    out
}

/// Same rotation order as the renderer: yaw (Y), then pitch (X), then roll (Z).
fn rotation(p: &PartProps) -> Mat3 {
    let (sx, cx) = p.rotation.x.to_radians().sin_cos();
    let (sy, cy) = p.rotation.y.to_radians().sin_cos();
    let (sz, cz) = p.rotation.z.to_radians().sin_cos();
    let ry = [[cy, 0.0, sy], [0.0, 1.0, 0.0], [-sy, 0.0, cy]];
    let rx = [[1.0, 0.0, 0.0], [0.0, cx, -sx], [0.0, sx, cx]];
    let rz = [[cz, -sz, 0.0], [sz, cz, 0.0], [0.0, 0.0, 1.0]];
    mul(mul(ry, rx), rz)
}

/// Centre and half-size of the axis-aligned box around a part.
fn bounds(p: &PartProps) -> ([f32; 3], [f32; 3]) {
    let r = rotation(p);
    let half = [p.size.x / 2.0, p.size.y / 2.0, p.size.z / 2.0];
    let mut extent = [0.0; 3];
    for i in 0..3 {
        extent[i] = (0..3).map(|j| r[i][j].abs() * half[j]).sum();
    }
    ([p.position.x, p.position.y, p.position.z], extent)
}

/// Every pair of parts currently overlapping, smaller id first.
pub fn overlapping_pairs(model: &DataModel) -> HashSet<(InstanceId, InstanceId)> {
    let boxes: Vec<(InstanceId, [f32; 3], [f32; 3])> = model
        .iter()
        .filter_map(|inst| {
            let p = model.part(inst.id)?;
            let (c, e) = bounds(p);
            Some((inst.id, c, e))
        })
        .collect();

    let mut pairs = HashSet::new();
    for i in 0..boxes.len() {
        for j in (i + 1)..boxes.len() {
            let (a, ca, ea) = boxes[i];
            let (b, cb, eb) = boxes[j];
            let overlaps = (0..3).all(|k| (ca[k] - cb[k]).abs() < ea[k] + eb[k] - TOUCH_EPSILON);
            if overlaps {
                let pair = if a.raw() < b.raw() { (a, b) } else { (b, a) };
                pairs.insert(pair);
            }
        }
    }
    pairs
}
