Coins, stages, upgrades: anything a player earns can be **saved**, so it's still there the next time they play your game. It takes two functions:

| Function | What it does |
|---|---|
| `save(player, "name", value)` | Keeps `value` for this player, under that name. |
| `load(player, "name")` | Gives back what was saved under that name, or `nil` if nothing was. |

Each game keeps its own saved data, and each player's is their own: it belongs to their **account**, so it follows them whatever server they land on.

## Saving coins

The usual pattern: **load when they join, save when they leave.**

```rovik run
on player_joined(p)
    -- Whatever they had last time, or 0 the first time they play.
    p.coins = load(p, "coins") or 0
end

on player_left(p)
    save(p, "coins", p.coins)
end
```

`load(p, "coins") or 0` reads "their saved coins, or 0 if there aren't any": `or` gives the value on its right when the one on its left is `nil`.

That's all a [coin game](howto-coins) needs. Put it in the same Workspace script as the coin counter, **before** the label is made, so the label starts at the right number.

## Saving more than one thing

Save each thing under its own name:

```rovik run
on player_joined(p)
    p.stage = load(p, "stage") or 1
    p.best_time = load(p, "best_time")   -- nil until they've finished once
end

on player_left(p)
    save(p, "stage", p.stage)
    if p.best_time != nil then
        save(p, "best_time", p.best_time)
    end
end
```

Or keep them together in a **map**, and lists work too:

```rovik run
on player_joined(p)
    data = load(p, "progress") or {level = 1, xp = 0, badges = []}
    p.level = data.level
    p.xp = data.xp
    p.badges = len(data.badges)
end
```

## Saving straight away

A player whose game crashes, or whose internet cuts out, still "leaves", and `on player_left` still runs: saving there is usually enough. For something that matters the moment it happens (they just bought something), save then too:

```rovik run in=textbutton:Buy_Sword
on clicked(p)
    if p.coins != nil and p.coins >= 100 then
        p.coins -= 100
        save(p, "coins", p.coins)
        save(p, "has_sword", true)
    end
end
```

## What can be saved

- **Numbers, text and true/false**, and **lists and maps** of them (even lists inside maps).
- **Not** parts, players or functions. To remember a part, save its name, and `find` it again later.
- `save(p, "name", nil)` forgets that value.
- Up to **64 KB** per player per game: thousands of numbers. A save that would go over the limit is refused with an error, and what was saved before stays.
- Names are up to 50 letters.

## Testing in Studio

In Studio, saved data is kept from one **Play** to the next until you close Studio or open another game, so you can test "coming back": press Play, earn some coins, Stop, and Play again. It starts empty when Studio does. On the website, it's kept for good.

> **Tip:** Changed what a saved value means (say coins used to be a number and now it's a map)? Give it a new name (`"coins2"`), so old saves don't confuse the new version of your game.
