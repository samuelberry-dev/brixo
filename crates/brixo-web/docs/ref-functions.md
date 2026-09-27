Every function built into Brixo. For events (`on ...`) see [Events](ref-events).

## The game world

| Function | What it does |
|---|---|
| `find(name)` | The first object anywhere in the game called exactly `name`, or `nil`. |
| `create(class, parent)` | A new object inside `parent`. Classes: `"Part"`, `"Model"`, `"Folder"`, `"TextLabel"`, `"TextButton"`, `"Frame"`, `"Tool"`, `"SpawnLocation"`. |
| `clone(obj)` | A copy of `obj` and everything in it (scripts too), in the same place. |
| `destroy(obj)` | Removes `obj` and everything in it. Destroying something already gone is fine. |
| `players()` | A list of everyone playing. |
| `time()` | Seconds since the game started. |
| `explode(position, radius)` | An explosion: knocks out players within `radius` (0.5 to 100), throws loose parts, breaks `breakable` parts, and shows a fireball. Gives back a list of the players it knocked out. |
| `explode(position, radius, power)` | The same, throwing things with `power` (0 to 400; 70 if left out). |

## Sound

| Function | What it does |
|---|---|
| `play_sound(name)` | A built-in sound for everyone. Names: [Sounds](ref-names#sounds). |
| `play_sound(sound)` | A Sound object in your game: `play_sound(find("Horn"))`. |
| `play_sound(name, player)` | Just for that player. |
| `play_music(name or sound)` | Starts looping music for everyone, crossfading from what was playing. |
| `play_music(name or sound, player)` | Just for that player. |
| `stop_music()`, `stop_music(player)` | Fades the music out. |

## Saving

Kept for each player in each game, between visits. See [Saving player data](howto-saving).

| Function | What it does |
|---|---|
| `save(player, name, value)` | Keeps `value` (a number, text, true/false, or a list or map of them) for this player under `name`. `nil` forgets it. Up to 64 KB per player per game. |
| `load(player, name)` | What was saved under `name` for this player, or `nil`. |

## Waiting

| Function | What it does |
|---|---|
| `wait(seconds)` | Pauses this script (or this event handler) for that long. The game carries on. |

## Printing and types

| Function | What it does |
|---|---|
| `print(a, b, ...)` | Writes the values to Output, with spaces between them. |
| `str(x)` | `x` as text. |
| `num(text)` | The number in `text`, or `nil` if it isn't one. |
| `type(x)` | `"number"`, `"text"`, `"boolean"`, `"nil"`, `"list"`, `"map"`, `"function"`, or an object's class. |
| `len(x)` | How many items in a list or map, or letters in text. |

## Lists and maps

| Function | What it does |
|---|---|
| `push(list, x)` | Adds `x` to the end. |
| `pop(list)` | Takes the last item off, and gives it back. |
| `insert(list, i, x)` | Puts `x` at position `i`, moving the rest along. |
| `remove(list, i)` | Takes out the item at `i`, and gives it back. |
| `remove(map, name)` | Deletes that entry. |
| `keys(map)` | A list of the map's names, alphabetically. |

## Maths

| Function | What it does |
|---|---|
| `floor(x)` | Rounds down: `floor(3.9)` is `3`. |
| `round(x)` | Rounds to the nearest whole number: `round(2.5)` is `3`. |
| `abs(x)` | Without the minus: `abs(-4)` is `4`. |
| `min(a, b, ...)`, `max(a, b, ...)` | The smallest, the biggest. |
| `sqrt(x)` | Square root. |
| `random()` | A decimal from 0 up to (not including) 1. |
| `random(a, b)` | A whole number from `a` to `b`, both included. |
| `sin(a)`, `cos(a)` | For angles in **radians**. Degrees to radians: divide by 57.2958. |
| `asin(x)`, `acos(x)` | The angle (in radians) with that sine or cosine. |
| `atan2(y, x)` | The angle (in radians) of the direction (x, y). `atan2(dx, dz) * 57.2958` is the `rotation.y` that faces along `dx, dz`. |
