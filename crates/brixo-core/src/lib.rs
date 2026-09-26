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
    /// The file itself, base64-encoded, so games stay plain JSON.
    pub data: String,
    /// 0 to 1.
    pub volume: f32,
}

impl SoundProps {
    pub fn from_bytes(format: &str, bytes: &[u8]) -> SoundProps {
        use base64::Engine;
        SoundProps { format: format.to_string(), data: base64::engine::general_purpose::STANDARD.encode(bytes), volume: 0.8 }
    }

    /// The file's bytes (None if the data is missing or damaged).
    pub fn bytes(&self) -> Option<Vec<u8>> {
        use base64::Engine;
        base64::engine::general_purpose::STANDARD.decode(&self.data).ok().filter(|b| !b.is_empty())
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
            camera_mode: CameraMode::Default,
            equipped: None,
            speed: 0.0,
            airborne: false,
            swing: 0.0,
            dead: 0.0,
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
            Class::Sound => Props::Sound(SoundProps { format: "wav".into(), data: String::new(), volume: 0.8 }),
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
