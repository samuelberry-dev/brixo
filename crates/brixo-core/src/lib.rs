use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Unique, never-reused identifier for an instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct InstanceId(u64);

impl InstanceId {
    /// The raw number, for handing to scripts.
    pub fn raw(self) -> u64 {
        self.0
    }

    pub fn from_raw(raw: u64) -> Self {
        Self(raw)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vec3 {
    pub const ZERO: Vec3 = Vec3 {
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };
    pub const ONE: Vec3 = Vec3 {
        x: 1.0,
        y: 1.0,
        z: 1.0,
    };

    pub fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }
}

/// 8-bit RGB colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Color {
    pub fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }
}

/// A part's shape. Every shape fills its size box: a Wedge is a ramp
/// rising toward +Z, a Cylinder stands upright (along Y), a Ball is round.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Shape {
    #[default]
    Block,
    Wedge,
    Cylinder,
    Ball,
}

impl Shape {
    pub const ALL: [Shape; 4] = [Shape::Block, Shape::Wedge, Shape::Cylinder, Shape::Ball];

    /// The name scripts use: `self.shape = "ball"`.
    pub fn name(self) -> &'static str {
        match self {
            Shape::Block => "block",
            Shape::Wedge => "wedge",
            Shape::Cylinder => "cylinder",
            Shape::Ball => "ball",
        }
    }

    pub fn from_name(name: &str) -> Option<Shape> {
        Shape::ALL.into_iter().find(|s| s.name() == name)
    }
}

/// Which of a part's own lines a hinge turns it around: its height (Y),
/// its width (X) or its depth (Z). A door turns around its height; a
/// cylinder wheel too (a cylinder's height is its axle).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Hinge {
    #[default]
    Off,
    Y,
    X,
    Z,
}

impl Hinge {
    pub const ALL: [Hinge; 4] = [Hinge::Off, Hinge::Y, Hinge::X, Hinge::Z];

    /// The name scripts use: `door.hinge = "y"`.
    pub fn name(self) -> &'static str {
        match self {
            Hinge::Off => "off",
            Hinge::Y => "y",
            Hinge::X => "x",
            Hinge::Z => "z",
        }
    }

    pub fn from_name(name: &str) -> Option<Hinge> {
        Hinge::ALL.into_iter().find(|h| h.name() == name)
    }

    /// What Studio calls it.
    pub fn title(self) -> &'static str {
        match self {
            Hinge::Off => "Off",
            Hinge::Y => "Its height (Y)",
            Hinge::X => "Its width (X)",
            Hinge::Z => "Its depth (Z)",
        }
    }

    /// The line in the part's own space, or None when off.
    pub fn axis(self) -> Option<Vec3> {
        match self {
            Hinge::Off => None,
            Hinge::Y => Some(Vec3::new(0.0, 1.0, 0.0)),
            Hinge::X => Some(Vec3::new(1.0, 0.0, 0.0)),
            Hinge::Z => Some(Vec3::new(0.0, 0.0, 1.0)),
        }
    }
}

/// Where on a part its hinge is: the middle, or the middle of one side
/// (left is -X, right +X, bottom -Y, top +Y, back -Z, front +Z).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Side {
    #[default]
    Middle,
    Left,
    Right,
    Top,
    Bottom,
    Front,
    Back,
}

impl Side {
    pub const ALL: [Side; 7] = [Side::Middle, Side::Left, Side::Right, Side::Top, Side::Bottom, Side::Front, Side::Back];

    pub fn name(self) -> &'static str {
        match self {
            Side::Middle => "middle",
            Side::Left => "left",
            Side::Right => "right",
            Side::Top => "top",
            Side::Bottom => "bottom",
            Side::Front => "front",
            Side::Back => "back",
        }
    }

    pub fn from_name(name: &str) -> Option<Side> {
        Side::ALL.into_iter().find(|s| s.name() == name)
    }

    /// The point, in the part's own space, for a part of this size.
    pub fn point(self, size: Vec3) -> Vec3 {
        let (x, y, z) = (size.x / 2.0, size.y / 2.0, size.z / 2.0);
        match self {
            Side::Middle => Vec3::ZERO,
            Side::Left => Vec3::new(-x, 0.0, 0.0),
            Side::Right => Vec3::new(x, 0.0, 0.0),
            Side::Top => Vec3::new(0.0, y, 0.0),
            Side::Bottom => Vec3::new(0.0, -y, 0.0),
            Side::Front => Vec3::new(0.0, 0.0, z),
            Side::Back => Vec3::new(0.0, 0.0, -z),
        }
    }
}

/// The Workspace field that lists the leaderboard's columns (player fields,
/// comma separated), set by `leaderboard("coins", "wins")`.
pub const LEADERBOARD_FIELD: &str = "leaderboard";

/// The player fields the game shows on its leaderboard, in order.
pub fn leaderboard_columns(world: &DataModel) -> Vec<String> {
    match world.get(world.root()).and_then(|w| w.attributes.get(LEADERBOARD_FIELD)) {
        Some(Attribute::Str(list)) => list.split(',').map(str::trim).filter(|c| !c.is_empty()).map(String::from).collect(),
        _ => Vec::new(),
    }
}

/// How a column's field name reads as a heading: "best_time" is "Best
/// Time"; short words are abbreviations, so "xp" is "XP" and "kos" "KOs".
pub fn leaderboard_title(field: &str) -> String {
    field
        .split('_')
        .filter(|w| !w.is_empty())
        .map(|w| {
            // Two letters (with a plural s): "xp", "hp", "kos".
            let stem = if w.len() == 3 { w.strip_suffix('s') } else { None };
            if w.chars().all(|c| c.is_ascii_alphabetic()) && (w.len() <= 2 || stem.is_some()) {
                return match stem {
                    Some(stem) => stem.to_uppercase() + "s",
                    None => w.to_uppercase(),
                };
            }
            let mut c = w.chars();
            c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// A game's lighting: the time of day (which moves the sun, and colours
/// the sky from dawn to night), how bright it is, fog, and the sky's
/// colour. Kept as fields on the Workspace, so it's saved with the game,
/// reaches every player, and scripts change it:
/// `find("Workspace").time_of_day = 19`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Lighting {
    /// Hours, 0 to 24: 6 is sunrise, 12 noon, 18 sunset. 14 by default.
    pub time_of_day: f32,
    /// 1 is normal; 0.5 gloomy; 2 dazzling.
    pub brightness: f32,
    /// Fog: things fade into `fog_color` from `fog_start` studs away until
    /// they're gone at `fog_end`. A `fog_end` of 0 is no fog.
    pub fog_start: f32,
    pub fog_end: f32,
    pub fog_color: Color,
    /// The sky's colour overhead, instead of the time of day's.
    pub sky_color: Option<Color>,
}

impl Default for Lighting {
    fn default() -> Self {
        Lighting { time_of_day: 14.0, brightness: 1.0, fog_start: 0.0, fog_end: 0.0, fog_color: Color::new(192, 204, 218), sky_color: None }
    }
}

impl Lighting {
    /// The Workspace fields it's kept in.
    pub const FIELDS: [&'static str; 6] = ["time_of_day", "brightness", "fog_start", "fog_end", "fog_color", "sky_color"];

    pub fn of(world: &DataModel) -> Lighting {
        let mut l = Lighting::default();
        let Some(ws) = world.get(world.root()) else { return l };
        let num = |k: &str| match ws.attributes.get(k) {
            Some(Attribute::Num(n)) if n.is_finite() => Some(*n as f32),
            _ => None,
        };
        let color = |k: &str| match ws.attributes.get(k) {
            Some(Attribute::Str(s)) => color_from_text(s),
            _ => None,
        };
        if let Some(t) = num("time_of_day") {
            l.time_of_day = t.rem_euclid(24.0);
        }
        if let Some(b) = num("brightness") {
            l.brightness = b.clamp(0.0, 3.0);
        }
        if let Some(f) = num("fog_start") {
            l.fog_start = f.max(0.0);
        }
        if let Some(f) = num("fog_end") {
            l.fog_end = f.max(0.0);
        }
        if let Some(c) = color("fog_color") {
            l.fog_color = c;
        }
        l.sky_color = color("sky_color");
        l
    }

    /// Writes it into the Workspace's fields (leaving out what's default,
    /// so a game that never touches lighting has no fields for it).
    pub fn set(&self, world: &mut DataModel) {
        let root = world.root();
        let Some(ws) = world.get_mut(root) else { return };
        let d = Lighting::default();
        let a = &mut ws.attributes;
        let mut put = |k: &str, v: Option<Attribute>| match v {
            Some(v) => {
                a.insert(k.into(), v);
            }
            None => {
                a.remove(k);
            }
        };
        let n = |v: f32, d: f32| (v != d).then(|| Attribute::Num((v as f64 * 1000.0).round() / 1000.0));
        put("time_of_day", n(self.time_of_day, d.time_of_day));
        put("brightness", n(self.brightness, d.brightness));
        put("fog_start", n(self.fog_start, d.fog_start));
        put("fog_end", n(self.fog_end, d.fog_end));
        put("fog_color", (self.fog_color != d.fog_color).then(|| Attribute::Str(color_text(self.fog_color))));
        put("sky_color", self.sky_color.map(|c| Attribute::Str(color_text(c))));
    }
}

/// A colour as a Workspace field holds it: "r,g,b".
pub fn color_text(c: Color) -> String {
    format!("{},{},{}", c.r, c.g, c.b)
}

pub fn color_from_text(s: &str) -> Option<Color> {
    let parts: Vec<u8> = s.split(',').filter_map(|p| p.trim().parse().ok()).collect();
    (parts.len() == 3).then(|| Color::new(parts[0], parts[1], parts[2]))
}

/// Karts: a Model with the custom field `kart = true`. Its **Chassis**
/// (the part called "Chassis", or else its first part) is what the engine
/// drives; the rest of its parts ride along, posed from it (see
/// `pose_kart_parts`). A part called "Seat" is where the driver sits.
pub const KART_FIELD: &str = "kart";

/// Whether `id` is a kart.
pub fn is_kart(world: &DataModel, id: InstanceId) -> bool {
    world.get(id).is_some_and(|i| i.class == Class::Model && matches!(i.attributes.get(KART_FIELD), Some(Attribute::Bool(true))))
}

/// The kart a part belongs to (its Model, if that's a kart).
pub fn kart_of(world: &DataModel, part: InstanceId) -> Option<InstanceId> {
    let parent = world.get(part)?.parent?;
    is_kart(world, parent).then_some(parent)
}

/// A kart's chassis: the part called "Chassis", or its first part.
pub fn kart_chassis(world: &DataModel, kart: InstanceId) -> Option<InstanceId> {
    let kids = &world.get(kart)?.children;
    kids.iter().copied().find(|c| world.part(*c).is_some() && world.get(*c).is_some_and(|i| i.name == "Chassis"))
        .or_else(|| kids.iter().copied().find(|c| world.part(*c).is_some()))
}

/// Where a kart part sits on its chassis: the custom fields `kart_at`
/// ("x,y,z" in the chassis's own space) and `kart_turn` (its own turn,
/// "x,y,z" degrees, as a quaternion's x,y,z,w in `kart_q`). Recorded by the
/// engine the first time it sees the kart, so every copy of the world
/// (server and players) poses the parts the same way.
pub const KART_AT: &str = "kart_at";
pub const KART_Q: &str = "kart_q";

/// A player's `camera_part` (scripts set it for a cutscene): the player's
/// camera sits at that part, looking the way its front faces. Kept as the
/// part's id in a custom field.
pub const CAMERA_PART: &str = "camera_part";

/// The part a player's camera is fixed to, if a script has set one.
pub fn camera_part(world: &DataModel, player: InstanceId) -> Option<InstanceId> {
    match world.get(player)?.attributes.get(CAMERA_PART) {
        Some(Attribute::Num(n)) => Some(InstanceId::from_raw(*n as u64)).filter(|p| world.part(*p).is_some()),
        _ => None,
    }
}

/// Where a character's right shoulder is, in its own space (facing +Z).
pub const RIGHT_SHOULDER: Vec3 = Vec3 { x: -1.43, y: 0.97, z: 0.0 };
/// From the shoulder to the middle of the hand.
pub const ARM_REACH: f32 = 1.6;
/// How long a tool swing lasts, in seconds.
pub const SWING_TIME: f32 = 0.3;

/// Whether a tool is held straight up (its `grip` is "up", like a sword)
/// rather than out in front.
pub fn holds_up(world: &DataModel, tool: InstanceId) -> bool {
    world.get(tool).is_some_and(|t| matches!(t.attributes.get("grip"), Some(Attribute::Str(g)) if g == "up"))
}

/// The right arm's angle while holding a tool, in radians about the
/// shoulder's side axis: 0 hangs down, -PI/2 points forward, -PI points
/// straight up. `swing` is the seconds left in a swing (see SWING_TIME).
/// A sword is held straight up and chops forward and back; anything else is
/// held forward and kicks back a little. The renderer draws the arm with
/// this and the runtime places the tool with it, so they always line up.
pub fn held_arm_angle(swing: f32, up: bool) -> f32 {
    use std::f32::consts::{FRAC_PI_2, PI};
    let chop = if swing > 0.0 { ((1.0 - swing / SWING_TIME).clamp(0.0, 1.0) * PI).sin() } else { 0.0 };
    if up { -PI + (FRAC_PI_2 + 0.25) * chop } else { -FRAC_PI_2 - 0.35 * chop }
}

/// Brixo's built-in sound effects: `play_sound("coin")`.
pub const SOUNDS: [&str; 16] = [
    "coin", "cash", "buy", "click", "error", "pop", "jump", "hit", "win", "whoosh", "death", "boom", "twang", "bonk",
    "splat", "thud",
];
/// Brixo's built-in music: `play_music("sunny")`.
pub const MUSIC: [&str; 2] = ["sunny", "rush"];

/// What a part's surface looks like: a pixel-art texture tinted by the
/// part's colour. Neon glows (ignores shadows and shading).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Material {
    #[default]
    Plastic,
    Wood,
    Brick,
    Metal,
    Grass,
    Concrete,
    Neon,
}

impl Material {
    pub const ALL: [Material; 7] = [
        Material::Plastic,
        Material::Wood,
        Material::Brick,
        Material::Metal,
        Material::Grass,
        Material::Concrete,
        Material::Neon,
    ];

    /// The name scripts use: `self.material = "wood"`.
    pub fn name(self) -> &'static str {
        match self {
            Material::Plastic => "plastic",
            Material::Wood => "wood",
            Material::Brick => "brick",
            Material::Metal => "metal",
            Material::Grass => "grass",
            Material::Concrete => "concrete",
            Material::Neon => "neon",
        }
    }

    pub fn from_name(name: &str) -> Option<Material> {
        Material::ALL.into_iter().find(|m| m.name() == name)
    }
}

/// A value scripts store in a custom field (`player.cash = 100`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Attribute {
    Num(f64),
    Str(String),
    Bool(bool),
}

/// Properties that only a Part has.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PartProps {
    pub position: Vec3,
    pub size: Vec3,
    /// Euler angles in degrees.
    pub rotation: Vec3,
    pub color: Color,
    /// Anchored parts stay put during play (scripts can still move them).
    /// Unanchored parts fall, collide and tumble. Saves from before physics
    /// have no value here and load as anchored, so old scenes don't collapse.
    #[serde(default = "yes")]
    pub anchored: bool,
    /// Whether other parts bump into this one. Parts that can't collide
    /// still fire `on touched`, so they work as pickups and trigger zones.
    #[serde(default = "yes")]
    pub can_collide: bool,
    /// Older saves have no shape: they're all blocks.
    #[serde(default)]
    pub shape: Shape,
    #[serde(default)]
    pub material: Material,
    /// 0 is solid, 1 is invisible. In between is drawn as a retro dither.
    #[serde(default)]
    pub transparency: f32,
    /// For loose parts: how fast they're moving (scripts can set it to
    /// launch them). For anchored parts: a conveyor belt, carrying whatever
    /// rests on top along at this speed.
    #[serde(default)]
    pub velocity: Vec3,
    /// Loose parts that ignore gravity (a rocket flies straight).
    #[serde(default)]
    pub floating: bool,
    /// How bouncy, 0 (a thud) to 1 (a superball).
    #[serde(default)]
    pub bounce: f32,
    /// A hinge: the (loose) part turns around this line of its own,
    /// held to whatever it's touching nearest `hinge_at` (or to the spot
    /// it's in, if it touches nothing).
    #[serde(default)]
    pub hinge: Hinge,
    #[serde(default)]
    pub hinge_at: Side,
    /// A motor on the hinge: keeps turning at this many degrees a second
    /// (a wheel, a spinner). 0 swings freely.
    #[serde(default)]
    pub motor_speed: f32,
    /// Swings to this angle (degrees, from where it started) and holds
    /// there (a door opening, a drawbridge). None swings freely.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub swing_to: Option<f32>,
}

fn yes() -> bool {
    true
}

impl Default for PartProps {
    fn default() -> Self {
        Self {
            position: Vec3::ZERO,
            size: Vec3::ONE,
            rotation: Vec3::ZERO,
            color: Color::new(160, 160, 160),
            anchored: true,
            can_collide: true,
            shape: Shape::Block,
            material: Material::Plastic,
            transparency: 0.0,
            velocity: Vec3::ZERO,
            floating: false,
            bounce: 0.0,
            hinge: Hinge::Off,
            hinge_at: Side::Middle,
            motor_speed: 0.0,
            swing_to: None,
        }
    }
}

/// A Rovik script. It runs when the game plays; `self` is its parent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScriptProps {
    pub source: String,
    #[serde(default = "yes")]
    pub enabled: bool,
}

impl Default for ScriptProps {
    fn default() -> Self {
        Self {
            source: "-- This script runs when you press Play.\n-- 'self' is the part it's inside.\n\nprint(\"Hello from \" + self.name)\n".to_string(),
            enabled: true,
        }
    }
}

/// A sound file in a game (mp3, wav or ogg), made by dropping the file on
/// Brixo Studio and played with `play_sound(find("Name"))`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SoundProps {
    /// "mp3", "wav" or "ogg".
    pub format: String,
    /// The file itself, base64-encoded, so games stay plain JSON. (Shared,
    /// so copying a world, which players do every frame, doesn't copy
    /// every song in it.)
    pub data: std::sync::Arc<str>,
    /// 0 to 1.
    pub volume: f32,
}

impl SoundProps {
    pub fn from_bytes(format: &str, bytes: &[u8]) -> SoundProps {
        use base64::Engine;
        SoundProps { format: format.to_string(), data: base64::engine::general_purpose::STANDARD.encode(bytes).into(), volume: 0.8 }
    }

    /// The file's bytes (None if the data is missing or damaged).
    pub fn bytes(&self) -> Option<Vec<u8>> {
        use base64::Engine;
        base64::engine::general_purpose::STANDARD.decode(self.data.as_bytes()).ok().filter(|b| !b.is_empty())
    }
}

/// A piece of on-screen interface: a TextLabel, TextButton or Frame.
/// Positions and sizes are fractions of the screen (0 to 1), from the
/// top-left, so a GUI looks the same on any window size.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GuiProps {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub text: String,
    /// In points.
    pub text_size: f32,
    pub text_color: Color,
    pub background_color: Color,
    /// Whether the box behind the text is drawn.
    pub background: bool,
    pub visible: bool,
    /// Float over this part instead of sitting on the screen: price tags,
    /// name tags. `x`/`y` then nudge it in screen fractions.
    #[serde(default)]
    pub attached_to: Option<InstanceId>,
}

impl GuiProps {
    fn for_class(class: Class) -> Self {
        let base = GuiProps {
            x: 0.4,
            y: 0.1,
            width: 0.2,
            height: 0.06,
            text: String::new(),
            text_size: 20.0,
            text_color: Color::new(255, 255, 255),
            background_color: Color::new(27, 42, 53),
            background: true,
            visible: true,
            attached_to: None,
        };
        match class {
            Class::TextLabel => GuiProps { text: "Label".into(), background: false, ..base },
            Class::TextButton => GuiProps { text: "Button".into(), background_color: Color::new(13, 105, 172), ..base },
            _ => GuiProps { width: 0.3, height: 0.2, ..base },
        }
    }
}

/// The expression drawn on a player's face panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Face {
    #[default]
    Smile,
    Happy,
    Surprised,
    Determined,
}

impl Face {
    pub const ALL: [Face; 4] = [Face::Smile, Face::Happy, Face::Surprised, Face::Determined];

    /// The name scripts use: `player.face = "surprised"`.
    pub fn name(self) -> &'static str {
        match self {
            Face::Smile => "smile",
            Face::Happy => "happy",
            Face::Surprised => "surprised",
            Face::Determined => "determined",
        }
    }

    pub fn from_name(name: &str) -> Option<Face> {
        Face::ALL.into_iter().find(|f| f.name() == name)
    }
}

/// A hat a player wears. Players can wear up to MAX_HATS at once; each one
/// sits on the head in its own spot, and it's fine if two overlap.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Hat {
    Cap,
    Beanie,
    TopHat,
    CowboyHat,
    Crown,
    Headphones,
    PartyHat,
    ChefHat,
    VikingHelmet,
    HardHat,
    PropellerCap,
    Halo,
    TrafficCone,
}

/// How many hats a player can wear at once.
pub const MAX_HATS: usize = 3;

impl Hat {
    pub const ALL: [Hat; 13] = [
        Hat::Cap,
        Hat::Beanie,
        Hat::TopHat,
        Hat::CowboyHat,
        Hat::Crown,
        Hat::Headphones,
        Hat::PartyHat,
        Hat::ChefHat,
        Hat::VikingHelmet,
        Hat::HardHat,
        Hat::PropellerCap,
        Hat::Halo,
        Hat::TrafficCone,
    ];

    /// The name the website and saved avatars use.
    pub fn name(self) -> &'static str {
        match self {
            Hat::Cap => "cap",
            Hat::Beanie => "beanie",
            Hat::TopHat => "top_hat",
            Hat::CowboyHat => "cowboy_hat",
            Hat::Crown => "crown",
            Hat::Headphones => "headphones",
            Hat::PartyHat => "party_hat",
            Hat::ChefHat => "chef_hat",
            Hat::VikingHelmet => "viking_helmet",
            Hat::HardHat => "hard_hat",
            Hat::PropellerCap => "propeller_cap",
            Hat::Halo => "halo",
            Hat::TrafficCone => "traffic_cone",
        }
    }

    /// What people see: "Top Hat".
    pub fn title(self) -> &'static str {
        match self {
            Hat::Cap => "Baseball Cap",
            Hat::Beanie => "Beanie",
            Hat::TopHat => "Top Hat",
            Hat::CowboyHat => "Cowboy Hat",
            Hat::Crown => "Crown",
            Hat::Headphones => "Headphones",
            Hat::PartyHat => "Party Hat",
            Hat::ChefHat => "Chef Hat",
            Hat::VikingHelmet => "Viking Helmet",
            Hat::HardHat => "Hard Hat",
            Hat::PropellerCap => "Propeller Cap",
            Hat::Halo => "Halo",
            Hat::TrafficCone => "Traffic Cone",
        }
    }

    pub fn from_name(name: &str) -> Option<Hat> {
        Hat::ALL.into_iter().find(|h| h.name() == name)
    }

    /// Up to MAX_HATS known hats from a list of names, skipping unknown
    /// names and repeats.
    pub fn list(names: &[String]) -> [Option<Hat>; MAX_HATS] {
        let mut out = [None; MAX_HATS];
        let mut n = 0;
        for hat in names.iter().filter_map(|s| Hat::from_name(s)) {
            if n < MAX_HATS && !out.contains(&Some(hat)) {
                out[n] = Some(hat);
                n += 1;
            }
        }
        out
    }
}

/// How the camera follows a player. Games choose; `Default` lets the
/// player scroll between third and first person.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum CameraMode {
    #[default]
    Default,
    FirstPerson,
    ThirdPerson,
}

impl CameraMode {
    pub const ALL: [CameraMode; 3] = [CameraMode::Default, CameraMode::FirstPerson, CameraMode::ThirdPerson];

    pub fn name(self) -> &'static str {
        match self {
            CameraMode::Default => "default",
            CameraMode::FirstPerson => "first_person",
            CameraMode::ThirdPerson => "third_person",
        }
    }

    pub fn from_name(name: &str) -> Option<CameraMode> {
        CameraMode::ALL.into_iter().find(|m| m.name() == name)
    }
}

/// A player's character during play. Players aren't saved with the scene;
/// the game creates one at a SpawnLocation when Play starts.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PlayerProps {
    /// Where the character is (its centre) and which way it faces
    /// (rotation.y). Size is its bounding box.
    pub body: PartProps,
    pub health: f32,
    pub max_health: f32,
    /// Studs per second.
    pub walk_speed: f32,
    /// Upward speed when jumping, in studs per second.
    pub jump_power: f32,
    pub face: Face,
    pub skin_color: Color,
    pub shirt_color: Color,
    pub pants_color: Color,
    pub shoes_color: Color,
    /// The hats they're wearing (up to MAX_HATS).
    #[serde(default)]
    pub hats: [Option<Hat>; MAX_HATS],
    /// Where in the world their mouse pointed the last time they clicked
    /// with a tool (scripts read it as `player.mouse`): what a gear aims at.
    #[serde(default)]
    pub mouse: Vec3,
    pub camera_mode: CameraMode,
    /// The Tool in the player's hand, if any (one of the Tools inside the
    /// player, which make up their backpack).
    #[serde(default)]
    pub equipped: Option<InstanceId>,
    /// How fast the character is walking (studs/s), for the walk animation.
    #[serde(default)]
    pub speed: f32,
    /// In the air (jumping or falling), for the jump pose.
    #[serde(default)]
    pub airborne: bool,
    /// Seconds left in a tool swing, for the swing animation.
    #[serde(default)]
    pub swing: f32,
    /// Seconds since this player died (0 while alive). Drives the
    /// falling-apart animation; they respawn a few seconds in.
    #[serde(default)]
    pub dead: f32,
    /// The kart they're sitting in and driving (a Model with `kart = true`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kart: Option<InstanceId>,
}

impl Default for PlayerProps {
    fn default() -> Self {
        Self {
            body: PartProps {
                size: Vec3::new(2.0, 5.0, 1.0),
                color: Color::new(40, 110, 200),
                anchored: true,
                ..PartProps::default()
            },
            health: 100.0,
            max_health: 100.0,
            walk_speed: 16.0,
            jump_power: 39.0,
            face: Face::Smile,
            skin_color: Color::new(242, 194, 123),
            shirt_color: Color::new(47, 158, 143),
            pants_color: Color::new(74, 85, 120),
            shoes_color: Color::new(43, 43, 51),
            hats: [None; MAX_HATS],
            mouse: Vec3::new(0.0, 0.0, 0.0),
            camera_mode: CameraMode::Default,
            equipped: None,
            speed: 0.0,
            airborne: false,
            swing: 0.0,
            dead: 0.0,
            kart: None,
        }
    }
}

/// What kind of thing an instance is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Class {
    Workspace,
    Folder,
    Part,
    Script,
    /// A pad players appear on when Play starts. It's a normal part too.
    SpawnLocation,
    /// A group of parts that act as one object: they move together in the
    /// studio, and during play the parts inside are welded together.
    Model,
    /// On-screen text.
    TextLabel,
    /// On-screen text you can click: scripts inside hear `on clicked(player)`.
    TextButton,
    /// An on-screen box, for panels and backgrounds.
    Frame,
    /// Something a player holds: put it inside a player to give it to them.
    /// Its parts are what's held; scripts inside hear `on activated(player)`.
    Tool,
    /// An audio file the game can play (see SoundProps).
    Sound,
    Player,
}

impl Class {
    /// Whether this kind of instance may have children.
    pub fn can_hold_children(self) -> bool {
        !matches!(self, Class::Script)
    }
}

/// Per-class data. Kept in sync with `Instance::class` by construction.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Props {
    Workspace,
    Folder,
    Model,
    Gui(GuiProps),
    Tool,
    Sound(SoundProps),
    /// Parts and SpawnLocations.
    Part(PartProps),
    Script(ScriptProps),
    Player(PlayerProps),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Instance {
    pub id: InstanceId,
    pub class: Class,
    pub name: String,
    pub parent: Option<InstanceId>,
    pub children: Vec<InstanceId>,
    pub props: Props,
    /// Custom fields scripts set: `player.cash = 100`.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub attributes: std::collections::BTreeMap<String, Attribute>,
}

/// Owns every instance in the tree. The Workspace is the root.
#[derive(Debug, Clone)]
pub struct DataModel {
    instances: HashMap<InstanceId, Instance>,
    next_id: u64,
    root: InstanceId,
}

/// The clipboard format (see DataModel::to_clipboard).
#[derive(Serialize, Deserialize)]
struct Clipboard {
    brixo_clipboard: u32,
    items: Vec<ClipItem>,
}

#[derive(Serialize, Deserialize)]
struct ClipItem {
    class: Class,
    name: String,
    attributes: std::collections::BTreeMap<String, Attribute>,
    props: Props,
    children: Vec<ClipItem>,
}

/// Flat, serialisable form of the tree. Maps with non-string keys don't
/// round-trip through JSON, so instances are stored as a list.
#[derive(Serialize, Deserialize)]
struct SavedModel {
    instances: Vec<Instance>,
    next_id: u64,
    root: InstanceId,
}

impl DataModel {
    pub fn new() -> Self {
        let root = InstanceId(0);
        let mut instances = HashMap::new();
        instances.insert(
            root,
            Instance {
                id: root,
                class: Class::Workspace,
                name: "Workspace".to_string(),
                parent: None,
                children: Vec::new(),
                attributes: Default::default(),
                props: Props::Workspace,
            },
        );
        Self {
            instances,
            next_id: 1,
            root,
        }
    }

    pub fn root(&self) -> InstanceId {
        self.root
    }

    pub fn get(&self, id: InstanceId) -> Option<&Instance> {
        self.instances.get(&id)
    }

    pub fn get_mut(&mut self, id: InstanceId) -> Option<&mut Instance> {
        self.instances.get_mut(&id)
    }

    pub fn len(&self) -> usize {
        self.instances.len()
    }

    pub fn is_empty(&self) -> bool {
        self.instances.is_empty()
    }

    /// Every instance, in no particular order.
    pub fn iter(&self) -> impl Iterator<Item = &Instance> {
        self.instances.values()
    }

    /// Every instance in tree order, starting at the root.
    pub fn walk(&self) -> Vec<InstanceId> {
        let mut out = Vec::new();
        let mut stack = vec![self.root];
        while let Some(id) = stack.pop() {
            if let Some(inst) = self.instances.get(&id) {
                out.push(id);
                stack.extend(inst.children.iter().rev().copied());
            }
        }
        out
    }

    /// The first instance with this name, searching in tree order.
    pub fn find_first(&self, name: &str) -> Option<InstanceId> {
        self.walk()
            .into_iter()
            .find(|id| self.instances[id].name == name)
    }

    /// Read a Part's properties. None if the id is missing or isn't a Part.
    pub fn part(&self, id: InstanceId) -> Option<&PartProps> {
        match self.instances.get(&id).map(|i| &i.props) {
            Some(Props::Part(p)) => Some(p),
            _ => None,
        }
    }

    /// Mutate a Part's properties. None if the id is missing or isn't a Part.
    pub fn part_mut(&mut self, id: InstanceId) -> Option<&mut PartProps> {
        match self.instances.get_mut(&id).map(|i| &mut i.props) {
            Some(Props::Part(p)) => Some(p),
            _ => None,
        }
    }

    /// Read a Player's properties.
    pub fn player(&self, id: InstanceId) -> Option<&PlayerProps> {
        match self.instances.get(&id).map(|i| &i.props) {
            Some(Props::Player(p)) => Some(p),
            _ => None,
        }
    }

    pub fn player_mut(&mut self, id: InstanceId) -> Option<&mut PlayerProps> {
        match self.instances.get_mut(&id).map(|i| &mut i.props) {
            Some(Props::Player(p)) => Some(p),
            _ => None,
        }
    }

    /// Position, size, rotation and color of anything with a body in the
    /// world: parts, spawn locations and players.
    pub fn body(&self, id: InstanceId) -> Option<&PartProps> {
        match self.instances.get(&id).map(|i| &i.props) {
            Some(Props::Part(p)) => Some(p),
            Some(Props::Player(p)) => Some(&p.body),
            _ => None,
        }
    }

    pub fn body_mut(&mut self, id: InstanceId) -> Option<&mut PartProps> {
        match self.instances.get_mut(&id).map(|i| &mut i.props) {
            Some(Props::Part(p)) => Some(p),
            Some(Props::Player(p)) => Some(&mut p.body),
            _ => None,
        }
    }

    pub fn gui(&self, id: InstanceId) -> Option<&GuiProps> {
        match self.instances.get(&id).map(|i| &i.props) {
            Some(Props::Gui(g)) => Some(g),
            _ => None,
        }
    }

    pub fn gui_mut(&mut self, id: InstanceId) -> Option<&mut GuiProps> {
        match self.instances.get_mut(&id).map(|i| &mut i.props) {
            Some(Props::Gui(g)) => Some(g),
            _ => None,
        }
    }

    /// Adds an instance exactly as given (its own id), under its parent.
    /// For copies of a world kept in step with another (a game client).
    /// Returns false if it's already here or its parent isn't.
    pub fn insert_instance(&mut self, mut inst: Instance) -> bool {
        if self.instances.contains_key(&inst.id) || inst.parent.is_some_and(|p| !self.instances.contains_key(&p)) {
            return false;
        }
        inst.children.clear(); // they arrive after it, and attach themselves
        if let Some(parent) = inst.parent {
            self.instances.get_mut(&parent).unwrap().children.push(inst.id);
        }
        self.next_id = self.next_id.max(inst.id.0 + 1);
        self.instances.insert(inst.id, inst);
        true
    }

    pub fn sound(&self, id: InstanceId) -> Option<&SoundProps> {
        match self.instances.get(&id).map(|i| &i.props) {
            Some(Props::Sound(s)) => Some(s),
            _ => None,
        }
    }

    pub fn sound_mut(&mut self, id: InstanceId) -> Option<&mut SoundProps> {
        match self.instances.get_mut(&id).map(|i| &mut i.props) {
            Some(Props::Sound(s)) => Some(s),
            _ => None,
        }
    }

    /// The Tool at or around `id`, if any.
    pub fn tool_of(&self, id: InstanceId) -> Option<InstanceId> {
        self.ancestor_of_class(id, Class::Tool)
    }

    /// The Player at or around `id`, if any.
    pub fn player_of(&self, id: InstanceId) -> Option<InstanceId> {
        self.ancestor_of_class(id, Class::Player)
    }

    fn ancestor_of_class(&self, id: InstanceId, class: Class) -> Option<InstanceId> {
        let mut at = Some(id);
        while let Some(n) = at {
            let inst = self.instances.get(&n)?;
            if inst.class == class {
                return Some(n);
            }
            at = inst.parent;
        }
        None
    }

    /// The closest Model around `id` (not counting `id` itself). Parts
    /// under the same Model are welded together during play.
    pub fn weld_group(&self, id: InstanceId) -> Option<InstanceId> {
        let mut at = self.instances.get(&id)?.parent;
        while let Some(p) = at {
            let inst = self.instances.get(&p)?;
            if inst.class == Class::Model {
                return Some(p);
            }
            at = inst.parent;
        }
        None
    }

    /// The outermost Model around `id`, or `id` itself if it isn't in one.
    /// Clicking a part in the studio selects this, like in Roblox.
    pub fn top_model(&self, id: InstanceId) -> InstanceId {
        let mut top = id;
        let mut at = self.instances.get(&id).and_then(|i| i.parent);
        while let Some(p) = at {
            let Some(inst) = self.instances.get(&p) else { break };
            if inst.class == Class::Model {
                top = p;
            }
            at = inst.parent;
        }
        top
    }

    /// Every part (including spawn locations) at or under `id`.
    pub fn parts_under(&self, id: InstanceId) -> Vec<InstanceId> {
        let mut out = Vec::new();
        let mut stack = vec![id];
        while let Some(n) = stack.pop() {
            let Some(inst) = self.instances.get(&n) else { continue };
            if self.part(n).is_some() {
                out.push(n);
            }
            stack.extend(inst.children.iter().rev().copied());
        }
        out
    }

    /// Read a Script's properties. None if the id is missing or isn't a Script.
    pub fn script(&self, id: InstanceId) -> Option<&ScriptProps> {
        match self.instances.get(&id).map(|i| &i.props) {
            Some(Props::Script(s)) => Some(s),
            _ => None,
        }
    }

    pub fn script_mut(&mut self, id: InstanceId) -> Option<&mut ScriptProps> {
        match self.instances.get_mut(&id).map(|i| &mut i.props) {
            Some(Props::Script(s)) => Some(s),
            _ => None,
        }
    }

    /// Creates an instance under `parent`. Returns None if the parent doesn't
    /// exist or can't hold children.
    pub fn create(&mut self, class: Class, name: &str, parent: InstanceId) -> Option<InstanceId> {
        let parent_class = self.instances.get(&parent)?.class;
        if !parent_class.can_hold_children() {
            return None;
        }
        let id = InstanceId(self.next_id);
        self.next_id += 1;
        let props = match class {
            Class::Workspace => Props::Workspace,
            Class::Folder => Props::Folder,
            Class::Model => Props::Model,
            Class::TextLabel | Class::TextButton | Class::Frame => Props::Gui(GuiProps::for_class(class)),
            Class::Tool => Props::Tool,
            Class::Sound => Props::Sound(SoundProps { format: "wav".into(), data: "".into(), volume: 0.8 }),
            Class::Part => Props::Part(PartProps::default()),
            Class::Script => Props::Script(ScriptProps::default()),
            Class::SpawnLocation => Props::Part(PartProps {
                size: Vec3::new(6.0, 1.0, 6.0),
                color: Color::new(90, 90, 100),
                ..PartProps::default()
            }),
            Class::Player => Props::Player(PlayerProps::default()),
        };
        self.instances.insert(
            id,
            Instance {
                id,
                class,
                name: name.to_string(),
                parent: Some(parent),
                children: Vec::new(),
                attributes: Default::default(),
                props,
            },
        );
        if let Some(p) = self.instances.get_mut(&parent) {
            p.children.push(id);
        }
        Some(id)
    }

    /// Copies an instance and everything inside it, placing the copy next to
    /// the original. Returns the copy's id.
    pub fn clone_subtree(&mut self, id: InstanceId) -> Option<InstanceId> {
        if id == self.root {
            return None;
        }
        let parent = self.instances.get(&id)?.parent?;
        let copy = self.copy_into(id, parent)?;
        Some(copy)
    }

    fn copy_into(&mut self, source: InstanceId, parent: InstanceId) -> Option<InstanceId> {
        let original = self.instances.get(&source)?.clone();
        let new_id = InstanceId(self.next_id);
        self.next_id += 1;
        self.instances.insert(
            new_id,
            Instance {
                id: new_id,
                class: original.class,
                name: original.name.clone(),
                parent: Some(parent),
                children: Vec::new(),
                attributes: original.attributes.clone(),
                props: original.props.clone(),
            },
        );
        if let Some(p) = self.instances.get_mut(&parent) {
            p.children.push(new_id);
        }
        for child in original.children {
            self.copy_into(child, new_id);
        }
        Some(new_id)
    }

    /// Things copied to the clipboard, as text another game can paste:
    /// each item with its properties, fields and everything inside it, but
    /// no ids (those belong to this game). Items inside other copied items
    /// are skipped (they come along anyway). The root can't be copied.
    pub fn to_clipboard(&self, ids: &[InstanceId]) -> String {
        let items: Vec<ClipItem> = ids
            .iter()
            .filter(|id| **id != self.root && self.instances.contains_key(id))
            .filter(|id| !ids.iter().any(|other| other != *id && self.is_descendant_of(**id, *other)))
            .filter_map(|id| self.clip_item(*id))
            .collect();
        serde_json::to_string(&Clipboard { brixo_clipboard: 1, items }).unwrap_or_default()
    }

    fn clip_item(&self, id: InstanceId) -> Option<ClipItem> {
        let inst = self.instances.get(&id)?;
        Some(ClipItem {
            class: inst.class,
            name: inst.name.clone(),
            attributes: inst.attributes.clone(),
            props: inst.props.clone(),
            children: inst.children.iter().filter_map(|c| self.clip_item(*c)).collect(),
        })
    }

    /// Pastes clipboard text (from to_clipboard, in any game) under
    /// `parent`, with fresh ids. Returns the new top-level things, or None if
    /// the text isn't Brixo's or `parent` can't hold anything.
    pub fn paste_clipboard(&mut self, text: &str, parent: InstanceId) -> Option<Vec<InstanceId>> {
        let clip: Clipboard = serde_json::from_str(text.trim()).ok()?;
        if clip.brixo_clipboard != 1 || !self.instances.get(&parent)?.class.can_hold_children() {
            return None;
        }
        Some(clip.items.into_iter().filter_map(|item| self.paste_item(item, parent)).collect())
    }

    fn paste_item(&mut self, item: ClipItem, parent: InstanceId) -> Option<InstanceId> {
        // (The Workspace is unique: one can't be pasted in.)
        if item.class == Class::Workspace {
            return None;
        }
        let id = InstanceId(self.next_id);
        self.next_id += 1;
        self.instances.insert(
            id,
            Instance {
                id,
                class: item.class,
                name: item.name,
                parent: Some(parent),
                children: Vec::new(),
                attributes: item.attributes,
                props: item.props,
            },
        );
        self.instances.get_mut(&parent)?.children.push(id);
        for child in item.children {
            self.paste_item(child, id);
        }
        Some(id)
    }

    /// Removes an instance and all its descendants.
    /// Returns false if it doesn't exist or is the root.
    pub fn remove(&mut self, id: InstanceId) -> bool {
        if id == self.root || !self.instances.contains_key(&id) {
            return false;
        }

        // Detach from the parent's child list.
        let parent_id = self.instances[&id].parent;
        if let Some(pid) = parent_id {
            if let Some(p) = self.instances.get_mut(&pid) {
                p.children.retain(|c| *c != id);
            }
        }

        // Delete the whole subtree.
        let mut stack = vec![id];
        while let Some(current) = stack.pop() {
            if let Some(inst) = self.instances.remove(&current) {
                stack.extend(inst.children);
            }
        }
        true
    }

    /// True if `node` sits somewhere below `ancestor` in the tree.
    fn is_descendant_of(&self, node: InstanceId, ancestor: InstanceId) -> bool {
        let mut current = self.instances.get(&node).and_then(|i| i.parent);
        while let Some(id) = current {
            if id == ancestor {
                return true;
            }
            current = self.instances.get(&id).and_then(|i| i.parent);
        }
        false
    }

    /// Moves `id` under `new_parent`. Returns false if either doesn't exist,
    /// `id` is the root, the new parent can't hold children, or the move
    /// would create a cycle.
    pub fn reparent(&mut self, id: InstanceId, new_parent: InstanceId) -> bool {
        if id == self.root || id == new_parent {
            return false;
        }
        if !self.instances.contains_key(&id) {
            return false;
        }
        match self.instances.get(&new_parent) {
            Some(p) if p.class.can_hold_children() => {}
            _ => return false,
        }
        if self.is_descendant_of(new_parent, id) {
            return false;
        }

        let old_parent = self.instances[&id].parent;
        if let Some(op) = old_parent {
            if let Some(p) = self.instances.get_mut(&op) {
                p.children.retain(|c| *c != id);
            }
        }
        self.instances
            .get_mut(&new_parent)
            .unwrap()
            .children
            .push(id);
        self.instances.get_mut(&id).unwrap().parent = Some(new_parent);
        true
    }

    // --- saving and loading ---

    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        let mut instances: Vec<Instance> = self.instances.values().cloned().collect();
        // Sort so saved files are stable rather than in random map order.
        instances.sort_by_key(|i| i.id.0);
        let saved = SavedModel {
            instances,
            next_id: self.next_id,
            root: self.root,
        };
        serde_json::to_string_pretty(&saved)
    }

    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        let saved: SavedModel = serde_json::from_str(json)?;
        let mut instances = HashMap::new();
        for inst in saved.instances {
            instances.insert(inst.id, inst);
        }
        Ok(Self {
            instances,
            next_id: saved.next_id,
            root: saved.root,
        })
    }

    pub fn save_file(&self, path: &str) -> std::io::Result<()> {
        let json = self
            .to_json()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        std::fs::write(path, json)
    }

    pub fn load_file(path: &str) -> std::io::Result<Self> {
        let json = std::fs::read_to_string(path)?;
        Self::from_json(&json).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }
}

impl Default for DataModel {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_a_tree_with_correct_links() {
        let mut dm = DataModel::new();
        let workspace = dm.root();
        let folder = dm.create(Class::Folder, "Enemies", workspace).unwrap();
        let part = dm.create(Class::Part, "Goblin", folder).unwrap();

        assert_eq!(dm.get(folder).unwrap().parent, Some(workspace));
        assert_eq!(dm.get(part).unwrap().parent, Some(folder));
        assert_eq!(dm.get(workspace).unwrap().children, vec![folder]);
        assert_eq!(dm.get(folder).unwrap().children, vec![part]);
    }

    #[test]
    fn create_under_missing_parent_returns_none() {
        let mut dm = DataModel::new();
        let bogus = InstanceId(999);
        assert!(dm.create(Class::Part, "Lost", bogus).is_none());
    }

    #[test]
    fn remove_deletes_the_whole_subtree() {
        let mut dm = DataModel::new();
        let workspace = dm.root();
        let folder = dm.create(Class::Folder, "Enemies", workspace).unwrap();
        let part = dm.create(Class::Part, "Goblin", folder).unwrap();

        assert!(dm.remove(folder));
        assert!(dm.get(folder).is_none());
        assert!(dm.get(part).is_none());
        assert!(dm.get(workspace).unwrap().children.is_empty());
    }

    #[test]
    fn cannot_remove_the_root_or_a_missing_instance() {
        let mut dm = DataModel::new();
        let root = dm.root();
        assert!(!dm.remove(root));
        assert!(!dm.remove(InstanceId(999)));
    }

    #[test]
    fn reparent_moves_an_instance() {
        let mut dm = DataModel::new();
        let workspace = dm.root();
        let a = dm.create(Class::Folder, "A", workspace).unwrap();
        let b = dm.create(Class::Folder, "B", workspace).unwrap();
        let part = dm.create(Class::Part, "Box", a).unwrap();

        assert!(dm.reparent(part, b));
        assert_eq!(dm.get(part).unwrap().parent, Some(b));
        assert!(dm.get(a).unwrap().children.is_empty());
        assert_eq!(dm.get(b).unwrap().children, vec![part]);
    }

    #[test]
    fn reparent_rejects_cycles() {
        let mut dm = DataModel::new();
        let workspace = dm.root();
        let a = dm.create(Class::Folder, "A", workspace).unwrap();
        let b = dm.create(Class::Folder, "B", a).unwrap();

        assert!(!dm.reparent(a, b)); // can't move a folder into its own child
        assert!(!dm.reparent(a, a)); // can't move something into itself
    }

    #[test]
    fn parts_get_default_props_and_can_be_edited() {
        let mut dm = DataModel::new();
        let workspace = dm.root();
        let part = dm.create(Class::Part, "Box", workspace).unwrap();

        assert_eq!(dm.part(part).unwrap().size, Vec3::ONE);

        let p = dm.part_mut(part).unwrap();
        p.position = Vec3::new(1.0, 5.0, -3.0);
        p.color = Color::new(255, 0, 0);

        assert_eq!(dm.part(part).unwrap().position, Vec3::new(1.0, 5.0, -3.0));
        assert_eq!(dm.part(part).unwrap().color, Color::new(255, 0, 0));
    }

    #[test]
    fn non_parts_have_no_part_props() {
        let mut dm = DataModel::new();
        let workspace = dm.root();
        let folder = dm.create(Class::Folder, "Stuff", workspace).unwrap();

        assert!(dm.part(folder).is_none());
        assert!(dm.part(workspace).is_none());
    }

    #[test]
    fn json_round_trip_preserves_the_tree() {
        let mut dm = DataModel::new();
        let workspace = dm.root();
        let folder = dm.create(Class::Folder, "Enemies", workspace).unwrap();
        let part = dm.create(Class::Part, "Goblin", folder).unwrap();
        dm.part_mut(part).unwrap().position = Vec3::new(2.0, 0.5, 7.0);

        let json = dm.to_json().unwrap();
        let loaded = DataModel::from_json(&json).unwrap();

        assert_eq!(loaded.len(), dm.len());
        assert_eq!(loaded.root(), workspace);
        assert_eq!(loaded.get(part).unwrap().name, "Goblin");
        assert_eq!(loaded.get(part).unwrap().parent, Some(folder));
        assert_eq!(loaded.get(folder).unwrap().children, vec![part]);
        assert_eq!(loaded.part(part).unwrap().position, Vec3::new(2.0, 0.5, 7.0));
    }

    #[test]
    fn loaded_model_keeps_handing_out_fresh_ids() {
        let mut dm = DataModel::new();
        let workspace = dm.root();
        let a = dm.create(Class::Part, "A", workspace).unwrap();

        let mut loaded = DataModel::from_json(&dm.to_json().unwrap()).unwrap();
        let b = loaded.create(Class::Part, "B", workspace).unwrap();

        assert_ne!(a, b);
        assert!(loaded.get(a).is_some());
        assert!(loaded.get(b).is_some());
    }

    #[test]
    fn scripts_hold_source_and_round_trip() {
        let mut dm = DataModel::new();
        let root = dm.root();
        let part = dm.create(Class::Part, "Coin", root).unwrap();
        let script = dm.create(Class::Script, "Spin", part).unwrap();
        dm.script_mut(script).unwrap().source = "print(1)".to_string();

        let loaded = DataModel::from_json(&dm.to_json().unwrap()).unwrap();
        assert_eq!(loaded.script(script).unwrap().source, "print(1)");
        assert!(loaded.script(script).unwrap().enabled);
        assert!(loaded.part(script).is_none());
    }

    #[test]
    fn scripts_cannot_hold_children() {
        let mut dm = DataModel::new();
        let root = dm.root();
        let script = dm.create(Class::Script, "S", root).unwrap();
        let folder = dm.create(Class::Folder, "F", root).unwrap();
        assert!(dm.create(Class::Part, "Inside", script).is_none());
        assert!(!dm.reparent(folder, script));
    }

    #[test]
    fn old_saves_without_enabled_still_load() {
        let json = r#"{"instances":[
            {"id":0,"class":"Workspace","name":"Workspace","parent":null,"children":[1],"props":"Workspace"},
            {"id":1,"class":"Script","name":"S","parent":0,"children":[],"props":{"Script":{"source":"x = 1"}}}
        ],"next_id":2,"root":0}"#;
        let dm = DataModel::from_json(json).unwrap();
        assert!(dm.script(InstanceId(1)).unwrap().enabled);
    }

    #[test]
    fn clone_subtree_copies_children_with_new_ids() {
        let mut dm = DataModel::new();
        let root = dm.root();
        let part = dm.create(Class::Part, "Coin", root).unwrap();
        let script = dm.create(Class::Script, "Spin", part).unwrap();
        dm.part_mut(part).unwrap().position = Vec3::new(1.0, 2.0, 3.0);

        let copy = dm.clone_subtree(part).unwrap();
        assert_ne!(copy, part);
        assert_eq!(dm.get(copy).unwrap().parent, Some(root));
        assert_eq!(dm.part(copy).unwrap().position, Vec3::new(1.0, 2.0, 3.0));
        let copied_children = &dm.get(copy).unwrap().children;
        assert_eq!(copied_children.len(), 1);
        assert_ne!(copied_children[0], script);
        assert!(dm.script(copied_children[0]).is_some());
    }

    #[test]
    fn saves_from_before_physics_load_as_anchored() {
        let json = r#"{"instances":[
            {"id":0,"class":"Workspace","name":"Workspace","parent":null,"children":[1],"props":"Workspace"},
            {"id":1,"class":"Part","name":"P","parent":0,"children":[],"props":{"Part":{
                "position":{"x":0,"y":0,"z":0},"size":{"x":1,"y":1,"z":1},
                "rotation":{"x":0,"y":0,"z":0},"color":{"r":1,"g":2,"b":3}}}}
        ],"next_id":2,"root":0}"#;
        let dm = DataModel::from_json(json).unwrap();
        let p = dm.part(InstanceId(1)).unwrap();
        assert!(p.anchored && p.can_collide);
    }

    #[test]
    fn spawn_locations_are_parts_and_players_have_bodies() {
        let mut dm = DataModel::new();
        let root = dm.root();
        let spawn = dm.create(Class::SpawnLocation, "Spawn", root).unwrap();
        let player = dm.create(Class::Player, "Player", root).unwrap();
        assert_eq!(dm.part(spawn).unwrap().size, Vec3::new(6.0, 1.0, 6.0));
        assert!(dm.part(player).is_none(), "players aren't simulated as parts");
        assert_eq!(dm.player(player).unwrap().health, 100.0);
        dm.body_mut(player).unwrap().position.y = 7.0;
        assert_eq!(dm.player(player).unwrap().body.position.y, 7.0);
        assert!(dm.body(spawn).is_some());
    }

    #[test]
    fn faces_and_camera_modes_have_script_names() {
        for f in Face::ALL {
            assert_eq!(Face::from_name(f.name()), Some(f));
        }
        for m in CameraMode::ALL {
            assert_eq!(CameraMode::from_name(m.name()), Some(m));
        }
        assert_eq!(Face::from_name("grumpy"), None);
    }

    #[test]
    fn models_group_parts_and_find_their_weld_group() {
        let mut dm = DataModel::new();
        let root = dm.root();
        let car = dm.create(Class::Model, "Car", root).unwrap();
        let body = dm.create(Class::Part, "Body", car).unwrap();
        let wheels = dm.create(Class::Model, "Wheels", car).unwrap();
        let wheel = dm.create(Class::Part, "Wheel", wheels).unwrap();
        let loose = dm.create(Class::Part, "Loose", root).unwrap();
        assert_eq!(dm.weld_group(body), Some(car));
        assert_eq!(dm.weld_group(wheel), Some(wheels), "closest model welds");
        assert_eq!(dm.weld_group(loose), None);
        assert_eq!(dm.top_model(wheel), car, "clicking a wheel selects the car");
        assert_eq!(dm.top_model(loose), loose);
        let mut parts = dm.parts_under(car);
        parts.sort_by_key(|id| id.raw());
        assert_eq!(parts, vec![body, wheel]);
    }

    #[test]
    fn old_saves_load_as_blocks() {
        let json = r#"{"position":{"x":0,"y":0,"z":0},"size":{"x":1,"y":1,"z":1},"rotation":{"x":0,"y":0,"z":0},"color":{"r":1,"g":2,"b":3}}"#;
        let p: PartProps = serde_json::from_str(json).unwrap();
        assert_eq!(p.shape, Shape::Block);
        for s in Shape::ALL {
            assert_eq!(Shape::from_name(s.name()), Some(s));
        }
    }

    #[test]
    fn find_first_searches_in_tree_order() {
        let mut dm = DataModel::new();
        let root = dm.root();
        let folder = dm.create(Class::Folder, "F", root).unwrap();
        let deep = dm.create(Class::Part, "Target", folder).unwrap();
        let _later = dm.create(Class::Part, "Target", root).unwrap();
        assert_eq!(dm.find_first("Target"), Some(deep));
        assert_eq!(dm.find_first("Nope"), None);
    }
}
