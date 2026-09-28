Every person in your game is a **player** object. Scripts get players from events (`on player_joined(p)`, `on touched(other)`...) or from `players()`, a list of everyone playing right now.

```rovik run
every 5 seconds
    for p in players() do
        print(p.name, "is at", p.position, "with", p.health, "health")
    end
end
```

## What a player has

| Property | | What it is |
|---|---|---|
| `name` | read | Their Brixo username. |
| `position` | read/set | Where they are. Setting it **teleports** them. |
| `velocity` | set | Setting it **launches** them. See [Physics](physics). |
| `health` | read/set | 0 to `max_health`. At 0 they're knocked out. |
| `max_health` | read/set | 100 to start with. |
| `walk_speed` | read/set | Studs per second. 16 to start with. |
| `jump_power` | read/set | How hard they jump. 39 to start with (about 7 studs). |
| `face` | read/set | `"smile"`, `"happy"`, `"surprised"` or `"determined"`. |
| `skin_color`, `shirt_color`, `pants_color` | read/set | `{r, g, b}` colors. Players pick their own on the website; games can change them (team shirts). |
| `camera_mode` | read/set | `"default"`, `"first_person"` or `"third_person"`. |
| `look` | read | Which way they're facing, flat: `{x, y = 0, z}`. |
| `mouse` | read | The spot in the world they last clicked with a tool. See [Tools](tools). |
| `equipped` | read | The Tool in their hand, or `nil`. |
| `swinging` | read | `true` for a moment after they click with a tool. |
| `children` | read | What's inside them: their tools, and their own GUI. |

## Health and damage

```rovik run in=part:Spikes
on touched(other)
    if other.class == "player" then
        other.health -= 20
        play_sound_at("hit", other)
    end
end
```

Health never goes above `max_health`, and never below 0. At 0 the player falls apart, `on died` runs, and they're back 4 seconds later with full health. To heal: `p.health = p.max_health`.

## Faster, higher, bigger

```rovik run
on player_joined(p)
    p.walk_speed = 24     -- a speedy game
    p.jump_power = 55     -- moon jumps
    p.max_health = 200
    p.health = 200
end
```

`walk_speed = 0` and `jump_power = 0` freeze someone in place: handy while a round is starting.

## Your own values: custom fields

Set **any name** on a player and it's stored on them for as long as they're in the game: coins, kills, level, which checkpoint they reached. These are **custom fields**. They can hold numbers, text, or `true`/`false`:

```rovik run
on player_joined(p)
    p.coins = 0
    p.level = 1
    p.title = "Newbie"
    p.vip = false
end

on died(p)
    p.coins = max(0, p.coins - 5)   -- lose a few coins
end
```

A custom field that was never set reads as `nil`. That's handy for checking (`if p.coins == nil then p.coins = 0 end`), but it means a **typo gives you `nil` instead of an error**: `p.coisn` is just `nil`. If a number seems stuck at nothing, check the spelling.

One catch: a custom field **can't be a near-miss of a real property**. On a part, `self.stage = 1` is an error ("Did you mean 'shape'?"), because Brixo guesses you mistyped `shape`. Pick a clearly different name, like `stage_number`.

Custom fields work on **any object**, not just players: a door can have `door.locked = true`, a part can remember `pad.uses = 3`. Brixo gives a few special meaning: `team`, `breakable`, `carried_by`, `beacon` (see [Physics](physics)) and `grip` (see [Tools](tools)).

> **Note:** Custom fields last until the player leaves. Saving coins between visits is on Brixo's list, but not here yet.

## Showing values on screen

Custom fields are invisible. To show someone their coins, put a TextLabel inside them and update it:

```rovik run
on player_joined(p)
    p.coins = 0
    label = create("TextLabel", p)
    label.name = "Coins Label"
    label.text = "Coins: 0"
    label.x = 0.02
    label.y = 0.1
    label.width = 0.16
    label.height = 0.06
    label.background = true
end

fn give_coins(p, n)
    p.coins += n
    for c in p.children do
        if c.name == "Coins Label" then
            c.text = "Coins: " + p.coins
        end
    end
end
```

To show everyone's coins in the leaderboard in the corner, it's one line: `leaderboard("coins")`. The full recipe is in [Coins and a leaderboard](howto-coins). All about on-screen text: [On-screen interface](gui).

## Faces and colors

```rovik run in=part:Paint_Bucket
on touched(other)
    if other.class == "player" then
        other.shirt_color = {r = random(0, 255), g = random(0, 255), b = random(0, 255)}
        other.face = "surprised"
    end
end
```

## The camera

`p.camera_mode` sets how a player's camera works:

- `"default"`: they choose. Scroll out for third person, all the way in for first person.
- `"first_person"`: locked in first person, like a shooter.
- `"third_person"`: locked behind their character.

```rovik run
on player_joined(p)
    p.camera_mode = "first_person"
end
```

Next: [On-screen interface](gui).
