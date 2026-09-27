Everything in the Rovik language, on one page. For explanations and examples, see [Scripting with Rovik](rovik-basics).

## Comments

```rovik sketch
-- to the end of the line
*** everything between the stars,
    over several lines ***
```

## Values

| Kind | Examples | `type(x)` |
|---|---|---|
| number | `5`, `-2`, `3.75` | `"number"` |
| text | `"Hi"`, `"line\nbreak"`, `"say \"hi\""` | `"text"` |
| boolean | `true`, `false` | `"boolean"` |
| nil | `nil` | `"nil"` |
| list | `[1, 2, 3]`, `[]` | `"list"` |
| map | `{name = "Sam", hp = 10}`, `{}` | `"map"` |
| function | `print`, `fn (x) return x * 2 end` | `"function"` |
| object | `self`, `find("Door")` | its class: `"part"`, `"player"`, `"tool"`... |

Text escapes: `\n` new line, `\t` tab, `\"` quote, `\\` backslash.

Only `false` and `nil` count as false. Everything else (including `0` and `""`) counts as true.

## Variables

```rovik sketch
score = 0        -- the first = creates it
score += 5       -- also -=  *=  /=
```

Names: letters, digits and `_`, not starting with a digit. Case matters. A variable created inside a function stays inside it; one created at the top of a script is seen by everything in that script. `if` and loops don't make their own scope.

## Operators

| | |
|---|---|
| `+ - * /` | maths. `/` always divides exactly: `7 / 2` is `3.5` |
| `%` | remainder: `7 % 3` is `1` |
| `+` with text | joins: `"Hi " + name`, `"Coins: " + 5` |
| `== !=` | same, not the same (lists and maps compare their contents) |
| `< > <= >=` | compare numbers (and text, alphabetically) |
| `and or not` | logic. `a or b` gives `a` if it's true, otherwise `b`: `x or "default"` |
| `-x` | minus |

## Lists and maps

```rovik sketch
l = ["a", "b"]
l[1]              -- "a" (lists start at 1)
l[2] = "c"
len(l)            -- 2

m = {hp = 10}
m.hp              -- 10
m["hp"]           -- 10, when the name is in a variable
m.new = 1         -- add an entry
m.missing         -- nil
```

Lists and maps are shared, not copied, when you give them another name.

## Decisions and loops

```rovik sketch
if a then
    ...
elseif b then
    ...
else
    ...
end

while cond do ... end
for i in 1..10 do ... end         -- both ends included
for item in list do ... end
for key in map do ... end         -- names, alphabetically
for letter in "text" do ... end
break                             -- leave the loop
continue                          -- next time round
```

## Functions

```rovik sketch
fn name(a, b)
    return a + b
end

f = fn (x)
    return x * 2
end
```

`return` without a value (or no `return`) gives `nil`. At the top level of a script, `return` stops the script.

## Events and timers

```rovik sketch
on touched(other) ... end
on player_joined(p) ... end
every 2 seconds ... end
wait(0.5)
```

See [Events](ref-events).

## Words you can't use as names

`fn end if then elseif else while do for in return break continue and or not true false nil on every`
