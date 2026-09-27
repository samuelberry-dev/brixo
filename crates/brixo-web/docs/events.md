A game is mostly **waiting for things to happen**: someone touches a coin, clicks a button, joins, gets knocked out. **Events** are how a script says "when this happens, do that".

## on

An event handler starts with `on`, the event's name, and a name for what it gives you:

```rovik run in=part:Coin
-- Inside a coin: whoever touches it gets it.
on touched(other)
    if other.class == "player" then
        print(other.name + " got the coin!")
        destroy(self)
    end
end
```

The code inside runs **every time** the event happens, for as long as the game runs. A script can have as many handlers as it likes.

## The events

| Event | When | You get |
|---|---|---|
| `on touched(other)` | Something **starts** touching the part this script is in | what touched it: a player or a part |
| `on player_joined(p)` | A player joins the game (including you, when you press Play) | the player |
| `on player_left(p)` | A player leaves | the player |
| `on died(p)` | A player is knocked out (health hit 0, or they fell off the world) | the player |
| `on respawned(p)` | A knocked-out player is back, at their spawn, 4 seconds later | the player |
| `on clicked(p)` | A player clicks the **TextButton** this script is in | who clicked it |
| `on activated(p)` | A player clicks while holding the **Tool** this script is in | who's holding it |

`touched`, `clicked` and `activated` are about the object the script is in, so the script has to be **inside** that part, button or tool. `player_joined`, `player_left`, `died` and `respawned` are about the whole game: they work from any script, anywhere.

> **Note:** `touched` fires once when touching **starts**, not over and over while something stands there. To hurt someone standing in fire every second, see [Kill bricks and lava](howto-kill-brick).

### Who touched it?

`other` in `on touched(other)` is whatever touched the part. Check what it is with `class`:

```rovik run in=part:Pad
on touched(other)
    if other.class == "player" then
        print("A player stepped on me: " + other.name)
    elseif other.class == "part" then
        print("A part bumped into me: " + other.name)
    end
end
```

### A welcome for everyone who joins

```rovik run
on player_joined(p)
    print("Welcome, " + p.name + "! " + len(players()) + " playing now")
    p.coins = 0
    p.walk_speed = 20
end

on player_left(p)
    print(p.name + " left")
end
```

`p.coins = 0` gives the player a **custom field**: a value of your own, stored on them. See [Players](players).

### Knockouts

```rovik run
on died(p)
    print(p.name + " was knocked out")
    if p.last_hit_by != nil then
        print("...by " + p.last_hit_by)
    end
end
```

Players respawn by themselves 4 seconds after being knocked out, and `on respawned(p)` runs when they're back. Moving them there sends them somewhere other than their spawn: a checkpoint, say (see [An obby with checkpoints](howto-checkpoints)). (`last_hit_by` isn't built in: it's a custom field that Brixo's weapons set, so you can tell who got the knockout. See [Tools and weapons](tools).)

## every

`every` repeats code on a timer, for as long as the game runs:

```rovik run in=part:Coin
every 0.03 seconds
    self.rotation.y += 5
end
```

```rovik run
every 10 seconds
    print("Ten more seconds have passed")
end
```

It's the right way to do anything that keeps going: spinning, blinking, a clock, spawning an enemy every so often.

## wait

`wait(seconds)` pauses **this script** for a while, then carries on. The rest of the game keeps going:

```rovik run in=part:Lamp
-- Blink between yellow and dark, forever.
while true do
    self.color = {r = 255, g = 220, b = 60}
    wait(0.5)
    self.color = {r = 40, g = 40, b = 40}
    wait(0.5)
end
```

A `while true` loop with a `wait` inside is fine: the `wait` lets the rest of the game run. Without the `wait`, it would freeze the game, and Rovik stops it with an error.

A `wait` inside an event handler pauses just that one handler. So this door stays open for 3 seconds after someone touches its button, and other scripts carry on meanwhile:

```rovik run in=part:Button with=part:Door
on touched(other)
    if other.class == "player" then
        door = find("Door")
        door.transparency = 0.8
        door.can_collide = false
        wait(3)
        door.transparency = 0
        door.can_collide = true
    end
end
```

## time

`time()` is how many seconds the game has been running. It's how you measure how long something took, or make a cooldown:

```rovik run in=part:Pad
last_used = -10
on touched(other)
    if other.class == "player" and time() - last_used > 5 then
        last_used = time()
        print("Boost! (ready again in 5 seconds)")
        other.walk_speed = 30
    end
end
```

Next: [Reading errors](errors).
