**Rovik** is the language Brixo games are scripted in. It's made to be easy to read: there are no semicolons or curly brackets, and blocks end with the word `end`. If you've seen Lua or Python, it'll look familiar. If you haven't, start here.

## Where scripts go

A script is an object like any other. Put it **inside the thing it's about**: a script inside a door controls the door, a script inside a coin controls the coin. Inside the script, **`self` is the object it's inside**:

```rovik run in=part:Door
-- Inside a part called "Door": make it see-through and ghostly.
self.transparency = 0.5
self.can_collide = false
print("The door is now a ghost")
```

> **Note:** `self` is the thing the script is in, **not** the script itself. This is the most common mix-up in Brixo.

A script that isn't about one object (a game's rules, a round timer) can go straight in the Workspace or in a Folder. Its `self` is the Workspace.

Every script starts running when the game starts. It runs from the top down, then stays alive to answer any **events** it set up (like `on touched`, in [Events](events)).

## print

`print` writes to **Output**. It's how you check what a script is doing:

```rovik run
print("Hello from my script!")
print("Two plus two is", 2 + 2)
```

Several values in one `print` are shown with spaces between them.

## Values

Rovik has a few kinds of value:

| Kind | Looks like | Used for |
|---|---|---|
| number | `5`, `-2`, `3.75` | amounts, positions, time |
| text | `"Hello"` | names, messages |
| boolean | `true`, `false` | yes-or-no things: is it anchored? |
| nil | `nil` | nothing at all: "no value" |
| list | `[1, 2, 3]` | several things in order |
| map | `{name = "Sam", coins = 10}` | named values, like a form |
| object | `self`, `find("Door")` | a thing in your game: a part, a player... |

Lists and maps have their own page: [Lists and maps](rovik-lists-maps).

## Variables

A **variable** is a name for a value. Make one by giving it a value with `=`:

```rovik run
coins = 0
player_name = "Ann"
coins = coins + 5
print(player_name, "has", coins, "coins")
```

There's no special word to make a variable: the first `=` creates it. Names can use letters, numbers and `_`, but can't start with a number. Capital letters matter: `Coins` and `coins` are different.

Shortcuts for changing a number:

```rovik run
score = 10
score += 5    -- same as score = score + 5
score -= 2
score *= 3
score /= 13
print(score)  -- 3
```

## Maths

`+ - * /` work as you'd expect, and `%` is the remainder after dividing (handy for "every third one"). Brackets go first:

```rovik run
print(2 + 3 * 4)     -- 14
print((2 + 3) * 4)   -- 20
print(10 / 4)        -- 2.5
print(10 % 3)        -- 1
```

Some maths helpers:

```rovik run
print(floor(3.7), round(3.5), abs(-4))      -- 3 4 4
print(min(4, 9, 2), max(4, 9, 2))           -- 2 9
print(sqrt(16))                             -- 4
print(random(1, 6))                         -- a dice roll: 1 to 6
print(random())                             -- a decimal from 0 up to 1
```

`floor` rounds down, `round` rounds to the nearest whole number. To round to one decimal place: `round(x * 10) / 10`. For angles there's `sin`, `cos` and `atan2`; see [Functions](ref-functions).

## Text

Text goes in double quotes. `+` joins text together, and joins numbers onto text too:

```rovik run
name = "Ann"
coins = 12
print("Hi " + name + "! You have " + coins + " coins.")
print(len("Brixo"))   -- 5: how many letters
```

Inside quotes, `\"` is a quote mark and `\n` starts a new line.

`str(x)` turns anything into text, and `num(t)` turns text into a number (or `nil` if it isn't one): `num("42") + 1` is `43`. That's useful for a label that shows a number: its `text` is text, so `num(label.text)` gets the number back.

## Comments

`--` starts a comment: everything after it on that line is a note for people, and Rovik skips it. For longer notes, put them between `***` and `***`:

```rovik run
-- Turns the lights on at the start.
lights_on = true

***
This part isn't finished yet.
Everything between the stars is skipped.
***
```

Next: [Decisions and loops](rovik-logic).
