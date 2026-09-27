Platforms that slide back and forth, go up and down, or spin: the heart of any obby. Players standing on them ride along.

## Back and forth

Put this in a platform. It glides between where you built it and a point 20 studs along x, and back, smoothly:

```rovik run in=part:Platform
start = {x = self.position.x, y = self.position.y, z = self.position.z}
distance = 20     -- how far it goes
seconds = 4       -- how long one trip there and back takes

every 0.03 seconds
    -- A smooth wave between 0 and 1, slowing at each end.
    t = (1 - cos(time() / seconds * 6.2832)) / 2
    self.position.x = start.x + distance * t
end
```

`cos` makes a smooth wave that slows down and turns around at each end. Change `self.position.x` to `.y` for an elevator, or `.z` to move the other way. `6.2832` is one full circle (2π), so the platform makes one trip there and back every `seconds`.

## An elevator that waits at each floor

```rovik run in=part:Elevator
bottom = self.position.y
top = bottom + 24

fn move_to(y)
    while abs(self.position.y - y) > 0.3 do
        if self.position.y < y then
            self.position.y += 0.3
        else
            self.position.y -= 0.3
        end
        wait(0.03)
    end
    self.position.y = y
end

while true do
    move_to(top)
    wait(2)
    move_to(bottom)
    wait(2)
end
```

## A spinning platform

```rovik run in=part:Spinner
every 0.03 seconds
    self.rotation.y += 2
end
```

A long thin spinning bar makes a "sweeper" that knocks players off: make it tall enough to hit them, and players have to jump over it each time it comes round.

## A platform that falls

It shakes when you land on it, then drops. It comes back after a few seconds:

```rovik run in=part:Crumbling_Platform
home = {x = self.position.x, y = self.position.y, z = self.position.z}
falling = false

on touched(other)
    if other.class != "player" or falling then
        return
    end
    falling = true
    -- Wobble for a moment...
    for i in 1..8 do
        self.position.x = home.x + 0.15
        wait(0.05)
        self.position.x = home.x - 0.15
        wait(0.05)
    end
    -- ...then fall.
    self.position.x = home.x
    self.anchored = false
    wait(4)
    -- Back where it was.
    self.anchored = true
    self.rotation = {x = 0, y = 0, z = 0}
    self.position = home
    falling = false
end
```

> **Note:** A loose part that falls off the bottom of the world is removed. Keep the map's floor or lava under it, or it won't come back.

## Conveyor belts

For moving walkways, don't move the part at all: give an anchored part a `velocity` and it carries whatever's on top:

```rovik run in=part:Conveyor
self.velocity = {x = 0, y = 0, z = -14}
self.material = "metal"
```
