//! `cargo run -p brixo-server --example bots -- ADDR COUNT SECONDS`
//!
//! Bot players for testing and filming: they join a Spire Wars server as
//! ordinary network clients and fight. Each finds the nearest enemy, runs at
//! them (off its tower, across the map), fires rockets and superballs on the
//! way in, and switches to the sword up close. Now and then it drops a
//! timebomb. Everything they do goes through the normal player protocol.
//!
//! On a Flagfall server they play capture the flag instead, with the same
//! brain the headless tests use (`brixo_samples::flagbots`): attackers on
//! each route, a defender per four, chasing carriers, returning flags.

use std::time::{Duration, Instant};

use brixo_core::{Attribute, InstanceId};
use brixo_runtime::PlayerInput;
use brixo_server::NetClient;

const NAMES: [&str; 8] = ["Blox", "Nova", "Rex", "Kit", "Zed", "Mo", "Ash", "Pip"];
// Backpack slots (the order Spire Wars gives tools in).
const SWORD: usize = 0;
const ROCKET: usize = 1;
const SUPERBALL: usize = 2;
const TIMEBOMB: usize = 5;

struct Bot {
    net: NetClient,
    /// Set once the bot knows its team, on a capture-the-flag server.
    flag_brain: Option<brixo_samples::flagbots::FlagBot>,
    next_shot: Instant,
    jump_until: Instant,
    wander: f32,
}

fn team(net: &NetClient, id: InstanceId) -> Option<String> {
    match net.world.get(id)?.attributes.get("team") {
        Some(Attribute::Str(t)) => Some(t.clone()),
        _ => None,
    }
}

fn rand(seed: &mut u64) -> f32 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 7;
    *seed ^= *seed << 17;
    (*seed % 10_000) as f32 / 10_000.0
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let addr = args.get(1).cloned().unwrap_or_else(|| "127.0.0.1:4570".into());
    let count: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(6);
    let seconds: f32 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(60.0);
    let mut seed = 0x9E37_79B9_7F4A_7C15u64;

    let mut bots: Vec<Bot> = (0..count)
        .map(|i| {
            let net = NetClient::connect(&addr, NAMES[i % NAMES.len()]).expect("couldn't reach the server");
            std::thread::sleep(Duration::from_millis(150));
            Bot { net, flag_brain: None, next_shot: Instant::now() + Duration::from_secs(2), jump_until: Instant::now(), wander: 0.0 }
        })
        .collect();

    let time_scale: f64 = std::env::var("BRIXO_TIME_SCALE").ok().and_then(|s| s.parse().ok()).unwrap_or(1.0);
    let start = Instant::now();
    let mut per_team: std::collections::HashMap<String, usize> = Default::default();
    while start.elapsed().as_secs_f32() < seconds {
        for i in 0..bots.len() {
            let bot = &mut bots[i];
            bot.net.poll();
            let Some(me) = bot.net.me else { continue };
            // Capture the flag: the shared brain decides everything.
            if bot.net.world.find_first("Red Flag Pole").is_some() {
                if bot.flag_brain.is_none() {
                    let Some(t) = team(&bot.net, me) else { continue };
                    let n = per_team.entry(t).or_insert(0);
                    bot.flag_brain = Some(brixo_samples::flagbots::FlagBot::new(*n, 0x5EED + i as u64 * 7919));
                    *n += 1;
                }
                // (In step with a slowed-down server when filming.)
                let now = start.elapsed().as_secs_f64() * time_scale;
                let act = bot.flag_brain.as_mut().unwrap().think(&bot.net.world, me, now);
                bot.net.send_input(PlayerInput { move_x: act.move_x, move_z: act.move_z, jump: act.jump });
                if let Some(slot) = act.equip {
                    bot.net.equip(slot);
                }
                if act.activate {
                    match act.aim {
                        Some(aim) => bot.net.activate_at(aim),
                        None => bot.net.activate(),
                    }
                }
                continue;
            }
            let Some(mine) = bot.net.world.player(me).copied() else { continue };
            if mine.dead > 0.0 {
                bot.net.send_input(PlayerInput::default());
                continue;
            }
            let my_team = team(&bot.net, me);
            let at = mine.body.position;
            // The nearest living enemy.
            let target = bot
                .net
                .world
                .walk()
                .into_iter()
                .filter(|id| *id != me)
                .filter_map(|id| bot.net.world.player(id).map(|p| (id, *p)))
                .filter(|(id, p)| p.dead == 0.0 && team(&bot.net, *id) != my_team)
                .map(|(_, p)| {
                    let (dx, dz) = (p.body.position.x - at.x, p.body.position.z - at.z);
                    (dx, dz, (dx * dx + dz * dz).sqrt())
                })
                .min_by(|a, b| a.2.total_cmp(&b.2));
            if std::env::var_os("BOTS_DEBUG").is_some() && (start.elapsed().as_millis() / 33) % 60 == i as u128 {
                eprintln!("{} team {:?} at ({:.0},{:.0},{:.0}) target {:?} holding {:?} health {}", NAMES[i], my_team, at.x, at.y, at.z, target.map(|t| t.2 as i32), mine.equipped, mine.health);
            }
            let Some((dx, dz, dist)) = target else {
                bot.net.send_input(PlayerInput::default());
                continue;
            };

            // Run at them, weaving a little so fights aren't straight lines.
            bot.wander += (rand(&mut seed) - 0.5) * 0.3;
            bot.wander = bot.wander.clamp(-0.6, 0.6);
            let (mut mx, mut mz) = (dx / dist.max(0.01), dz / dist.max(0.01));
            let (c, s) = (bot.wander.cos(), bot.wander.sin());
            (mx, mz) = (mx * c - mz * s, mx * s + mz * c);
            if dist < 3.5 {
                (mx, mz) = (mz * 0.6, -mx * 0.6); // circle them up close
            }
            let now = Instant::now();
            if rand(&mut seed) < 0.01 {
                bot.jump_until = now + Duration::from_millis(250);
            }
            bot.net.send_input(PlayerInput { move_x: mx, move_z: mz, jump: now < bot.jump_until });

            // Pick a weapon for the distance, and use it when it's ready.
            let tools: Vec<InstanceId> = bot
                .net
                .world
                .get(me)
                .map(|p| p.children.iter().copied().filter(|c| bot.net.world.get(*c).is_some_and(|i| i.class == brixo_core::Class::Tool)).collect())
                .unwrap_or_default();
            let roll = rand(&mut seed);
            let wanted = if dist < 7.0 {
                SWORD
            } else if dist < 16.0 && roll < 0.02 {
                TIMEBOMB
            } else if dist < 60.0 {
                if roll < 0.5 { ROCKET } else { SUPERBALL }
            } else {
                SWORD
            };
            if now >= bot.next_shot {
                let holding = mine.equipped;
                if tools.get(wanted).copied() != holding {
                    bot.net.equip(wanted);
                    bot.next_shot = now + Duration::from_millis(250);
                } else if wanted == SWORD && dist < 7.0 || wanted != SWORD {
                    bot.net.activate();
                    let wait = match wanted {
                        SWORD => 450,
                        ROCKET => 1700,
                        TIMEBOMB => 3500,
                        _ => 1200,
                    };
                    bot.next_shot = now + Duration::from_millis(wait + (rand(&mut seed) * 400.0) as u64);
                }
            }
        }
        std::thread::sleep(Duration::from_millis(33));
    }
}
