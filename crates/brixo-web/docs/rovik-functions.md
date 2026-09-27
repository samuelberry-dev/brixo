A **function** is a named piece of code you can run whenever you like. You've been using Brixo's built-in ones already (`print`, `find`, `random`). Here's how to make your own.

## Making a function

```rovik run
fn greet(name)
    print("Welcome to the game, " + name + "!")
end

greet("Ann")
greet("Bob")
```

`fn` starts it, then the name, then the **parameters** in brackets: the values it's given each time. Calling `greet("Ann")` runs the code with `name` set to `"Ann"`.

## Giving back a value

`return` sends a value back to whoever called the function, and stops it there:

```rovik run
fn price_with_discount(price, percent)
    return price - price * percent / 100
end

print(price_with_discount(200, 25))   -- 150

fn biggest(a, b)
    if a > b then
        return a
    end
    return b
end
print(biggest(3, 8))   -- 8
```

A function without `return` gives back `nil`.

## Why functions help

When you find yourself writing the same few lines twice, make them a function. It's shorter, and when you change your mind you only change it in one place:

```rovik run
fn reward(p, amount)
    if p.coins == nil then
        p.coins = 0
    end
    p.coins += amount
    play_sound("coin", p)
end

on player_joined(p)
    reward(p, 10)   -- a welcome bonus
end
```

## Which variables a function can see

- A function can **read and change** variables the script made **outside** it (at the top of the script).
- A variable a function **creates** stays inside it: it's gone when the function ends.

```rovik run
score = 0

fn add_points(n)
    bonus = n * 2      -- only exists inside add_points
    score += bonus     -- changes the script's score
end

add_points(5)
print(score)           -- 10
```

`if` blocks and loops don't hide their variables: a variable made inside an `if` is still there after its `end`.

## Functions are values

A function can be kept in a variable, put in a list, or passed to another function, just like a number:

```rovik run
fn twice(f)
    f()
    f()
end

twice(fn ()
    print("Hi!")
end)
```

`fn () ... end` without a name makes a function on the spot. You won't need this often, but it's there.

Next: [Events, waiting and time](events).
