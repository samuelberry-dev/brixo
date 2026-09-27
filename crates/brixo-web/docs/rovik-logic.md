Scripts get interesting when they make **decisions** ("if the player has enough coins...") and **repeat** things ("for every player...").

## if

```rovik run
coins = 25
if coins >= 20 then
    print("You can buy the sword!")
end
```

The code between `then` and `end` runs only if the **condition** is true. Add `else` for what to do otherwise, and `elseif` for more choices:

```rovik run
health = 45
if health > 70 then
    print("Feeling great")
elseif health > 30 then
    print("A bit hurt")
else
    print("Find a medkit!")
end
```

Short ones can go on one line: `if coins < 0 then coins = 0 end`.

## Comparing

| Write | Means |
|---|---|
| `a == b` | a is the same as b |
| `a != b` | a is not the same as b |
| `a < b`, `a > b` | less than, more than |
| `a <= b`, `a >= b` | less than or equal, more than or equal |

> **Watch out:** `==` (two equals signs) *compares*. `=` (one) *sets* a variable. `if x = 5` is a mistake, and Rovik will tell you so.

Text compares too: `other.name == "Ann"`, `self.material == "neon"`.

## and, or, not

Combine conditions with `and` (both must be true), `or` (at least one), and `not` (the opposite):

```rovik run
coins = 30
level = 2
has_key = false
if coins >= 20 and level >= 2 then
    print("Welcome to the VIP room")
end
if not has_key then
    print("The door is locked")
end
```

What counts as true? Everything except `false` and `nil`. So `if p.team then` means "if the player has a team". A handy trick: `x or "default"` gives `x`, or `"default"` if `x` is `nil`.

## Loops

### for ... in a range

Count from one number to another (both ends included):

```rovik run
for i in 1..5 do
    print("Countdown:", 6 - i)
end
print("Go!")
```

### for ... in a list

Go through every item in a list, like every player in the game:

```rovik run
for p in players() do
    print(p.name, "has", p.health, "health")
end
```

The same works for every child of an object (`for part in find("Walls").children do`), every key in a map, and every letter in some text.

### while

Keep going as long as something is true:

```rovik run
lives = 3
while lives > 0 do
    print("Lives left:", lives)
    lives -= 1
end
```

### break and continue

`break` stops a loop early. `continue` skips to the next time round:

```rovik run
for i in 1..10 do
    if i % 2 == 0 then
        continue   -- skip even numbers
    end
    if i > 7 then
        break      -- stop after 7
    end
    print(i)
end
```

> **Note:** A loop that never ends freezes a game, so Rovik stops any loop that runs for too long without a `wait`. For something that should keep happening forever (a spinning coin, a round timer), use `every`, or put a `wait` inside the loop. See [Events, waiting and time](events).

Next: [Lists and maps](rovik-lists-maps).
