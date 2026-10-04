A **vehicle** is something you build from parts: a body, wheels on [hinges](howto-hinges) with motors, and a **seat**. Walk into the seat and you sit down; your keys go to the seat, and a short script turns them into motor speeds. Cars, tanks, buggies, a rolling ball: they're all built the same way. The Toolbox's **Car** is a ready-made one to start from.

## Seats

Any part can be a seat: tick **Seat** in Properties (or **Add Part → Seat**, which makes a small dark one). A seat's front is its **+Z** side, the way the driver faces.

- **Getting in:** walk into the seat (or land on it). You sit down, facing its front.
- **Getting out:** press **Space**. You hop up and off.
- **While you sit**, your keys stop moving your character and go to the seat instead:

| Field on the seat | What it is |
|---|---|
| `throttle` | **W** is `1`, **S** is `-1`, nothing is `0`. |
| `steer` | **A** is `1` (left), **D** is `-1` (right), nothing is `0`. |
| `occupant` | Who's sitting in it (`nil` if nobody). Read only. |

`throttle` and `steer` are `nil` until someone has sat in it, and `0` again once they get up. The seat moves with whatever it's welded to, and the driver moves with the seat.

```rovik run in=part:Seat
self.seat = true
every 1 seconds
    if self.occupant != nil then
        print(self.occupant.name, "is pressing", self.throttle, self.steer)
    end
end
```

## A car

The Toolbox's **Car** is built like this. The back wheels drive; the front wheels steer, each on a small **steering knuckle**: a block on an upright hinge that swings left and right, with the wheel hinged to it.

1. Make the **body**: a loose part, Size `4, 1, 8`, its front facing +Z.
2. Add a **seat** on top of it, near the back, facing +Z.
3. Add two **back wheels**: *Cylinders*, Size `3, 1, 3`, turned `90` on z, against the sides of the body near the back. **Hinge** *Its height (Y)*. Name them `Rear Wheel`.
4. Add two **steering knuckles**: small blocks (Size `0.8, 0.8, 0.8`) against the sides of the body near the front. **Hinge** *Its height (Y)* (upright), **Swing to** `0`. Name them `Steering`.
5. Add two **front wheels** like the back ones, against the outside of each knuckle, **not touching the body**: a wheel hangs on whatever it touches nearest its middle, and it should be the knuckle. Name them `Front Wheel`.
6. Group everything into a **Model** (Ctrl+G) with the **body first**, so the seat is welded to it.
7. Put this script in the Model:

```rovik
-- Walk into the seat to drive: W/S go, A/D steer, Space gets out.
-- The seat's throttle and steer say what the driver presses: this turns
-- the back wheels' motors, and swings the front wheels' steering.
seat = nil
drive = []
steering = []
for c in self.children do
    if c.name == "Seat" then
        seat = c
    elseif c.name == "Rear Wheel" then
        push(drive, c)
    elseif c.name == "Steering" then
        push(steering, c)
    end
end
-- Degrees a second the wheels turn at full throttle. (Wheels turned the
-- other way round drive backwards: flip the sign.)
speed = -600
-- How far the front wheels turn, in degrees.
angle = 25
every 0.05 seconds
    go = seat.throttle
    turn = seat.steer
    if go == nil then
        go = 0
    end
    if turn == nil then
        turn = 0
    end
    for w in drive do
        w.motor_speed = go * speed
    end
    for s in steering do
        s.swing_to = turn * angle
    end
end
```

Facing the car's front (+Z), **left is +X**. `steer` is `1` for left, and a positive `swing_to` turns the knuckles left. Change `speed` and `angle` to taste, make it bigger, add a passenger seat that does nothing, or skip the knuckles and steer like a tank: run the left and right wheels at different speeds (it skids round, slowly).

## Seats from scripts

A player's `seat` is the seat they're in (`nil` if none). Set it to sit them down, or to `nil` to get them up:

```rovik run with=part:Seat
find("Seat").seat = true
on player_joined(p)
    wait(1)
    p.seat = find("Seat")
    wait(3)
    p.seat = nil
end
```

Only one player sits in a seat at a time; sitting a second player in it is an error. Someone in a seat isn't in a [kart](howto-karts), and the other way round.

## Things to know

- **Online, the vehicle is the server's.** Your keys go to the server and it moves the vehicle, so on a far-away server steering answers a moment late. (Walking doesn't: Brixo moves your own character on your screen straight away. Vehicles will get the same.)
- **Getting up** puts you a little above the seat. You can't sit straight back down for a second.
