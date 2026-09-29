//! The Catalog: everything a character can wear, what it costs in Brix,
//! and who owns what.
//!
//! Brix are earned, never bought: a few for visiting each day, and more
//! for completing challenges in games (the amounts are set by admins, and
//! there's a limit a day). A starter set is free for everyone, admins own
//! everything, and prices can be changed without a release (the prices
//! table overrides the ones here).

use brixo_core::{Face, Hat, Pants, Shirt, TShirt};
use serde::Serialize;

/// Brix for visiting the website, once a day.
pub const DAILY_BONUS: i64 = 10;
/// Brix a new account starts with, to buy something straight away.
pub const WELCOME_BRIX: i64 = 50;
/// The most Brix challenges can pay one player in a day, across all games.
pub const DAILY_CHALLENGE_LIMIT: i64 = 200;
/// The most challenges one game can have (so a script can't make endless ones).
pub const MAX_CHALLENGES: i64 = 40;
/// The most Brix one challenge can pay.
pub const MAX_REWARD: i64 = 500;
/// Saved outfits per account.
pub const MAX_OUTFITS: i64 = 5;

/// Something in the Catalog.
#[derive(Debug, Clone, Serialize)]
pub struct Item {
    /// "hat:crown", "shirt:hoodie", "face:wink", "pants:cargo", "tshirt:star".
    pub id: String,
    /// "hat", "face", "shirt", "pants" or "tshirt".
    pub kind: &'static str,
    /// The name inside: "crown".
    pub name: &'static str,
    pub title: &'static str,
    /// Where an accessory goes: "head", "face", "neck", "back".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slot: Option<&'static str>,
    /// What it costs (0: free for everyone).
    pub price: i64,
}

impl Item {
    pub fn free(&self) -> bool {
        self.price == 0
    }
}

/// What things cost, before any changes an admin made.
fn default_price(kind: &str, name: &str) -> i64 {
    match (kind, name) {
        // The starter set.
        ("face", "smile" | "happy" | "surprised" | "determined") => 0,
        ("shirt", "none" | "tee" | "tank" | "long_sleeve") => 0,
        ("pants", "none" | "plain" | "shorts" | "jeans") => 0,
        ("tshirt", "smiley" | "brick") => 0,
        ("hat", "cap" | "beanie") => 0,
        // Faces.
        ("face", "wink" | "smirk" | "sleepy" | "worried") => 25,
        ("face", "grin" | "angry" | "silly" | "laugh") => 35,
        ("face", _) => 50,
        // Clothes.
        ("shirt", "striped" | "camo") => 30,
        ("shirt", "polo" | "jersey") => 45,
        ("shirt", "hoodie" | "flannel" | "sweater") => 60,
        ("shirt", _) => 80,
        ("pants", "plaid") => 30,
        ("pants", _) => 40,
        ("tshirt", "heart" | "star" | "pizza" | "ghost") => 20,
        ("tshirt", _) => 35,
        // Accessories, cheapest to rarest.
        ("hat", "hard_hat" | "chef_hat" | "party_hat" | "headphones" | "mustache" | "bow_tie" | "necktie") => 40,
        ("hat", "cowboy_hat" | "propeller_cap" | "traffic_cone" | "fedora" | "sunglasses" | "nerd_glasses" | "eye_patch" | "scarf" | "backpack") => 60,
        ("hat", "top_hat" | "viking_helmet" | "pirate_hat" | "bunny_ears" | "gold_chain" | "cape") => 100,
        ("hat", "wizard_hat" | "jetpack") => 150,
        ("hat", "halo" | "angel_wings") => 200,
        ("hat", "crown") => 300,
        _ => 50,
    }
}

/// Everything in the Catalog, with prices as they are now (`price_of` gives
/// an admin's change, if any).
pub fn catalog(price_of: impl Fn(&str) -> Option<i64>) -> Vec<Item> {
    let mut out = Vec::new();
    let mut add = |kind: &'static str, name: &'static str, title: &'static str, slot: Option<&'static str>| {
        let id = format!("{kind}:{name}");
        let price = price_of(&id).unwrap_or_else(|| default_price(kind, name));
        out.push(Item { id, kind, name, title, slot, price });
    };
    for &f in Face::ALL {
        add("face", f.name(), f.title(), None);
    }
    for &s in Shirt::ALL {
        add("shirt", s.name(), s.title(), None);
    }
    for &p in Pants::ALL {
        add("pants", p.name(), p.title(), None);
    }
    for &t in TShirt::ALL {
        add("tshirt", t.name(), t.title(), None);
    }
    for &h in Hat::ALL {
        add("hat", h.name(), h.title(), Some(h.slot().name()));
    }
    out
}

/// Whether an id names something in the Catalog.
pub fn exists(id: &str) -> bool {
    let Some((kind, name)) = id.split_once(':') else { return false };
    match kind {
        "face" => Face::from_name(name).is_some(),
        "shirt" => Shirt::from_name(name).is_some(),
        "pants" => Pants::from_name(name).is_some(),
        "tshirt" => TShirt::from_name(name).is_some(),
        "hat" => Hat::from_name(name).is_some(),
        _ => false,
    }
}

/// The Catalog items a look wears (for checking they're all owned).
pub fn worn(a: &crate::db::Avatar) -> Vec<String> {
    let mut ids = vec![format!("face:{}", a.face), format!("shirt:{}", a.shirt_style), format!("pants:{}", a.pants_style)];
    if !a.tshirt.is_empty() {
        ids.push(format!("tshirt:{}", a.tshirt));
    }
    ids.extend(a.hats.iter().map(|h| format!("hat:{h}")));
    ids
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn there_is_a_free_starter_set_and_everything_else_costs_something() {
        let items = catalog(|_| None);
        for kind in ["face", "shirt", "pants", "tshirt", "hat"] {
            let of_kind: Vec<&Item> = items.iter().filter(|i| i.kind == kind).collect();
            assert!(of_kind.iter().filter(|i| i.free()).count() >= 2, "free {kind}s");
            assert!(of_kind.iter().any(|i| !i.free()), "{kind}s for sale");
        }
        assert!(items.iter().all(|i| exists(&i.id)));
        assert!(!exists("hat:nope") && !exists("nope"));
        let crown = items.iter().find(|i| i.id == "hat:crown").unwrap();
        assert!(crown.price >= 200);
        let changed = catalog(|id| (id == "hat:crown").then_some(5));
        assert_eq!(changed.iter().find(|i| i.id == "hat:crown").unwrap().price, 5);
    }
}
