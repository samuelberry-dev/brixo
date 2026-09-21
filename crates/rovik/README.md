# Rovik

The scripting language for Brixo. Beginner-first, and it grows with you.

Run a script:

    cargo run -p rovik -- path/to/script.rvk

## Cheat sheet

    -- a line comment
    *** a block comment,
        across lines ***

    score = 10                  -- variables are made by assigning
    score += 5                  -- also -=, *=, /=
    name = "Sam"
    print("Hi " + name)         -- + joins text; numbers become text

    if score >= 10 and not done then
        print("high")
    elseif score > 0 then
        print("low")
    else
        print("none")
    end

    for i in 1..5 do print(i) end      -- ranges include both ends
    for item in ["a", "b"] do print(item) end
    while score > 0 do score -= 1 end  -- break / continue work in loops

    fn add(a, b)
        return a + b
    end
    double = fn (x) return x * 2 end   -- functions are values

    items = [10, 20, 30]        -- lists start at 1: items[1] is 10
    player = {name = "Sam", coins = 0}
    player.coins += 5           -- a missing field reads as nil

    on touched(player) ... end  -- events (run inside Brixo)
    every 5 seconds ... end     -- timers (run inside Brixo)

## Scope

At the top of a script, assigning creates a script-level variable.
Inside a function, assigning updates a variable that already exists
outside; otherwise it creates one local to the function. Reading a
name that was never assigned is an error, which catches typos.

## Built-in functions

print, len, str, num, type, push, pop, insert, remove, keys, wait,
floor, round, abs, min, max, sqrt, random

## Inside Brixo

A script sits inside a part. `self` is that part.

    self.name = "Coin"
    self.position.y += 1            -- live: this really moves the part
    self.size = {x = 4}             -- missing axes stay the same
    self.color = {r = 255, g = 200, b = 0}
    print(self.position)            -- (0, 1, 0)

    other = find("Door")            -- first thing with that name, or nil
    copy = clone(other)             -- copies it (and its scripts)
    destroy(copy)
    print(time())                   -- seconds since Play

    wait(2)                         -- pauses only this script

    on touched(other)               -- another part started overlapping
        print(other.name)
    end

    every 1 seconds                 -- won't overlap its own last run
        self.rotation.y += 15
    end

Parts have name, position, size, rotation, color, parent and children.
