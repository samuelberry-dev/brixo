//! What players wear: faces, clothes, t-shirt pictures and accessories,
//! and the body colours under them. Each has a name scripts and the website
//! use ("hoodie") and a title people see ("Hoodie").
//!
//! The character is made of six body parts, each its own colour (like old
//! Roblox's Body Colors). A shirt covers the torso (and the arms, as far as
//! its sleeves go), pants cover the legs (as far as they go), and a t-shirt
//! picture is printed on the front of the torso. Accessories go in four
//! slots: one on the head, one on the face, one at the neck, one on the back.

use serde::{Deserialize, Serialize};

use crate::Color;

/// Defines a list of named things: `ALL`, `name()`, `title()` and
/// `from_name()`.
macro_rules! named {
    ($(#[$meta:meta])* $ty:ident { $($(#[$vmeta:meta])* $v:ident = $name:literal, $title:literal;)* }) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
        pub enum $ty {
            $($(#[$vmeta])* $v,)*
        }

        impl $ty {
            pub const ALL: &'static [$ty] = &[$($ty::$v),*];

            /// The name scripts and the website use.
            pub fn name(self) -> &'static str {
                match self {
                    $($ty::$v => $name,)*
                }
            }

            /// What people see.
            pub fn title(self) -> &'static str {
                match self {
                    $($ty::$v => $title,)*
                }
            }

            pub fn from_name(name: &str) -> Option<$ty> {
                Self::ALL.iter().copied().find(|x| x.name() == name)
            }
        }
    };
}

named! {
    /// The expression drawn on a player's face.
    Face {
        Smile = "smile", "Smile";
        Happy = "happy", "Happy";
        Surprised = "surprised", "Surprised";
        Determined = "determined", "Determined";
        Wink = "wink", "Wink";
        Grin = "grin", "Big Grin";
        Silly = "silly", "Silly";
        Sleepy = "sleepy", "Sleepy";
        Angry = "angry", "Angry";
        Smirk = "smirk", "Smirk";
        Cat = "cat", "Kitty";
        HeartEyes = "heart_eyes", "Heart Eyes";
        Worried = "worried", "Worried";
        Laugh = "laugh", "Laughing";
    }
}

impl Default for Face {
    fn default() -> Self {
        Face::Smile
    }
}

named! {
    /// A shirt's cut and print. It's worn in the player's shirt colour; the
    /// print is shading on top (stripes, pockets, a collar).
    Shirt {
        /// No shirt: the torso and arms show their body colours.
        None = "none", "No Shirt";
        Tee = "tee", "T-Shirt";
        Tank = "tank", "Tank Top";
        LongSleeve = "long_sleeve", "Long Sleeve";
        Striped = "striped", "Striped Tee";
        Polo = "polo", "Polo";
        Hoodie = "hoodie", "Hoodie";
        Flannel = "flannel", "Flannel";
        Jacket = "jacket", "Jacket";
        Sweater = "sweater", "Sweater";
        Jersey = "jersey", "Sports Jersey";
        Camo = "camo", "Camo Tee";
    }
}

/// How far a shirt's sleeves come down the arm.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sleeves {
    None,
    Short,
    Long,
}

impl Shirt {
    pub fn sleeves(self) -> Sleeves {
        match self {
            Shirt::None | Shirt::Tank => Sleeves::None,
            Shirt::Tee | Shirt::Striped | Shirt::Polo | Shirt::Jersey | Shirt::Camo => Sleeves::Short,
            Shirt::LongSleeve | Shirt::Hoodie | Shirt::Flannel | Shirt::Jacket | Shirt::Sweater => Sleeves::Long,
        }
    }
}

impl Default for Shirt {
    fn default() -> Self {
        Shirt::Tee
    }
}

named! {
    /// Pants: their cut and print, worn in the player's pants colour.
    Pants {
        /// No pants: the legs show their body colours.
        None = "none", "No Pants";
        Plain = "plain", "Plain Pants";
        Jeans = "jeans", "Jeans";
        Shorts = "shorts", "Shorts";
        Cargo = "cargo", "Cargo Pants";
        Track = "track", "Track Pants";
        Plaid = "plaid", "Plaid Shorts";
    }
}

impl Pants {
    /// Shorts stop above the knee.
    pub fn short(self) -> bool {
        matches!(self, Pants::Shorts | Pants::Plaid)
    }
}

impl Default for Pants {
    fn default() -> Self {
        Pants::Plain
    }
}

named! {
    /// A picture printed on the front of the shirt (or the torso).
    TShirt {
        Brick = "brick", "Brixo Brick";
        Smiley = "smiley", "Smiley";
        Heart = "heart", "Heart";
        Star = "star", "Gold Star";
        Flame = "flame", "Flames";
        Lightning = "lightning", "Lightning";
        Rocket = "rocket", "Rocket";
        Pizza = "pizza", "Pizza";
        Rainbow = "rainbow", "Rainbow";
        Ghost = "ghost", "Ghost";
        NumberOne = "number_one", "#1";
    }
}

/// Where on the character an accessory goes. One accessory per slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HatSlot {
    Head,
    Face,
    Neck,
    Back,
}

impl HatSlot {
    pub const ALL: [HatSlot; 4] = [HatSlot::Head, HatSlot::Face, HatSlot::Neck, HatSlot::Back];

    pub fn name(self) -> &'static str {
        match self {
            HatSlot::Head => "head",
            HatSlot::Face => "face",
            HatSlot::Neck => "neck",
            HatSlot::Back => "back",
        }
    }

    /// Worn on the head (turns and flies off with it), or on the body.
    pub fn on_head(self) -> bool {
        matches!(self, HatSlot::Head | HatSlot::Face)
    }
}

named! {
    /// An accessory: hats, glasses, scarves, backpacks. (Called hats in
    /// code and scripts, as they all were once.)
    Hat {
        Cap = "cap", "Baseball Cap";
        Beanie = "beanie", "Beanie";
        TopHat = "top_hat", "Top Hat";
        CowboyHat = "cowboy_hat", "Cowboy Hat";
        Crown = "crown", "Crown";
        Headphones = "headphones", "Headphones";
        PartyHat = "party_hat", "Party Hat";
        ChefHat = "chef_hat", "Chef Hat";
        VikingHelmet = "viking_helmet", "Viking Helmet";
        HardHat = "hard_hat", "Hard Hat";
        PropellerCap = "propeller_cap", "Propeller Cap";
        Halo = "halo", "Halo";
        TrafficCone = "traffic_cone", "Traffic Cone";
        WizardHat = "wizard_hat", "Wizard Hat";
        PirateHat = "pirate_hat", "Pirate Hat";
        BunnyEars = "bunny_ears", "Bunny Ears";
        Fedora = "fedora", "Fedora";
        Sunglasses = "sunglasses", "Sunglasses";
        NerdGlasses = "nerd_glasses", "Nerd Glasses";
        EyePatch = "eye_patch", "Eye Patch";
        Mustache = "mustache", "Mustache";
        Scarf = "scarf", "Red Scarf";
        BowTie = "bow_tie", "Bow Tie";
        GoldChain = "gold_chain", "Gold Chain";
        Necktie = "necktie", "Necktie";
        Backpack = "backpack", "Backpack";
        Cape = "cape", "Hero Cape";
        AngelWings = "angel_wings", "Angel Wings";
        Jetpack = "jetpack", "Jetpack";
    }
}

/// How many accessories a player can wear at once: one per slot.
pub const MAX_HATS: usize = 4;

impl Hat {
    pub fn slot(self) -> HatSlot {
        use Hat::*;
        match self {
            Sunglasses | NerdGlasses | EyePatch | Mustache => HatSlot::Face,
            Scarf | BowTie | GoldChain | Necktie => HatSlot::Neck,
            Backpack | Cape | AngelWings | Jetpack => HatSlot::Back,
            _ => HatSlot::Head,
        }
    }

    /// Known accessories from a list of names, at most one per slot (the
    /// first one listed wins), skipping unknown names and repeats.
    pub fn list(names: &[String]) -> [Option<Hat>; MAX_HATS] {
        let mut out = [None; MAX_HATS];
        let mut n = 0;
        for hat in names.iter().filter_map(|s| Hat::from_name(s)) {
            if n < MAX_HATS && !out.iter().flatten().any(|h: &Hat| h.slot() == hat.slot()) {
                out[n] = Some(hat);
                n += 1;
            }
        }
        out
    }
}

/// The six body parts, each with its own colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BodyPart {
    Head,
    Torso,
    LeftArm,
    RightArm,
    LeftLeg,
    RightLeg,
}

impl BodyPart {
    pub const ALL: [BodyPart; 6] = [BodyPart::Head, BodyPart::Torso, BodyPart::LeftArm, BodyPart::RightArm, BodyPart::LeftLeg, BodyPart::RightLeg];

    pub fn name(self) -> &'static str {
        match self {
            BodyPart::Head => "head",
            BodyPart::Torso => "torso",
            BodyPart::LeftArm => "left_arm",
            BodyPart::RightArm => "right_arm",
            BodyPart::LeftLeg => "left_leg",
            BodyPart::RightLeg => "right_leg",
        }
    }

    pub fn index(self) -> usize {
        self as usize
    }
}

/// A colour for each body part, in `BodyPart::ALL` order.
pub type BodyColors = [Color; 6];

/// Every body part in one colour (a skin tone).
pub fn body_all(c: Color) -> BodyColors {
    [c; 6]
}

/// The classic brick colours players pick body colours from.
pub const BODY_PALETTE: [(u8, u8, u8); 32] = [
    // Skin tones.
    (245, 205, 164), (234, 196, 160), (227, 185, 138), (204, 142, 105), (175, 116, 75), (160, 99, 62), (124, 78, 50), (86, 58, 36),
    // Bright.
    (196, 40, 28), (218, 133, 65), (245, 205, 48), (164, 189, 71), (75, 151, 75), (0, 143, 156), (13, 105, 172), (110, 153, 202),
    // Deep.
    (123, 46, 47), (160, 95, 53), (226, 155, 64), (39, 70, 45), (40, 127, 71), (33, 84, 185), (52, 43, 117), (107, 50, 124),
    // Soft and plain.
    (255, 201, 201), (232, 186, 200), (180, 210, 228), (204, 255, 204), (248, 241, 132), (242, 243, 243), (163, 162, 165), (27, 42, 53),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_go_both_ways_and_are_unique() {
        fn check<T: Copy + PartialEq + std::fmt::Debug>(all: &[T], name: fn(T) -> &'static str, from: fn(&str) -> Option<T>) {
            for &x in all {
                assert_eq!(from(name(x)), Some(x));
                assert_eq!(all.iter().filter(|y| name(**y) == name(x)).count(), 1, "{x:?}");
            }
        }
        check(Face::ALL, Face::name, Face::from_name);
        check(Shirt::ALL, Shirt::name, Shirt::from_name);
        check(Pants::ALL, Pants::name, Pants::from_name);
        check(TShirt::ALL, TShirt::name, TShirt::from_name);
        check(Hat::ALL, Hat::name, Hat::from_name);
        assert_eq!(Face::from_name("grumpy"), None);
    }

    #[test]
    fn one_accessory_per_slot() {
        let names: Vec<String> = ["cap", "crown", "sunglasses", "cape", "scarf", "nope", "cap"].iter().map(|s| s.to_string()).collect();
        let worn = Hat::list(&names);
        assert_eq!(worn, [Some(Hat::Cap), Some(Hat::Sunglasses), Some(Hat::Cape), Some(Hat::Scarf)]);
        for slot in HatSlot::ALL {
            assert!(Hat::ALL.iter().any(|h| h.slot() == slot), "nothing for {slot:?}");
        }
    }
}
