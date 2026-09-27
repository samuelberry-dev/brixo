Games get big. **Folders** and **Models** keep them organized, and let scripts work with many parts at once. **Templates** let you build something once and copy it as often as you like.

## Folders

A **Folder** is a box for keeping things tidy. It does nothing on its own, but it's how scripts find a whole group:

```rovik run with=folder:Lights
-- Make three lights, keep them in the Lights folder, then turn them all on.
lights = find("Lights")
for i in 1..3 do
    lamp = create("Part", lights)
    lamp.position = {x = i * 6, y = 8, z = 0}
end
for lamp in lights.children do
    lamp.material = "neon"
    lamp.color = {r = 255, g = 230, b = 150}
end
```

**Add Folder** in Studio makes one inside whatever's selected. Drag things onto it in the Explorer to put them in.

## Models

A **Model** is a group of parts **glued together**. If its parts are loose, they fall and tumble as **one piece** instead of coming apart: a car, a crate with a lid, a tree.

In Studio, select several parts (Ctrl-click) and press **Ctrl+G** (Edit → Group into Model). **Ctrl+U** ungroups. Clicking any part of a Model selects the whole Model; **Alt-click** picks one part inside it.

To move a whole model from a script, move each of its parts by the same amount:

```rovik run with=model:Car
car = find("Car")
body = create("Part", car)
body.size = {x = 4, y = 2, z = 8}
roof = create("Part", car)
roof.size = {x = 4, y = 1, z = 4}
roof.position.y = 1.5

-- Lift the whole car 10 studs.
for piece in car.children do
    piece.position.y += 10
end
```

## Finding things

`find("Name")` searches the whole game and gives you the **first** object with exactly that name, or `nil` if there isn't one. Names are case-sensitive: `"door"` won't find `Door`.

```rovik run with=part:Door
door = find("Door")
if door == nil then
    print("There's nothing called Door!")
else
    print("Found it at", door.position)
end
```

> **Tip:** Give things **unique names** when scripts need to find them. With two parts called `Door`, `find` gives you whichever comes first, which might not be the one you meant. Or keep them in a folder and go through its `children`.

From any object you can move around the tree:

| | |
|---|---|
| `obj.parent` | what it's inside |
| `obj.children` | a list of what's inside it |
| `obj.name` | its name (you can change it) |
| `obj.class` | what kind of object: `"part"`, `"model"`, `"folder"`, `"player"`, `"tool"`, `"script"`, `"textlabel"`... |

Move something by changing its parent: `coin.parent = find("Collected")`. That's also how you give a player a tool: see [Tools and weapons](tools).

## Making, copying and removing

| | |
|---|---|
| `create("Part", parent)` | a new object inside `parent`. You can create `"Part"`, `"Model"`, `"Folder"`, `"TextLabel"`, `"TextButton"`, `"Frame"`, `"Tool"` and `"SpawnLocation"`. |
| `clone(obj)` | a copy of `obj` and everything inside it, **scripts included**, in the same place as the original (set its `parent` to move it). |
| `destroy(obj)` | removes it and everything inside it, for good. |

## Templates: build once, copy many times

The best way to make lots of something (coins, enemies, bullets, prizes) is to **build one by hand**, put it somewhere out of the way, and **clone** it whenever you need another. Its scripts come with every copy, so each copy works on its own.

1. Build the thing (say, a spinning coin with a script that gives a coin when touched).
2. Name it **Coin Template**, and make a **Folder** called **Storage** to keep it in.
3. Move the Storage folder somewhere players won't see it, like far below the map: it's fine, anchored parts never fall.
4. Clone it from a script:

```rovik run with=part:Coin_Template
-- Every 3 seconds, a coin appears at a random spot.
every 3 seconds
    coin = clone(find("Coin Template"))
    coin.name = "Coin"
    coin.position = {x = random(-40, 40), y = 3, z = random(-40, 40)}
    coin.parent = find("Workspace")   -- out of Storage, into the game
end
```

A copy starts out **in the same place as the original**, inside Storage, so move it out by setting its `parent`, as the last line does.

> **Important:** Scripts run **wherever they are**, including inside the template in Storage. If the template's script does something by itself when it starts (like destroying itself after 10 seconds), it'll destroy the template too. Start those scripts with a check that stops them in Storage: `return` at the top of a script ends it there.

```rovik run in=part:Coin_Template with=folder:Storage
-- The coin's own script. The template sits in Storage and must not vanish.
if self.parent.name == "Storage" then
    return
end
wait(10)
destroy(self)   -- copies disappear after 10 seconds if nobody grabs them
```

Handlers (`on touched` and friends) are safe in templates: nobody touches the template, so they never fire there.

Next: [Spawns and teams](spawns-teams).
