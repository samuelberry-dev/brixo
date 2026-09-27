Brixo has real physics: loose parts fall, tumble, bounce and knock each other over, and explosions throw them around.

## How players move

| | |
|---|---|
| `walk_speed` | 16 studs a second |
| `jump_power` | 39: a tap jumps about 7 studs high, holding jump about 11 |
| Gravity | pulls things down at 110 studs/s². Falling is a little heavier than rising, so jumps feel snappy |

Players ride along on moving platforms, and get carried by conveyors (anchored parts with a `velocity`). You can change `walk_speed` and `jump_power` for each player at any time; see [Players](players).

## Throwing players

Setting a player's `velocity` **launches** them: up (and down) by its `y`, and sideways by its `x` and `z` as a push that wears off when they land:

```rovik run in=part:Launcher
on touched(other)
    if other.class == "player" then
        other.velocity = {x = 0, y = 90, z = 0}   -- straight up, very high
        play_sound("jump")
    end
end
```

That's how jump pads, cannons and knock-backs work. See [Jump pads and speed pads](howto-pads).

## Loose parts

A part with **Anchored** off falls when the game runs, and can be pushed, stacked and knocked over. A few settings change how they move:

- `bounce` (0 to 1) makes it bouncy.
- `floating = true` switches off its gravity, so it flies in a straight line: rockets and lasers.
- Parts in a **Model** are glued together and move as one.

> **Tip:** Don't build loose parts touching or overlapping each other exactly: physics pushes overlapping parts apart, and a wall of loose bricks can jump apart when the game starts. Leave a tiny gap (0.02 studs is plenty), or build them anchored and let explosions knock them loose (below).

## Explosions

`explode(position, radius)` sets off an explosion: a fireball, a boom everyone hears, loose parts nearby thrown outward, and **every player within the radius knocked out**.

```rovik run in=part:Mine
-- A land mine: step on it and... boom.
gone = false
on touched(other)
    if other.class == "player" and not gone then
        gone = true
        explode(self.position, 10)
        destroy(self)
    end
end
```

An optional third value is the **power**: how hard things get thrown (70 if you leave it out, up to 400). `explode` gives back a **list of the players it knocked out**, so you can reward whoever set it off:

```rovik run
hit = explode({x = 0, y = 2, z = 60}, 8, 120)
for p in hit do
    print(p.name + " got caught in the blast")
end
```

> **Why the `gone` flag in the mine?** `touched` can fire several times in the same instant (both feet land at once, or a rocket touches three bricks). The flag makes sure it only explodes once.

## Breakable buildings

Give an anchored part a custom field **`breakable = true`** and it stays perfectly still until an explosion reaches it. Then it comes loose and flies. Bricks that are left **not connected to the ground** through other bricks fall too, so blowing out the bottom of a tower brings the whole thing down.

```rovik run
-- Build a small tower of breakable bricks.
for row in 0..7 do
    for col in 0..2 do
        b = create("Part", find("Workspace"))
        b.size = {x = 4, y = 2, z = 4}
        b.position = {x = 30 + col * 4, y = 1 + row * 2, z = 0}
        b.material = "brick"
        b.color = {r = 170, g = 80, b = 60}
        b.breakable = true
    end
end
wait(2)
explode({x = 34, y = 1, z = 0}, 5, 100)   -- knock out the bottom
```

This is how the Rocket Launcher and Timebomb bring down towers in Spire Wars. The **ground itself** shouldn't be breakable, or the first rocket would dig a hole to nowhere.

## Carrying things

A part with a custom field **`carried_by`** set to a player's name follows that player around: a flag on their back, a backpack, a glowing orb. `carry_x`, `carry_y` and `carry_z` place it relative to them (`z` is in front, `y` is up), and it turns as they turn:

```rovik run in=part:Orb
self.shape = "ball"
self.material = "neon"
self.can_collide = false
on touched(other)
    if other.class == "player" then
        self.carried_by = other.name
        self.carry_y = 4       -- over their head
        self.carry_z = 0
    end
end
```

Set `carried_by = nil` to let go: it stays where it is. While the player is knocked out, whatever they carry stays where they fell (Flagfall uses this to drop the flag).

## Beacons

A part with **`beacon = true`** gets a marker every player can see **through walls**, at any distance, with how far away it is. When it's off to the side of the screen, the marker sits at the edge, pointing the way. Add `beacon_text` for a label:

```rovik run in=part:Goal
self.beacon = true
self.beacon_text = "Treasure"
```

Great for objectives: the enemy flag, the exit, the next checkpoint.

Next: [Players](players).
