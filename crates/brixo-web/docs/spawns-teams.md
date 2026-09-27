## Spawn locations

A **SpawnLocation** is a pad players appear on. **Add Spawn** in Studio makes one. When a game starts, players appear on the **first** SpawnLocation in the game, spread out a little so they don't land inside each other. You can size, color and move it like any part.

Knocked-out players come back **4 seconds** later, with full health, at their spawn.

## Moving players

Set a player's `position` to teleport them instantly:

```rovik run
on player_joined(p)
    p.position = {x = 0, y = 20, z = 40}   -- start high up, and fall in
end
```

Aim a little above the ground (a player's middle is about 2.5 studs above their feet), so they don't start stuck in the floor. To send someone to a part: `p.position = {x = pad.position.x, y = pad.position.y + 4, z = pad.position.z}`. There's a full version in [Teleporters](howto-teleporter).

## Teams

Teams in Brixo are simple: a **team is a name**, stored in a custom field called `team`. Give each team a SpawnLocation with the same `team`, and players on that team respawn on their own pad.

1. Add a SpawnLocation for each team. Name them **Red Spawn** and **Blue Spawn**, and color them.
2. Give each pad its team, with a script inside the pad, or one script for both (below).
3. When a player joins, pick their team, color their shirt, and send them to their pad.

```rovik run with=spawn:Red_Spawn,spawn:Blue_Spawn
-- Two teams, kept even.
find("Red Spawn").team = "Red"
find("Blue Spawn").team = "Blue"

fn count(team)
    n = 0
    for p in players() do
        if p.team == team then
            n += 1
        end
    end
    return n
end

fn send_home(p)
    pad = find(p.team + " Spawn")
    p.position = {x = pad.position.x, y = pad.position.y + 4, z = pad.position.z}
end

on player_joined(p)
    if count("Red") <= count("Blue") then
        p.team = "Red"
        p.shirt_color = {r = 196, g = 40, b = 28}
    else
        p.team = "Blue"
        p.shirt_color = {r = 13, g = 105, b = 172}
    end
    send_home(p)
end
```

From then on, whenever a Red player is knocked out, they respawn on the pad whose `team` is `"Red"`. Brixo's weapons already know about teams: they never hurt teammates. See [A team game](howto-teams) for a complete game with scores.

> **Note:** Brixo puts a new player on the first SpawnLocation it finds, before your `on player_joined` runs, so it's your script that sends them to their team's pad the first time, as above. After that, respawning uses the team pad by itself.

## Falling off the world

A player who falls below **y = -60** is knocked out and respawns. Loose parts that fall that far are removed. Build your whole game above that, and if your map has edges to fall off, that's how it works.

Next: [Physics, explosions and breaking](physics).
