Everyone's scripts go wrong, all the time. That's normal: the skill is fixing them quickly. Brixo helps a lot, because its errors say **what went wrong, where, and usually how to fix it**.

## Where errors show up

**While you type:** the script editor checks your code as you write it. A mistake gets its line shaded red and a message under the editor, before you even press Play.

**While the game runs:** errors appear in **Output**, in red, like this:

```text
[Lava/Script] line 3: a player doesn't have 'helth'. Did you mean 'health'?
```

In the square brackets is **which script**: the object it's in, then the script's name. Then the **line number**, and what happened. Click the script in the Explorer, go to that line, and fix it. When a script hits an error it stops, but the rest of the game keeps running.

In a published game, press **F9** in Brixo Player to see the same log.

## The common ones

### A missing end

Every `if`, `while`, `for`, `fn`, `on` and `every` needs an `end`. Forget one and Brixo points at the block that's missing it:

```text
line 2: this 'while' is missing its 'end'
```

Indenting the code inside each block (the script editor keeps it for you) makes a missing `end` easy to spot.

### = instead of ==

```rovik broken
if coins = 10 then
    print("Ten!")
end
```

```text
line 1: '=' gives a variable a value. To compare two things, use '=='
```

### A typo in a name

Rovik spots names that are nearly right:

```text
line 5: 'scroe' hasn't been given a value yet. Did you mean 'score'?
line 3: a part doesn't have 'colour'. Did you mean 'color'?
```

### Something that's nil

`nil` means "nothing". It turns up when you use something that was never set:

```text
line 4: can't use + with a nil and a number. One side is nil, so something may not have been set
```

The usual causes:

- A **custom field that was never set**: `p.coins += 1` before anything set `p.coins`. Set it first, when the player joins: `p.coins = 0`.
- **`find` didn't find anything**: `find("Door")` gives `nil` if nothing is called exactly `Door`. Check the name in the Explorer, including capital letters and spaces.

### Lists start at 1

```text
line 2: position 0 is outside the list, which has 1 item. Positions start at 1
```

### The object was destroyed

```text
line 6: this object was destroyed, so it can't be used any more
```

Something destroyed an object (a coin that was collected, a projectile that exploded) and a script tried to use it afterwards. Check whether it still exists, or stop using it after `destroy`.

### A loop that never ends

```text
line 1: this script ran for too long without pausing. Is there a loop that never ends? Add wait() inside long loops
```

A `while` loop that runs forever without a `wait` would freeze the game, so Rovik stops it. Add a `wait(...)` inside, or use `every`. See [Events, waiting and time](events).

## Finding problems with print

When there's no error but something's not happening, find out what the script is actually doing. Put `print`s in:

```rovik run in=part:Button
print("Button script started")
on touched(other)
    print("touched by", other.class, other.name)
    if other.class == "player" then
        print("it's a player!")
    end
end
```

If "Button script started" never shows, the script isn't where you think it is. If "touched by" never shows, nothing is touching the part. (Is **Can Collide** off, so things pass through? Is it big enough?) If it does show but "it's a player!" doesn't, your `if` is checking the wrong thing.

Next: start building worlds, with [Parts](parts).
