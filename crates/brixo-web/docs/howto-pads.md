Pads that throw you into the air, speed you up, or fling you across the map.

## Jump pad

```rovik run in=part:Jump_Pad
self.color = {r = 60, g = 220, b = 255}
self.material = "neon"

on touched(other)
    if other.class == "player" then
        other.velocity = {x = 0, y = 80, z = 0}
        play_sound_at("jump", self)
    end
end
```

Setting a player's `velocity` launches them. `y = 80` is about three times a normal jump. Try 60 for a small boost and 120 for a huge one.

## Launch pad

Throw players forward too, in the direction the pad is facing. Turn the pad (its **Rotation** y) to aim it:

```rovik run in=part:Launch_Pad
power = 70

on touched(other)
    if other.class == "player" then
        -- Which way the pad faces: its rotation, as a direction.
        a = self.rotation.y / 57.2958
        other.velocity = {x = sin(a) * power, y = 60, z = cos(a) * power}
        play_sound_at("whoosh", self)
    end
end
```

`rotation.y` is in degrees, and `sin` and `cos` want radians, so it's divided by 57.2958 (the number of degrees in a radian). The sideways push wears off when they land.

## Speed pad

Faster running for a few seconds:

```rovik run in=part:Speed_Pad
self.color = {r = 255, g = 200, b = 0}
self.material = "neon"

on touched(other)
    if other.class == "player" and other.boosted != true then
        other.boosted = true
        other.walk_speed = 34
        play_sound("pop", other)
        wait(3)
        other.walk_speed = 16
        other.boosted = false
    end
end
```

`boosted` stops a second touch from starting another timer while the first boost is running.

## Low gravity zone

There's no gravity setting for players, but a big `jump_power` feels the same. Make a see-through zone (Can Collide off, Transparency 0.8) and:

```rovik run in=part:Moon_Zone
on touched(other)
    if other.class == "player" then
        other.jump_power = 80
    end
end
```

And give them their normal jump back somewhere else, like on a pad at the exit: `other.jump_power = 39`.
