//! The leaderboard: the classic box in the top-right corner listing
//! everyone in the game with the stats the game picked
//! (`leaderboard("coins", "wins")` in a script shows each player's
//! `p.coins` and `p.wins`). Players on teams are grouped under their team,
//! in its colour, with the team's total. Tab, or the arrow on its title
//! bar, folds it away and back.
//!
//! Brixo Player and Studio's Play both draw it from their copy of the world.

use brixo_core::{leaderboard_columns, leaderboard_title, Attribute, DataModel, InstanceId};
use egui::Color32;

use crate::theme;

/// One player's line.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub name: String,
    pub values: Vec<String>,
    pub me: bool,
}

/// A team's players (or everyone, with no team name, when there are no teams).
#[derive(Debug, Clone, PartialEq)]
pub struct Group {
    pub team: Option<String>,
    /// The team's total of the first column, if it's a number.
    pub total: Option<String>,
    pub rows: Vec<Row>,
}

/// What the board shows: headings and players, best first.
#[derive(Debug, Clone, PartialEq)]
pub struct Board {
    pub headings: Vec<String>,
    pub groups: Vec<Group>,
}

fn attr_num(a: Option<&Attribute>) -> Option<f64> {
    match a {
        Some(Attribute::Num(n)) => Some(*n),
        _ => None,
    }
}

/// A value as the board shows it: whole numbers with commas (1,250),
/// others to one decimal place, text as it is, nothing as a dash.
pub fn show(a: Option<&Attribute>) -> String {
    match a {
        None => "-".into(),
        Some(Attribute::Num(n)) => number(*n),
        Some(Attribute::Str(s)) => s.chars().take(16).collect(),
        Some(Attribute::Bool(b)) => if *b { "yes".into() } else { "no".into() },
    }
}

fn number(n: f64) -> String {
    if !n.is_finite() {
        return "-".into();
    }
    if n.fract() != 0.0 {
        return format!("{:.1}", n);
    }
    let whole = n.abs() as u64;
    let digits = whole.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    if n < 0.0 { format!("-{out}") } else { out }
}

/// Everyone in the game, sorted by the first column (highest first), then
/// by name; grouped by team if anyone has one.
pub fn board(world: &DataModel, me: Option<InstanceId>) -> Board {
    let columns = leaderboard_columns(world);
    let players: Vec<InstanceId> = world.walk().into_iter().filter(|id| world.player(*id).is_some()).collect();
    let team_of = |id: InstanceId| match world.get(id).and_then(|i| i.attributes.get("team")) {
        Some(Attribute::Str(t)) if !t.trim().is_empty() => Some(t.clone()),
        _ => None,
    };
    let first = |id: InstanceId| {
        columns.first().and_then(|c| attr_num(world.get(id).and_then(|i| i.attributes.get(c)))).unwrap_or(f64::NEG_INFINITY)
    };
    let mut sorted = players.clone();
    sorted.sort_by(|a, b| {
        first(*b)
            .total_cmp(&first(*a))
            .then_with(|| world.get(*a).map(|i| i.name.to_lowercase()).cmp(&world.get(*b).map(|i| i.name.to_lowercase())))
    });
    let row = |id: InstanceId| {
        let inst = world.get(id).unwrap();
        Row { name: inst.name.clone(), values: columns.iter().map(|c| show(inst.attributes.get(c))).collect(), me: Some(id) == me }
    };

    let mut groups: Vec<Group> = Vec::new();
    if sorted.iter().any(|id| team_of(*id).is_some()) {
        let mut teams: Vec<Option<String>> = Vec::new();
        for id in &sorted {
            let t = team_of(*id);
            if !teams.contains(&t) {
                teams.push(t);
            }
        }
        for t in teams {
            let members: Vec<InstanceId> = sorted.iter().copied().filter(|id| team_of(*id) == t).collect();
            let total = columns.first().and_then(|c| {
                let nums: Vec<f64> = members.iter().filter_map(|id| attr_num(world.get(*id).and_then(|i| i.attributes.get(c)))).collect();
                (!nums.is_empty()).then(|| number(nums.iter().sum()))
            });
            groups.push(Group { team: Some(t.unwrap_or_else(|| "No team".into())), total, rows: members.into_iter().map(row).collect() });
        }
        // The team with the most first, like the players.
        groups.sort_by(|a, b| {
            let v = |g: &Group| g.total.as_deref().and_then(|t| t.replace(',', "").parse::<f64>().ok()).unwrap_or(f64::NEG_INFINITY);
            v(b).total_cmp(&v(a))
        });
    } else {
        groups.push(Group { team: None, total: None, rows: sorted.into_iter().map(row).collect() });
    }
    Board { headings: columns.iter().map(|c| leaderboard_title(c)).collect(), groups }
}

/// Whether there's a board to show: the game asked for one, or it's a
/// game with company (a list of who's here is useful then).
pub fn wanted(world: &DataModel) -> bool {
    !leaderboard_columns(world).is_empty() || world.walk().into_iter().filter(|id| world.player(*id).is_some()).count() > 1
}

/// A team's colour, from its name ("Red", "blue team", ...); grey if it
/// doesn't name one.
pub fn team_color(team: &str) -> Color32 {
    let t = team.to_lowercase();
    const NAMED: [(&str, [u8; 3]); 12] = [
        ("red", [196, 40, 28]),
        ("blue", [13, 105, 172]),
        ("green", [40, 127, 71]),
        ("yellow", [245, 205, 48]),
        ("orange", [218, 133, 65]),
        ("purple", [107, 50, 124]),
        ("pink", [255, 102, 204]),
        ("white", [242, 243, 243]),
        ("black", [27, 42, 53]),
        ("cyan", [4, 175, 236]),
        ("gold", [217, 169, 15]),
        ("brown", [105, 64, 40]),
    ];
    NAMED.iter().find(|(n, _)| t.contains(n)).map(|(_, c)| theme::rgb(*c)).unwrap_or(Color32::from_rgb(99, 95, 98))
}

const ROW_H: f32 = 19.0;
const HEAD_H: f32 = 22.0;
const NAME_W: f32 = 132.0;
const COL_W: f32 = 64.0;
const ARROW_W: f32 = 22.0;

/// Draws the board in the top-right corner of `area` (the window, or
/// Studio's view), if the game has one: the classic list, bold white names
/// outlined in black on see-through black, yours in yellow. `open` is
/// whether it's unfolded; clicking its heading flips it.
pub fn draw(ctx: &egui::Context, area: egui::Rect, world: &DataModel, me: Option<InstanceId>, open: &mut bool) {
    use crate::classic;
    if !wanted(world) {
        return;
    }
    let b = board(world, me);
    // Name, the columns, and room for the fold arrow.
    let width = NAME_W + COL_W * b.headings.len() as f32 + ARROW_W;
    egui::Area::new(egui::Id::new("leaderboard"))
        .fixed_pos((area.right_top() + egui::vec2(-8.0 - width, 6.0)).round())
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
            let lines: usize = if *open { b.groups.iter().map(|g| g.rows.len() + g.team.is_some() as usize).sum() } else { 0 };
            let body_h = if *open { lines as f32 * ROW_H + 5.0 } else { 0.0 };
            let (whole, _) = ui.allocate_exact_size(egui::vec2(width, HEAD_H + body_h), egui::Sense::hover());
            let head = egui::Rect::from_min_size(whole.min, egui::vec2(width, HEAD_H));
            let response = ui.interact(head, ui.id().with("fold"), egui::Sense::click());
            let p = ui.painter();
            classic::panel(p, whole);
            // The heading: a little darker, with a line under it.
            p.rect_filled(head.shrink(1.0), 0.0, egui::Color32::from_black_alpha(if response.hovered() { 40 } else { 70 }));
            let font = classic::bold(13.0);
            classic::text(p, head.left_center() + egui::vec2(7.0, 0.0), egui::Align2::LEFT_CENTER, "Players", font.clone(), Color32::WHITE);
            if *open {
                for (i, h) in b.headings.iter().enumerate() {
                    let x = head.left() + NAME_W + COL_W * (i as f32 + 1.0);
                    classic::text(p, egui::pos2(x, head.center().y), egui::Align2::RIGHT_CENTER, h, font.clone(), Color32::WHITE);
                }
                p.rect_filled(egui::Rect::from_min_size(egui::pos2(head.left() + 1.0, head.bottom() - 1.0), egui::vec2(width - 2.0, 1.0)), 0.0, egui::Color32::from_white_alpha(70));
            }
            // The fold arrow: up when open (fold it up), down when folded.
            let c = head.right_center() - egui::vec2(ARROW_W / 2.0, 0.0);
            let (dx, dy) = (4.5, if *open { -2.5 } else { 2.5 });
            p.add(egui::Shape::convex_polygon(
                vec![egui::pos2(c.x - dx, c.y - dy), egui::pos2(c.x + dx, c.y - dy), egui::pos2(c.x, c.y + dy)],
                Color32::WHITE,
                egui::Stroke::new(1.0, Color32::BLACK),
            ));
            if response.on_hover_text("Tab folds the leaderboard away (and back)").clicked() {
                *open = !*open;
            }
            if !*open {
                return;
            }
            let mut y = head.bottom() + 2.0;
            let small = classic::bold(13.0);
            for g in &b.groups {
                if let Some(team) = &g.team {
                    let r = egui::Rect::from_min_size(egui::pos2(whole.left() + 2.0, y + 1.0), egui::vec2(width - 4.0, ROW_H - 2.0));
                    let c = team_color(team);
                    p.rect_filled(classic::snap(r), 0.0, c.gamma_multiply(0.8));
                    classic::edge(p, r, egui::Color32::from_black_alpha(140));
                    classic::text(p, egui::pos2(r.left() + 6.0, r.center().y), egui::Align2::LEFT_CENTER, team, small.clone(), Color32::WHITE);
                    if let Some(t) = &g.total {
                        classic::text(p, egui::pos2(whole.left() + NAME_W + COL_W, r.center().y), egui::Align2::RIGHT_CENTER, t, small.clone(), Color32::WHITE);
                    }
                    y += ROW_H;
                }
                for row in &g.rows {
                    let r = egui::Rect::from_min_size(egui::pos2(whole.left(), y), egui::vec2(width, ROW_H));
                    if row.me {
                        p.rect_filled(classic::snap(r.shrink2(egui::vec2(2.0, 0.0))), 0.0, egui::Color32::from_white_alpha(28));
                    }
                    let color = if row.me { classic::YELLOW } else { Color32::WHITE };
                    let name: String = row.name.chars().take(18).collect();
                    classic::text(p, egui::pos2(r.left() + 8.0, r.center().y), egui::Align2::LEFT_CENTER, &name, small.clone(), color);
                    for (i, v) in row.values.iter().enumerate() {
                        let x = r.left() + NAME_W + COL_W * (i as f32 + 1.0);
                        classic::text(p, egui::pos2(x, r.center().y), egui::Align2::RIGHT_CENTER, v, small.clone(), color);
                    }
                    y += ROW_H;
                }
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use brixo_core::{Class, LEADERBOARD_FIELD};

    fn game(columns: Option<&str>, players: &[(&str, Option<&str>, f64, Option<f64>)]) -> (DataModel, Vec<InstanceId>) {
        let mut dm = DataModel::new();
        let root = dm.root();
        if let Some(c) = columns {
            dm.get_mut(root).unwrap().attributes.insert(LEADERBOARD_FIELD.into(), Attribute::Str(c.into()));
        }
        let mut ids = Vec::new();
        for (name, team, coins, wins) in players {
            let id = dm.create(Class::Player, name, root).unwrap();
            let a = &mut dm.get_mut(id).unwrap().attributes;
            a.insert("coins".into(), Attribute::Num(*coins));
            if let Some(w) = wins {
                a.insert("wins".into(), Attribute::Num(*w));
            }
            if let Some(t) = team {
                a.insert("team".into(), Attribute::Str(t.to_string()));
            }
            ids.push(id);
        }
        (dm, ids)
    }

    #[test]
    fn players_are_listed_best_first_with_their_stats() {
        let (dm, ids) = game(Some("coins,wins"), &[("ann", None, 50.0, Some(2.0)), ("Bob", None, 1250.0, None), ("cat", None, 50.0, Some(0.5))]);
        let b = board(&dm, Some(ids[0]));
        assert_eq!(b.headings, vec!["Coins", "Wins"]);
        assert_eq!(b.groups.len(), 1);
        let rows = &b.groups[0].rows;
        assert_eq!(rows.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(), ["Bob", "ann", "cat"], "most coins first, ties by name");
        assert_eq!(rows[0].values, ["1,250", "-"]);
        assert_eq!(rows[2].values, ["50", "0.5"]);
        assert!(rows[1].me && !rows[0].me);
    }

    #[test]
    fn teams_are_grouped_in_their_colours_with_totals() {
        let (dm, _) = game(Some("coins"), &[("a", Some("Red"), 5.0, None), ("b", Some("Blue"), 30.0, None), ("c", Some("Red"), 40.0, None), ("d", None, 1.0, None)]);
        let b = board(&dm, None);
        let summary: Vec<(String, Option<String>, usize)> = b.groups.iter().map(|g| (g.team.clone().unwrap(), g.total.clone(), g.rows.len())).collect();
        assert_eq!(summary, [("Red".into(), Some("45".into()), 2), ("Blue".into(), Some("30".into()), 1), ("No team".into(), Some("1".into()), 1)]);
        assert_eq!(team_color("Red team"), theme::rgb([196, 40, 28]));
        assert_eq!(team_color("Wizards"), Color32::from_rgb(99, 95, 98));
    }

    #[test]
    fn a_board_shows_when_the_game_asks_or_theres_company() {
        let (solo, _) = game(None, &[("a", None, 0.0, None)]);
        assert!(!wanted(&solo));
        let (two, _) = game(None, &[("a", None, 0.0, None), ("b", None, 0.0, None)]);
        assert!(wanted(&two));
        assert!(board(&two, None).headings.is_empty());
        let (asked, _) = game(Some("coins"), &[("a", None, 0.0, None)]);
        assert!(wanted(&asked));
    }

    #[test]
    fn values_read_nicely() {
        assert_eq!(number(0.0), "0");
        assert_eq!(number(-1234567.0), "-1,234,567");
        assert_eq!(number(12.345), "12.3");
        assert_eq!(show(Some(&Attribute::Str("Gold league champion".into()))), "Gold league cham");
        assert_eq!(show(Some(&Attribute::Bool(true))), "yes");
        assert_eq!(leaderboard_title("best_time"), "Best Time");
        assert_eq!(leaderboard_title("kos"), "KOs");
        assert_eq!(leaderboard_title("xp"), "XP");
        assert_eq!(leaderboard_title("max_hp"), "Max HP");
        assert_eq!(leaderboard_title("coins"), "Coins");
    }
}
