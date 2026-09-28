//! Writes a scene per toolbox item, for rendering its Toolbox picture:
//! `cargo run -p brixo-samples --example toolbox_scenes DIR`. Each scene is
//! the item on a baseplate; DIR/<slug>.cam is where the camera goes.
use brixo_core::{Class, DataModel, Material, Vec3};

fn main() {
    let dir_name = std::env::args().nth(1).unwrap_or_else(|| "toolbox-scenes".into());
    std::fs::create_dir_all(&dir_name).unwrap();
    for item in brixo_samples::toolbox::items() {
        let mut dm = DataModel::new();
        let root = dm.root();
        let floor = dm.create(Class::Part, "Baseplate", root).unwrap();
        let p = dm.part_mut(floor).unwrap();
        p.size = Vec3::new(400.0, 1.0, 400.0);
        p.position = Vec3::new(0.0, -0.5, 0.0);
        p.color = brixo_core::Color::new(163, 162, 165);
        p.material = Material::Concrete;
        let spawn = dm.create(Class::SpawnLocation, "Spawn", root).unwrap();
        dm.part_mut(spawn).unwrap().position = Vec3::new(0.0, 0.5, -150.0);
        let pasted = dm.paste_clipboard(&item.content, root).unwrap();
        let mut parts: Vec<_> = pasted.iter().flat_map(|t| dm.parts_under(*t).into_iter().chain(dm.part(*t).map(|_| *t))).collect();
        if item.slug == "gear-kit" {
            // The gear waits underground: lay each piece out on the floor.
            let storage = dm.find_first("Storage").unwrap();
            let tools: Vec<_> = dm.get(storage).unwrap().children.iter().copied().filter(|c| dm.get(*c).unwrap().class == Class::Tool).collect();
            // Each lies on the floor pointing forward, side by side.
            for (i, t) in tools.iter().enumerate() {
                let under = dm.parts_under(*t);
                let first = dm.part(under[0]).unwrap().position;
                for id in under {
                    let q = dm.part_mut(id).unwrap();
                    q.position = Vec3::new((i as f32 - 2.5) * 2.6 + q.position.x - first.x, 1.0 + q.position.y - first.y, q.position.z - first.z);
                    q.anchored = true;
                }
            }
            parts = tools.iter().flat_map(|t| dm.parts_under(*t)).collect();
        }
        // Frame what's there.
        let (mut lo, mut hi) = (Vec3::new(f32::MAX, f32::MAX, f32::MAX), Vec3::new(f32::MIN, f32::MIN, f32::MIN));
        for id in &parts {
            let q = dm.part(*id).unwrap();
            if q.position.y < -100.0 || q.transparency >= 1.0 {
                continue;
            }
            let r = q.size.x.max(q.size.y).max(q.size.z) / 2.0;
            lo = Vec3::new(lo.x.min(q.position.x - r), lo.y.min(q.position.y - r), lo.z.min(q.position.z - r));
            hi = Vec3::new(hi.x.max(q.position.x + r), hi.y.max(q.position.y + r), hi.z.max(q.position.z + r));
        }
        let c = Vec3::new((lo.x + hi.x) / 2.0, (lo.y + hi.y) / 2.0, (lo.z + hi.z) / 2.0);
        let size = (hi.x - lo.x).max(hi.y - lo.y).max(hi.z - lo.z).max(3.0);
        let d = size * 0.8 + 1.5;
        let n = (0.75f32 * 0.75 + 0.55 * 0.55 + 1.0).sqrt();
        let dir = Vec3::new(0.75 / n * d, 0.55 / n * d, 1.0 / n * d);
        let eye = Vec3::new(c.x + dir.x, c.y + dir.y, c.z + dir.z);
        std::fs::write(format!("{}/{}.cam", dir_name, item.slug), format!("{} {} {} {} {} {}", eye.x, eye.y, eye.z, c.x, c.y, c.z)).unwrap();
        dm.save_file(&format!("{}/{}.brixo", dir_name, item.slug)).unwrap();
        println!("{}", item.slug);
    }
}
