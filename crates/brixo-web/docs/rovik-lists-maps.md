## Lists

A **list** holds several values in order, between square brackets:

```rovik run
colors = ["red", "green", "blue"]
print(colors[1])      -- red: lists start at 1
print(len(colors))    -- 3
colors[2] = "yellow"
print(colors)         -- ["red", "yellow", "blue"]
```

> **Note:** The first item is `[1]`, not `[0]` like in some languages. `colors[0]` is an error, and Rovik says so.

Adding and removing:

```rovik run
queue = ["Ann"]
push(queue, "Bob")         -- add to the end
insert(queue, 1, "Cleo")   -- add at position 1
print(queue)               -- ["Cleo", "Ann", "Bob"]
last = pop(queue)          -- take the last one off
first = remove(queue, 1)   -- take out position 1
print(last, first, queue)  -- Bob Cleo ["Ann"]
```

Go through a list with `for`:

```rovik run
prizes = [10, 25, 50]
total = 0
for p in prizes do
    total += p
end
print("Total prizes:", total)
```

Pick a random item: `colors[random(1, len(colors))]`.

Lists from the game work the same way: `players()` is a list of everyone playing, and `obj.children` is a list of what's inside an object.

## Maps

A **map** keeps values under names, like a little form:

```rovik run
weapon = {name = "Laser", damage = 30, ammo = 12}
print(weapon.name, weapon.damage)
weapon.ammo -= 1
weapon.color = "red"       -- add a new entry
print(weapon.ammo, weapon.color)
print(weapon.missing)      -- nil: there's nothing called that
```

Two ways to reach an entry: `weapon.damage` when you know the name, and `weapon["damage"]` when the name is in a variable:

```rovik run
scores = {Red = 0, Blue = 0}
team = "Red"
scores[team] += 1
print(scores.Red)   -- 1
for t in keys(scores) do
    print(t, scores[t])
end
```

`keys(map)` gives a list of its names (in alphabetical order), and `for k in map do` goes through them too. `remove(map, "name")` deletes an entry.

## Positions and colors are maps

Brixo uses small maps for positions, sizes and colors. That's why you'll see these everywhere:

```rovik run in=part:Box
self.position = {x = 0, y = 10, z = 0}
self.size = {x = 4, y = 1, z = 4}
self.color = {r = 255, g = 200, b = 0}
```

You can also change just one part of them: `self.position.y += 2` lifts a part by 2, and `self.color.r = 255` turns up the red. See [Parts](parts).

## Shared, not copied

Giving a list or map a second name doesn't copy it: both names point at the **same** list.

```rovik run
a = [1, 2]
b = a
push(b, 3)
print(a)   -- [1, 2, 3]: a changed too
```

That's usually what you want (a function can add to a list you give it). To compare, `==` checks whether two lists or maps hold the same things.

Next: [Functions](rovik-functions).
