//! Bots that play Flagfall. The brain only reads the world (what any
//! player's client can see) and answers with keys to press, so the same
//! code drives bots in a headless test and bots joined over the network.
//!
//! Each bot has a job. Attackers take one of the three routes (Bridge,
//! Ruins, Tunnel) to the enemy flag and run it home the same way. Defenders
//! guard their stand. Everyone drops their job for the things that matter
//! more: carrying a flag home, returning your own dropped flag, chasing
//! the enemy who has it, and escorting a team-mate who has theirs.

use brixo_core::{Attribute, Class, DataModel, InstanceId, PlayerProps, Vec3};

use crate::flagfall::STAND_A;

/// Gear slots, in the order everyone gets the kit.
pub const SWORD: usize = 0;
pub const SLINGSHOT: usize = 1;
pub const ROCKET: usize = 2;
pub const SUPERBALL: usize = 3;

/// The ways across, as waypoints from your own stand to the enemy's. `a`
/// runs toward your own side (so +120 is your stand, -120 theirs), `z` is
/// across the map.
pub const ROUTES: [(&str, &[(f32, f32)]); 3] = [
    ("Bridge", &[(120.0, 0.0), (106.0, 0.0), (90.0, 0.0), (32.0, 0.0), (0.0, 0.0), (-32.0, 0.0), (-90.0, 0.0), (-106.0, 0.0), (-120.0, 0.0)]),
    (
        "Ruins",
        &[
            (120.0, 0.0), (117.0, -18.0), (117.0, -25.0), (117.0, -36.0), (60.0, -45.0), (24.0, -45.0), (0.0, -45.0),
            (-24.0, -45.0), (-60.0, -45.0), (-117.0, -36.0), (-117.0, -25.0), (-117.0, -18.0), (-120.0, 0.0),
        ],
    ),
    (
        "Tunnel",
        &[
            (120.0, 0.0), (117.0, 18.0), (117.0, 25.0), (117.0, 36.0), (98.0, 44.0), (58.0, 44.0), (0.0, 44.0),
            (-58.0, 44.0), (-98.0, 44.0), (-117.0, 36.0), (-117.0, 25.0), (-117.0, 18.0), (-120.0, 0.0),
        ],
    ),
];

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Job {
    Attack(usize),
    Defend,
}

/// What the bot wants to do this moment.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct Action {
    pub move_x: f32,
    pub move_z: f32,
    pub jump: bool,
    /// Switch to this backpack slot.
    pub equip: Option<usize>,
    /// Use what's in hand...
    pub activate: bool,
    /// ...aimed here, as a player would click on their target.
    pub aim: Option<Vec3>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Flag {
    Home,
    Carried(InstanceId),
    Dropped(Vec3),
}

pub struct FlagBot {
    pub job: Job,
    /// Where along its route it is, and which way it's going.
    waypoint: usize,
    homeward: bool,
    /// For noticing it's stuck on something.
    last_pos: Vec3,
    stuck_since: f64,
    jump_until: f64,
    hold_until: f64,
    next_shot: f64,
    patrol: f32,
    seed: u64,
    /// Which route it has joined for the run home with a flag.
    home_joined: Option<usize>,
}

fn team(world: &DataModel, id: InstanceId) -> Option<String> {
    match world.get(id)?.attributes.get("team") {
        Some(Attribute::Str(t)) => Some(t.clone()),
        _ => None,
    }
}

fn flat(a: Vec3, b: Vec3) -> f32 {
    ((a.x - b.x).powi(2) + (a.z - b.z).powi(2)).sqrt()
}

fn side(t: &str) -> f32 {
    if t == "Red" { -1.0 } else { 1.0 }
}

fn other(t: &str) -> &'static str {
    if t == "Red" { "Blue" } else { "Red" }
}

/// A route waypoint in the world, for a bot on team `t`.
fn waypoint(t: &str, (a, z): (f32, f32)) -> Vec3 {
    Vec3::new(side(t) * a, 0.0, z)
}

fn stand(t: &str) -> Vec3 {
    Vec3::new(side(t) * STAND_A, 0.0, 0.0)
}

/// Where `t`'s flag is.
fn flag(world: &DataModel, t: &str) -> Flag {
    let Some(pole) = world.find_first(&format!("{t} Flag Pole")) else { return Flag::Home };
    if let Some(Attribute::Str(name)) = world.get(pole).and_then(|i| i.attributes.get("carried_by")) {
        if let Some(p) = world.walk().into_iter().find(|id| world.get(*id).is_some_and(|i| i.class == Class::Player && &i.name == name)) {
            return Flag::Carried(p);
        }
    }
    let at = world.part(pole).map(|p| p.position).unwrap_or(Vec3::new(0.0, 0.0, 0.0));
    if flat(at, stand(t)) < 1.0 { Flag::Home } else { Flag::Dropped(at) }
}

impl FlagBot {
    /// The n-th bot on a team: one in four defends, the rest spread over
    /// the three routes.
    pub fn new(n: usize, seed: u64) -> FlagBot {
        let job = if n % 4 == 3 { Job::Defend } else { Job::Attack(n % 3) };
        FlagBot {
            job,
            waypoint: 0,
            homeward: false,
            last_pos: Vec3::new(0.0, 0.0, 0.0),
            stuck_since: 0.0,
            jump_until: 0.0,
            // Attackers leave at slightly different times.
            hold_until: n as f64 * 1.3,
            home_joined: None,
            next_shot: 0.0,
            patrol: n as f32 * 1.7,
            seed: seed | 1,
        }
    }

    fn rand(&mut self) -> f32 {
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 7;
        self.seed ^= self.seed << 17;
        (self.seed % 10_000) as f32 / 10_000.0
    }

    /// The next point along a route, joining it at the nearest waypoint
    /// ahead when starting out (after a respawn, say).
    fn follow(&mut self, t: &str, route: usize, homeward: bool, at: Vec3) -> Vec3 {
        let points = ROUTES[route].1;
        let n = points.len();
        let index = |i: usize| if homeward { n - 1 - i } else { i };
        if homeward != self.homeward {
            // Starting over in the other direction: join at the closest point.
            self.homeward = homeward;
            self.waypoint = (0..n).min_by(|a, b| flat(waypoint(t, points[index(*a)]), at).total_cmp(&flat(waypoint(t, points[index(*b)]), at))).unwrap_or(0);
        }
        while self.waypoint < n - 1 && flat(waypoint(t, points[index(self.waypoint)]), at) < 3.5 {
            self.waypoint += 1;
        }
        waypoint(t, points[index(self.waypoint)])
    }

    /// Heading home on `route` (which may not be the one it came by).
    fn follow_home(&mut self, t: &str, route: usize, at: Vec3) -> Vec3 {
        if self.home_joined != Some(route) {
            self.home_joined = Some(route);
            self.homeward = false; // makes follow() join this route afresh
        }
        self.follow(t, route, true, at)
    }

    /// Restart the route from the nearest point (after a respawn).
    fn rejoin(&mut self, t: &str, route: usize, at: Vec3) {
        let points = ROUTES[route].1;
        let n = points.len();
        let index = |i: usize| if self.homeward { n - 1 - i } else { i };
        self.waypoint = (0..n).min_by(|a, b| flat(waypoint(t, points[index(*a)]), at).total_cmp(&flat(waypoint(t, points[index(*b)]), at))).unwrap_or(0);
    }

    /// One decision. `now` is game time in seconds.
    pub fn think(&mut self, world: &DataModel, me: InstanceId, now: f64) -> Action {
        let Some(mine) = world.player(me).copied() else { return Action::default() };
        let Some(t) = team(world, me) else { return Action::default() };
        if mine.dead > 0.0 {
            self.waypoint = 0;
            self.stuck_since = now;
            // Back in a moment: not everyone leaves the spawn at once.
            self.hold_until = now + 4.0 + self.rand() as f64 * 3.0;
            return Action::default();
        }
        if now < self.hold_until && matches!(self.job, Job::Attack(_)) {
            return Action::default();
        }
        if self.last_pos == Vec3::new(0.0, 0.0, 0.0) {
            self.last_pos = mine.body.position;
        }
        let at = mine.body.position;
        let carrying = matches!(world.get(me).and_then(|i| i.attributes.get("carrying")), Some(Attribute::Str(_)));
        let ours = flag(world, &t);
        let theirs = flag(world, other(&t));

        // Who's around.
        let people: Vec<(InstanceId, PlayerProps)> = world
            .walk()
            .into_iter()
            .filter(|id| *id != me)
            .filter_map(|id| world.player(id).map(|p| (id, *p)))
            .filter(|(_, p)| p.dead == 0.0 && p.health > 0.0)
            // (Not someone on the ground over your head while you're in
            // the tunnel: you can't reach each other.)
            .filter(|(_, p)| (p.body.position.y - mine.body.position.y).abs() < 4.5)
            .collect();
        let enemies: Vec<(InstanceId, Vec3)> =
            people.iter().filter(|(id, _)| team(world, *id).as_deref() != Some(t.as_str())).map(|(id, p)| (*id, p.body.position)).collect();
        let nearest_enemy = enemies.iter().copied().min_by(|a, b| flat(a.1, at).total_cmp(&flat(b.1, at)));

        // The two teams hand out routes in a different order, so attackers
        // don't all meet their mirror image head-on in the middle.
        let route = match self.job {
            Job::Attack(r) => (r + if t == "Blue" { 1 } else { 0 }) % ROUTES.len(),
            Job::Defend => 0,
        };
        // Where to go, and whether fighting on the way is worth it.
        if !carrying {
            self.home_joined = None;
        }
        let (goal, fight_range): (Vec3, f32) = if carrying {
            // Run it home the way it came. Don't stop to fight.
            (self.follow_home(&t, route, at), 0.0)
        } else if let Flag::Dropped(p) = ours {
            if matches!(self.job, Job::Defend) || flat(p, at) < 70.0 { (p, 10.0) } else { self.job_goal(&t, route, at, world, &enemies) }
        } else if let Flag::Carried(thief) = ours {
            let tp = world.player(thief).map(|p| p.body.position).unwrap_or(at);
            if matches!(self.job, Job::Defend) || flat(tp, at) < 60.0 {
                (tp, 30.0)
            } else {
                self.job_goal(&t, route, at, world, &enemies)
            }
        } else if let Flag::Dropped(p) = theirs {
            if flat(p, at) < 45.0 { (p, 8.0) } else { self.job_goal(&t, route, at, world, &enemies) }
        } else if let Flag::Carried(mate) = theirs {
            // A team-mate has it: go with them, a few studs behind.
            let mp = world.player(mate).map(|p| p.body.position).unwrap_or(at);
            if flat(mp, at) < 80.0 { (mp, 25.0) } else { self.job_goal(&t, route, at, world, &enemies) }
        } else {
            self.job_goal(&t, route, at, world, &enemies)
        };

        // Places that trap a straight-line walker: out of them first.
        let goal = self.unstick(&t, at, goal);

        let mut act = Action::default();
        let mut fighting = false;
        let (mut dx, mut dz) = (goal.x - at.x, goal.z - at.z);
        let d = (dx * dx + dz * dz).sqrt();
        if d > 1.2 {
            (dx, dz) = (dx / d, dz / d);
        } else {
            (dx, dz) = (0.0, 0.0);
        }

        // A fight worth having: turn on them and use whatever suits the range.
        if let Some((_, ep)) = nearest_enemy.filter(|(_, p)| fight_range > 0.0 && flat(*p, at) < fight_range) {
            let ed = flat(ep, at).max(0.01);
            let (ex, ez) = ((ep.x - at.x) / ed, (ep.z - at.z) / ed);
            // Face them (a swing or a shot goes where you're facing, and
            // you face the way you move), weaving a little so it isn't a
            // straight line; right up close, mostly just turn toward them.
            self.patrol += (self.rand() - 0.5) * 0.4;
            let weave = (self.patrol * 3.0).sin() * if ed < 3.0 { 0.2 } else { 0.35 };
            (dx, dz) = (ex - ez * weave, ez + ex * weave);
            fighting = true;
            let slot = if ed < 7.0 {
                SWORD
            } else if ed < 16.0 {
                if self.rand() < 0.5 { SLINGSHOT } else { SUPERBALL }
            } else if ed < 45.0 {
                ROCKET
            } else {
                SLINGSHOT
            };
            let held = mine.equipped.and_then(|tool| world.get(me).and_then(|p| p.children.iter().filter(|c| world.get(**c).is_some_and(|i| i.class == Class::Tool)).position(|c| *c == tool)));
            if now >= self.next_shot {
                if held != Some(slot) {
                    act.equip = Some(slot);
                    self.next_shot = now + 0.25;
                } else {
                    act.activate = true;
                    // Clicked roughly on them: people miss, and a bot with
                    // perfect aim would knock out everyone it meets.
                    let wobble = 2.5 + ed * 0.12;
                    act.aim = Some(Vec3::new(
                        ep.x + (self.rand() - 0.5) * 2.0 * wobble,
                        ep.y + (self.rand() - 0.5) * wobble,
                        ep.z + (self.rand() - 0.5) * 2.0 * wobble,
                    ));
                    let wait = match slot {
                        SWORD => 0.45,
                        SLINGSHOT => 0.35,
                        ROCKET => 3.1,
                        _ => 1.9,
                    };
                    self.next_shot = now + wait + self.rand() as f64 * 0.4;
                }
            }
        } else if !carrying && mine.equipped.is_none() {
            act.equip = Some(SWORD);
        }

        // Stuck on something: hop, and slide sideways a little.
        if flat(at, self.last_pos) > 1.5 {
            self.last_pos = at;
            self.stuck_since = now;
        } else if !fighting && d > 2.0 && now - self.stuck_since > 1.2 {
            self.jump_until = now + 0.35;
            self.stuck_since = now;
            let s = if self.rand() < 0.5 { 1.0 } else { -1.0 };
            (dx, dz) = (dx * 0.5 + dz * s, dz * 0.5 - dx * s);
        }
        act.jump = now < self.jump_until;
        act.move_x = dx;
        act.move_z = dz;
        act
    }

    /// Detours for the two spots a straight line gets stuck in: inside a
    /// spawn house (out through its door first), and in the river under the
    /// bridge (whose banks there are walls: wade out to open bank first).
    fn unstick(&self, t: &str, at: Vec3, goal: Vec3) -> Vec3 {
        for team in ["Red", "Blue"] {
            let s = side(team);
            let a = at.x * s;
            if a > 126.5 && a < 138.0 && at.z.abs() < 10.5 && at.y < 6.0 {
                let ga = goal.x * s;
                let outside = ga < 126.0 || goal.z.abs() > 11.0;
                if outside {
                    // The door faces the middle, at z -4..4.
                    return if at.z.abs() > 2.5 { Vec3::new(s * 130.0, 0.0, 0.0) } else { Vec3::new(s * 122.0, 0.0, 0.0) };
                }
            }
        }
        let _ = t;
        let in_river = at.x.abs() < 10.5 && at.y < 1.0 && at.y > -6.0;
        if in_river && at.z.abs() < 9.5 && (goal.x - at.x).abs() > 4.0 {
            let away = if at.z >= 0.0 { 12.5 } else { -12.5 };
            return Vec3::new(at.x, 0.0, away);
        }
        goal
    }

    /// Where a bot's own job takes it when nothing more urgent is going on.
    fn job_goal(&mut self, t: &str, route: usize, at: Vec3, _world: &DataModel, enemies: &[(InstanceId, Vec3)]) -> (Vec3, f32) {
        match self.job {
            Job::Attack(_) => {
                if self.homeward {
                    self.homeward = false;
                    self.rejoin(t, route, at);
                }
                // On the way in, only fight what's in your face: the flag
                // is the point.
                (self.follow(t, route, false, at), 7.0)
            }
            Job::Defend => {
                let home = stand(t);
                // Anyone coming for the flag gets met.
                if let Some((_, p)) = enemies.iter().copied().filter(|(_, p)| flat(*p, home) < 40.0).min_by(|a, b| flat(a.1, at).total_cmp(&flat(b.1, at))) {
                    return (p, 40.0);
                }
                // Otherwise walk a slow circle round the stand.
                self.patrol += 0.01;
                let r = 9.0;
                (Vec3::new(home.x + r * self.patrol.cos() - side(t) * 4.0, 0.0, home.z + r * self.patrol.sin()), 35.0)
            }
        }
    }
}
