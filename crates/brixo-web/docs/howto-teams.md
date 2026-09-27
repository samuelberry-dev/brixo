Red against Blue: teams with their own spawns and shirt colors, swords for everyone, knockouts scored for your team, and the first team to 10 wins the round.

## Set up the map

- Two **SpawnLocations**, one at each end of the map, named **Red Spawn** and **Blue Spawn**, colored red and blue.
- A shared **TextLabel** named **Score** across the top (x `0.3`, y `0.02`, width `0.4`, height `0.07`, background on).
- A **Storage** folder with a **Sword** tool in it (the next section makes it).

## The sword

Build a sword with **Add Part → Tool** (a handle, a crossguard, and a long thin blade pointing along +z), name the Tool **Sword**, and put it in **Storage**. Put this script inside the Tool:

```rovik run in=tool:Sword
self.grip = "up"     -- held straight up, chops forward
ready_at = 0

on activated(p)
    if time() < ready_at then
        return
    end
    ready_at = time() + 0.5
    play_sound("whoosh")
    f = p.look
    for other in players() do
        -- An enemy: someone else, still up, on another team.
        if other != p and other.health > 0 and other.team != p.team then
            dx = other.position.x - p.position.x
            dz = other.position.z - p.position.z
            d = sqrt(dx * dx + dz * dz)
            ahead = (dx * f.x + dz * f.z) / max(d, 0.01)
            -- Close, in front of you, and not far above or below.
            if d < 7 and ahead > 0.3 and abs(other.position.y - p.position.y) < 4 then
                other.health -= 25
                other.last_hit_by = p.name
                play_sound("hit")
            end
        end
    end
end
```

How the swing works: `d` is how far away each other player is, and `ahead` is how much they're **in front of you** (1 is straight ahead, 0 is to your side, below 0 is behind). The sword hits everyone close, in front, and on the other team. It also writes the attacker's name on the victim as `last_hit_by`, so the game knows who to credit when they're knocked out.

## The game script

One script in the **Workspace**:

```rovik run with=spawn:Red_Spawn,spawn:Blue_Spawn,textlabel:Score,tool:Sword
TO_WIN = 10
find("Red Spawn").team = "Red"
find("Blue Spawn").team = "Blue"
scores = {Red = 0, Blue = 0}

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
    p.position = {x = pad.position.x + random(-3, 3), y = pad.position.y + 4, z = pad.position.z + random(-3, 3)}
end

fn show_score()
    find("Score").text = "RED " + scores.Red + "   vs   " + scores.Blue + " BLUE"
end

on player_joined(p)
    -- The smaller team gets the new player.
    if count("Red") <= count("Blue") then
        p.team = "Red"
        p.shirt_color = {r = 196, g = 40, b = 28}
    else
        p.team = "Blue"
        p.shirt_color = {r = 13, g = 105, b = 172}
    end
    sword = clone(find("Sword"))
    sword.parent = p
    send_home(p)
end

on died(p)
    if p.last_hit_by == nil then
        return
    end
    killer = find(p.last_hit_by)
    p.last_hit_by = nil
    if killer == nil or killer.team == p.team then
        return
    end
    scores[killer.team] += 1
    show_score()
    if scores[killer.team] >= TO_WIN then
        find("Score").text = killer.team + " TEAM WINS!"
        play_sound("win")
        wait(5)
        scores = {Red = 0, Blue = 0}
        show_score()
        for q in players() do
            q.health = q.max_health
            send_home(q)
        end
    end
end

show_score()
```

What each part does:

- **Teams:** `find("Red Spawn").team = "Red"` marks each pad with its team, so knocked-out players respawn at their own end. New players join the smaller team, get the team's shirt color, a sword, and are sent to their pad.
- **Scoring:** when someone's knocked out, `on died` looks up who hit them last (`last_hit_by`, set by the sword), and gives that player's team a point. Knocking yourself out, or being knocked out by a teammate, scores nothing.
- **Winning:** at 10, the winning team is announced, and after 5 seconds the scores reset and everyone's sent home for the next round.

## Make it your own

- **More weapons:** anything that sets `other.last_hit_by = p.name` when it hurts someone scores just the same. [Make your own gun](howto-gun) does.
- **A time limit:** combine this with [Timed rounds](howto-rounds): the round ends at 10 points or when time runs out, whichever comes first.
- **Capture the flag, or a king of the hill:** see how Flagfall does it in [The sample games, explained](samples).
