//! Coin Tycoon: claim a plot, drop ore onto a conveyor, sell it in the
//! furnace, buy upgrades, finish your tower.

use brixo_core::{Attribute, Class, Color, DataModel, InstanceId, Material, Shape, Vec3};

// --- scripts ---------------------------------------------------------------------

/// Helpers every plot script shares (Rovik has no imports yet).
const HELPERS: &str = r#"
-- The child of obj with this name (nil if none).
fn child(obj, name)
    for c in obj.children do
        if c.name == name then
            return c
        end
    end
    return nil
end

-- Makes every part in obj see-through by t (0 solid, 1 invisible).
fn set_look(obj, t)
    if obj.class == "part" then
        obj.transparency = t
        obj.can_collide = t < 0.5 and obj.ghost != true
    end
    for c in obj.children do
        set_look(c, t)
    end
end

-- Fades a bought item in.
fn fade_in(item)
    t = 1
    while t > 0 do
        t -= 0.125
        set_look(item, max(t, 0))
        wait(0.04)
    end
end

-- Shows or hides the floating label on a part.
fn show_label(part, shown)
    for c in part.children do
        if c.class == "textlabel" then
            c.visible = shown
        end
    end
end

fn show_pad(pad)
    pad.transparency = 0
    show_label(pad, true)
end
"#;

const GAME: &str = r#"-- Coin Tycoon: music, cash and hints for every player.
play_music("sunny")

on player_joined(p)
    p.cash = 0
    cash = create("TextLabel", p)
    cash.name = "Cash"
    cash.text = "$0"
    cash.x = 0.02
    cash.y = 0.1
    cash.width = 0.16
    cash.height = 0.08
    cash.text_size = 34
    cash.text_color = {r = 245, g = 205, b = 48}
    cash.background = true
    hint = create("TextLabel", p)
    hint.name = "Hint"
    hint.text = "Find an empty plot and step on its green pad!"
    hint.x = 0.2
    hint.y = 0.8
    hint.width = 0.6
    hint.height = 0.06
    hint.text_size = 22
    hint.background = true
end

-- Keep everyone's cash display up to date.
every 0.2 seconds
    for p in players() do
        for c in p.children do
            if c.name == "Cash" and p.cash != nil then
                c.text = "$" + p.cash
            end
        end
    end
end
"#;

const CLAIM: &str = r#"
-- Step on me to claim this plot.
on touched(other)
    plot = self.parent
    if other.class != "player" or plot.owner != nil or other.plot != nil then
        return
    end
    plot.owner = other.name
    other.plot = plot.name
    for c in child(plot, "Sign").children do
        if c.class == "textlabel" then
            c.text = other.name + "'s Tycoon"
            c.background_color = {r = 13, g = 105, b = 172}
        end
    end
    self.transparency = 1
    show_label(self, false)
    hint = child(other, "Hint")
    if hint != nil then
        hint.text = "Ore becomes cash in the furnace. Step on red pads to buy upgrades!"
    end
    play_sound("win", other)
    show_pad(child(child(plot, "Pads"), "Pad1"))
end
"#;

const PAD: &str = r#"
-- A buy pad: price, item and next pad are custom fields set in the studio.
on touched(other)
    plot = self.parent.parent
    if other.class != "player" or self.transparency > 0.5 or plot.owner != other.name then
        return
    end
    if other.cash < self.price then
        play_sound("error", other)
        return
    end
    other.cash -= self.price
    self.transparency = 1
    show_label(self, false)
    play_sound("buy", other)
    item = child(plot, self.item)
    item.bought = true
    if self.next != nil then
        show_pad(child(self.parent, self.next))
    end
    if self.item == "Tower" then
        play_sound("win")
        hint = child(other, "Hint")
        if hint != nil then
            hint.text = "You finished your tycoon! Nice work, " + other.name
        end
    end
    fade_in(item)
end
"#;

fn dropper_script(interval: f32) -> String {
    format!(
        r#"
-- Drops ore onto the belt, once bought and the plot has an owner.
every {interval} seconds
    dropper = self
    plot = self.parent
    if dropper.bought and plot.owner != nil then
        spout = child(dropper, "Spout")
        ore = create("Part", child(plot, "Ores"))
        ore.name = "Ore"
        s = dropper.ore_size
        ore.size = {{x = s, y = s, z = s}}
        ore.position = {{x = spout.position.x, y = spout.position.y - 1.2, z = spout.position.z}}
        if dropper.gold then
            ore.color = {{r = 245, g = 205, b = 48}}
            ore.material = "neon"
        else
            ore.color = {{r = 150, g = 150, b = 155}}
            ore.material = "concrete"
        end
        ore.anchored = false
        ore.value = dropper.value
        ore.born = time()
    end
end
"#
    )
}

const FURNACE: &str = r#"
-- Ore that falls in here becomes cash for the plot's owner.
on touched(other)
    if other.name == "Ore" then
        owner = find(self.parent.parent.owner)
        if owner != nil and other.value != nil then
            owner.cash += other.value
            -- A soft pop for everyday ore; the full coin sound for gold.
            if other.value >= 50 then
                play_sound("coin", owner)
            else
                play_sound("pop", owner)
            end
        end
        destroy(other)
    end
end
"#;

const UPGRADER: &str = r#"
-- Doubles the value of ore that passes through (once each).
on touched(other)
    if other.name == "Ore" and other.upgraded != true and self.parent.bought then
        other.value = other.value * 2
        other.upgraded = true
        other.color = {r = 60, g = 170, b = 255}
        other.material = "neon"
    end
end
"#;

const CLEANUP: &str = r#"
-- Ore that got stuck somewhere is cleared away after a while.
every 5 seconds
    for o in self.children do
        if o.born != nil and time() - o.born > 40 then
            destroy(o)
        end
    end
end
"#;

// --- building ----------------------------------------------------------------------

struct Builder {
    dm: DataModel,
}

type Rgb = (u8, u8, u8);

impl Builder {
    #[allow(clippy::too_many_arguments)]
    fn part(&mut self, parent: InstanceId, name: &str, pos: Vec3, size: Vec3, color: Rgb, material: Material, shape: Shape) -> InstanceId {
        let id = self.dm.create(Class::Part, name, parent).unwrap();
        let p = self.dm.part_mut(id).unwrap();
        p.position = pos;
        p.size = size;
        p.color = Color::new(color.0, color.1, color.2);
        p.material = material;
        p.shape = shape;
        id
    }

    fn block(&mut self, parent: InstanceId, name: &str, pos: Vec3, size: Vec3, color: Rgb, material: Material) -> InstanceId {
        self.part(parent, name, pos, size, color, material, Shape::Block)
    }

    fn script(&mut self, parent: InstanceId, name: &str, source: &str) {
        let s = self.dm.create(Class::Script, name, parent).unwrap();
        self.dm.script_mut(s).unwrap().source = source.trim_start().to_string();
    }

    fn attr(&mut self, id: InstanceId, key: &str, value: Attribute) {
        self.dm.get_mut(id).unwrap().attributes.insert(key.to_string(), value);
    }

    /// A label floating over `part`.
    fn tag(&mut self, part: InstanceId, text: &str, bg: Rgb, visible: bool) -> InstanceId {
        let id = self.dm.create(Class::TextLabel, "Tag", part).unwrap();
        let g = self.dm.gui_mut(id).unwrap();
        g.text = text.to_string();
        g.attached_to = Some(part);
        g.x = 0.0;
        g.y = 0.0;
        g.width = 0.17;
        g.height = 0.05;
        g.text_size = 18.0;
        g.background = true;
        g.background_color = Color::new(bg.0, bg.1, bg.2);
        g.visible = visible;
        id
    }

    /// Makes every part under `id` invisible and non-solid (not bought yet).
    fn unbought(&mut self, id: InstanceId) {
        for p in self.dm.parts_under(id) {
            let part = self.dm.part_mut(p).unwrap();
            part.transparency = 1.0;
            part.can_collide = false;
        }
        self.attr(id, "bought", Attribute::Bool(false));
    }

    fn tree(&mut self, parent: InstanceId, x: f32, z: f32, height: f32) {
        let t = self.dm.create(Class::Model, "Tree", parent).unwrap();
        self.part(t, "Trunk", Vec3::new(x, height / 2.0, z), Vec3::new(1.6, height, 1.6), (105, 64, 40), Material::Wood, Shape::Cylinder);
        self.part(t, "Leaves", Vec3::new(x, height + 1.5, z), Vec3::new(6.0, 6.0, 6.0), (75, 151, 75), Material::Grass, Shape::Ball);
    }

    fn lamp(&mut self, parent: InstanceId, x: f32, z: f32) {
        let l = self.dm.create(Class::Model, "Lamp", parent).unwrap();
        self.part(l, "Post", Vec3::new(x, 3.0, z), Vec3::new(0.6, 6.0, 0.6), (27, 42, 53), Material::Metal, Shape::Cylinder);
        self.part(l, "Light", Vec3::new(x, 6.5, z), Vec3::new(1.4, 1.4, 1.4), (255, 230, 160), Material::Neon, Shape::Ball);
    }
}

/// One plot, at (px, pz). `f` mirrors it so its entrance faces the plaza.
fn plot(b: &mut Builder, root: InstanceId, name: &str, px: f32, pz: f32) {
    let f = if pz > 0.0 { 1.0 } else { -1.0 };
    let at = |lx: f32, ly: f32, lz: f32| Vec3::new(px + lx, ly, pz + f * lz);
    let plot = b.dm.create(Class::Folder, name, root).unwrap();

    b.block(plot, "Floor", at(0.0, 0.2, 0.0), Vec3::new(36.0, 0.4, 36.0), (99, 95, 98), Material::Concrete);
    let ores = b.dm.create(Class::Folder, "Ores", plot).unwrap();
    b.script(ores, "Cleanup", CLEANUP);

    // The sign, with the owner's name over it.
    let sign = b.dm.create(Class::Model, "Sign", plot).unwrap();
    b.block(sign, "Post", at(-11.0, 2.0, -17.0), Vec3::new(0.6, 4.0, 0.6), (105, 64, 40), Material::Wood);
    let board = b.block(sign, "Board", at(-11.0, 4.2, -17.0), Vec3::new(5.0, 2.0, 0.4), (218, 133, 65), Material::Wood);
    b.tag(board, "Empty plot", (60, 60, 66), true);
    b.dm.reparent(b.dm.get(board).unwrap().children[0], sign); // the tag belongs to the sign
    // The claim pad.
    let claim = b.block(plot, "ClaimPad", at(0.0, 0.55, -14.0), Vec3::new(5.0, 0.3, 5.0), (75, 200, 75), Material::Neon);
    b.dm.part_mut(claim).unwrap().can_collide = false;
    b.tag(claim, "Claim this plot!", (39, 110, 45), true);
    b.script(claim, "Claim", &format!("{HELPERS}{CLAIM}"));

    // The conveyor: a metal belt with rails, carrying ore toward the furnace.
    let belt = b.block(plot, "Belt", at(0.0, 1.0, 6.0), Vec3::new(28.0, 1.0, 4.0), (60, 64, 72), Material::Metal);
    b.dm.part_mut(belt).unwrap().velocity = Vec3::new(8.0, 0.0, 0.0);
    for side in [-2.2, 2.2] {
        b.block(plot, "Rail", at(0.0, 2.0, 6.0 + side), Vec3::new(28.0, 1.0, 0.4), (27, 42, 53), Material::Metal);
    }
    // The furnace: ore that reaches it is sold.
    let furnace = b.dm.create(Class::Model, "Furnace", plot).unwrap();
    b.block(furnace, "Body", at(17.0, 3.0, 6.0), Vec3::new(4.0, 6.0, 7.0), (140, 60, 40), Material::Brick);
    let intake = b.block(furnace, "Intake", at(14.6, 2.4, 6.0), Vec3::new(0.8, 2.4, 4.0), (255, 120, 20), Material::Neon);
    b.dm.part_mut(intake).unwrap().can_collide = false;
    b.script(intake, "Sell", FURNACE);

    // Droppers: (name, x, interval, ore value, ore size, gold, bought).
    let droppers = [
        ("Dropper1", -11.0, 2.0, 5.0, 1.2, false, true),
        ("Dropper2", -7.0, 2.0, 5.0, 1.2, false, false),
        ("Dropper3", -3.0, 2.0, 5.0, 1.2, false, false),
        ("MegaDropper", 1.5, 2.5, 50.0, 1.8, true, false),
    ];
    for (dname, x, interval, value, size, gold, bought) in droppers {
        let d = b.dm.create(Class::Model, dname, plot).unwrap();
        let (bw, color) = if gold { (4.0, (245, 205, 48)) } else { (3.0, (120, 124, 132)) };
        // High enough that stacked ore (and big gold ore) passes underneath.
        b.block(d, "Box", at(x, 8.5, 6.0), Vec3::new(bw, 3.0, bw), color, Material::Metal);
        let spout = b.block(d, "Spout", at(x, 6.6, 6.0), Vec3::new(1.4, 0.8, 1.4), (255, 120, 20), Material::Neon);
        for dz in [-1.0, 1.0] {
            b.block(d, "Leg", at(x, 5.0, 6.0 + dz * 2.6), Vec3::new(0.5, 10.0, 0.5), (27, 42, 53), Material::Metal);
        }
        b.script(d, "Drop", &format!("{HELPERS}{}", dropper_script(interval)));
        b.attr(d, "value", Attribute::Num(value));
        b.attr(d, "ore_size", Attribute::Num(size));
        b.attr(d, "gold", Attribute::Bool(gold));
        if bought {
            b.attr(d, "bought", Attribute::Bool(true));
        } else {
            b.unbought(d);
        }
        // The spout is never solid, even after buying fades it in.
        b.dm.part_mut(spout).unwrap().can_collide = false;
        b.attr(spout, "ghost", Attribute::Bool(true));
    }
    // The upgrader arch.
    let up = b.dm.create(Class::Model, "Upgrader", plot).unwrap();
    let arch = b.block(up, "Arch", at(7.0, 2.6, 6.0), Vec3::new(0.8, 3.0, 4.2), (60, 170, 255), Material::Neon);
    for dz in [-2.8, 2.8] {
        b.block(up, "Pillar", at(7.0, 2.5, 6.0 + dz), Vec3::new(1.0, 5.0, 1.0), (27, 42, 53), Material::Metal);
    }
    b.block(up, "Top", at(7.0, 5.3, 6.0), Vec3::new(1.2, 0.6, 6.6), (27, 42, 53), Material::Metal);
    b.script(arch, "Upgrade", UPGRADER);
    b.unbought(up);
    b.attr(arch, "ghost", Attribute::Bool(true));
    // Walls around the back and sides.
    let walls = b.dm.create(Class::Model, "Walls", plot).unwrap();
    b.block(walls, "Back", at(0.0, 3.4, 17.8), Vec3::new(36.0, 6.0, 0.8), (170, 80, 60), Material::Brick);
    for sx in [-17.8, 17.8] {
        b.block(walls, "Side", at(sx, 3.4, 4.0), Vec3::new(0.8, 6.0, 28.0), (170, 80, 60), Material::Brick);
    }
    b.unbought(walls);
    // The tower: the goal.
    let tower = b.dm.create(Class::Model, "Tower", plot).unwrap();
    b.block(tower, "Base", at(-12.0, 8.4, 13.0), Vec3::new(5.0, 16.0, 5.0), (230, 225, 215), Material::Brick);
    b.part(tower, "Roof", at(-12.0, 17.4, 13.0), Vec3::new(6.0, 2.0, 6.0), (27, 42, 53), Material::Metal, Shape::Cylinder);
    b.part(tower, "Crown", at(-12.0, 20.0, 13.0), Vec3::new(3.0, 3.0, 3.0), (245, 205, 48), Material::Neon, Shape::Ball);
    b.unbought(tower);
    // Silence the ghost part's collision after unbought() touched it.
    b.dm.part_mut(arch).unwrap().can_collide = false;

    // Buy pads, in order. Each shows the next when bought.
    let pads = b.dm.create(Class::Folder, "Pads", plot).unwrap();
    let list = [
        ("Dropper 2", "Dropper2", 25.0),
        ("Upgrader (x2 ore)", "Upgrader", 60.0),
        ("Dropper 3", "Dropper3", 150.0),
        ("Brick Walls", "Walls", 250.0),
        ("Mega Dropper", "MegaDropper", 500.0),
        ("Tower (finish!)", "Tower", 1500.0),
    ];
    for (i, (label, item, price)) in list.iter().enumerate() {
        let pad = b.block(pads, &format!("Pad{}", i + 1), at(-12.5 + i as f32 * 5.0, 0.55, -8.0), Vec3::new(4.0, 0.3, 4.0), (196, 40, 28), Material::Neon);
        let p = b.dm.part_mut(pad).unwrap();
        p.can_collide = false;
        p.transparency = 1.0;
        b.tag(pad, &format!("{label}  ${price}"), (120, 30, 25), false);
        b.script(pad, "Buy", &format!("{HELPERS}{PAD}"));
        b.attr(pad, "price", Attribute::Num(*price));
        b.attr(pad, "item", Attribute::Str(item.to_string()));
        if i + 1 < list.len() {
            b.attr(pad, "next", Attribute::Str(format!("Pad{}", i + 2)));
        }
    }
}

/// The whole game.
pub fn coin_tycoon() -> DataModel {
    let mut b = Builder { dm: DataModel::new() };
    let root = b.dm.root();

    b.block(root, "Island", Vec3::new(0.0, -1.0, 0.0), Vec3::new(160.0, 2.0, 140.0), (75, 151, 75), Material::Grass);
    b.block(root, "Plaza", Vec3::new(0.0, 0.1, 0.0), Vec3::new(40.0, 0.2, 40.0), (180, 176, 170), Material::Concrete);
    let spawn = b.dm.create(Class::SpawnLocation, "SpawnLocation", root).unwrap();
    {
        let p = b.dm.part_mut(spawn).unwrap();
        p.position = Vec3::new(0.0, 0.7, 0.0);
        p.size = Vec3::new(8.0, 1.0, 8.0);
        p.color = Color::new(13, 105, 172);
        p.material = Material::Metal;
    }
    // Paths to each plot.
    for x in [-22.0, 22.0] {
        b.block(root, "Path", Vec3::new(x, 0.08, 0.0), Vec3::new(6.0, 0.16, 44.0), (160, 156, 150), Material::Concrete);
    }
    let decor = b.dm.create(Class::Folder, "Decor", root).unwrap();
    for (x, z) in [(-14.0, -14.0), (14.0, -14.0), (-14.0, 14.0), (14.0, 14.0)] {
        b.lamp(decor, x, z);
    }
    for (x, z, h) in [(-70.0, 0.0, 7.0), (70.0, 5.0, 8.0), (-60.0, -62.0, 6.0), (62.0, 62.0, 7.0), (0.0, 63.0, 6.5), (5.0, -64.0, 7.5), (-66.0, 60.0, 8.0), (66.0, -58.0, 6.0)] {
        b.tree(decor, x, z, h);
    }

    let title = b.dm.create(Class::TextLabel, "Title", root).unwrap();
    {
        let g = b.dm.gui_mut(title).unwrap();
        g.text = "COIN TYCOON".into();
        g.x = 0.35;
        g.y = 0.015;
        g.width = 0.3;
        g.height = 0.06;
        g.text_size = 30.0;
        g.text_color = Color::new(245, 205, 48);
    }
    b.script(root, "Game", GAME);

    plot(&mut b, root, "PlotA", -22.0, 40.0);
    plot(&mut b, root, "PlotB", 22.0, 40.0);
    plot(&mut b, root, "PlotC", -22.0, -40.0);
    plot(&mut b, root, "PlotD", 22.0, -40.0);
    b.dm
}
