The most classic obstacle there is: touch it and you're out.

## A kill brick

Make a part, color it red (Neon looks great), and put this script inside:

```rovik run in=part:Kill_Brick
on touched(other)
    if other.class == "player" then
        other.health = 0
    end
end
```

To make lots of them, build one, then **Ctrl+D** to duplicate it as often as you like: the script comes with each copy.

## A brick that hurts, but doesn't kill

Take away some health instead, with a little cooldown so one touch counts once:

```rovik run in=part:Spikes
hurt_at = {}   -- when we last hurt each player, by name

on touched(other)
    if other.class == "player" then
        last = hurt_at[other.name]
        if last == nil or time() - last > 1 then
            hurt_at[other.name] = time()
            other.health -= 25
            play_sound("hit", other)
        end
    end
end
```

`hurt_at` is a map that remembers, for each player's name, when they last got hurt, so walking back and forth doesn't hit them ten times a second.

## Lava that burns while you stand in it

`on touched` fires when touching **starts**, so someone standing still in the lava is only hurt once. For damage **over time**, check who's standing on it with `every`:

```rovik run in=part:Lava
-- Is player p standing on (or in) this part?
fn on_me(p)
    dx = abs(p.position.x - self.position.x)
    dz = abs(p.position.z - self.position.z)
    feet = p.position.y - 2.5
    top = self.position.y + self.size.y / 2
    return dx < self.size.x / 2 + 1 and dz < self.size.z / 2 + 1 and feet < top + 1 and feet > top - 3
end

every 0.5 seconds
    for p in players() do
        if p.health > 0 and on_me(p) then
            p.health -= 10
        end
    end
end
```

## Rising lava

Make the lava a huge flat part at the bottom of your map, and raise it slowly. Players have to keep climbing:

```rovik run in=part:Lava
start_y = self.position.y

on touched(other)
    if other.class == "player" then
        other.health = 0
    end
end

every 1 seconds
    self.position.y += 0.5
end

-- Back down every 2 minutes, for a new round.
every 120 seconds
    self.position.y = start_y
end
```

## Kill bricks that come and go

A brick that switches on and off keeps players on their toes. Show when it's safe with its color:

```rovik run in=part:Blinking_Laser
on_now = true

on touched(other)
    if on_now and other.class == "player" then
        other.health = 0
    end
end

every 1.5 seconds
    on_now = not on_now
    if on_now then
        self.transparency = 0
        self.color = {r = 255, g = 40, b = 40}
    else
        self.transparency = 0.8
        self.color = {r = 80, g = 80, b = 80}
    end
end
```

> **Note:** Someone already standing on it when it switches on won't be knocked out (they didn't *start* touching). For that, use the "burns while you stand in it" check above.
