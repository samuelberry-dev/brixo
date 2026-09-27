Step on one pad, appear on another.

## One-way

Make two pads, **Teleporter A** and **Teleporter B**. Put this in **A**:

```rovik run in=part:Teleporter_A with=part:Teleporter_B
on touched(other)
    if other.class == "player" then
        target = find("Teleporter B")
        other.position = {x = target.position.x, y = target.position.y + 4, z = target.position.z}
        play_sound("whoosh", other)
    end
end
```

`+ 4` puts them just above the pad, standing on it, rather than inside it.

## Two-way, without bouncing

With a script in both pads, landing on B sends you straight back to A. Stop that with a short cooldown per player. Give each pad a custom field `destination` with the other pad's name, and use the same script in both:

```rovik run in=part:Pad_A with=part:Pad_B
self.destination = "Pad B"
-- (In Pad B's copy of this script: self.destination = "Pad A")

on touched(other)
    if other.class != "player" then
        return
    end
    -- Just teleported? Ignore touches for a moment.
    if other.teleported_at != nil and time() - other.teleported_at < 2 then
        return
    end
    target = find(self.destination)
    other.teleported_at = time()
    other.position = {x = target.position.x, y = target.position.y + 4, z = target.position.z}
    play_sound("whoosh", other)
end
```

The cooldown is stored **on the player** (`other.teleported_at`), so it works across both pads.

## Random destinations

Put your destinations in a folder called **Destinations** and pick one:

```rovik run in=part:Mystery_Pad with=folder:Destinations
spots = find("Destinations")
for i in 1..3 do
    spot = create("Part", spots)
    spot.position = {x = i * 20, y = 1, z = -30}
end

on touched(other)
    if other.class == "player" then
        list = spots.children
        target = list[random(1, len(list))]
        other.position = {x = target.position.x, y = target.position.y + 4, z = target.position.z}
    end
end
```

(The loop at the top makes three example spots. In your game, build them by hand in the folder instead.)
