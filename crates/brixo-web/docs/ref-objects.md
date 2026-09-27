Every kind of object, and everything scripts can read and change on it. **Vectors** are `{x, y, z}` and **colors** are `{r, g, b}` (0 to 255); you can set a whole one (`part.position = {x = 1, y = 2, z = 3}`), just one part of it (`part.position.y += 1`), or copy another (`part.color = other.color`).

## Every object

| Property | | |
|---|---|---|
| `name` | read/set | Its name. |
| `class` | read | What kind of object: `"part"`, `"spawnlocation"`, `"model"`, `"folder"`, `"script"`, `"tool"`, `"textlabel"`, `"textbutton"`, `"frame"`, `"sound"`, `"player"`, `"workspace"`. |
| `parent` | read/set | What it's inside. Setting it moves it. |
| `children` | read | A list of what's inside it. |
| *anything else* | read/set | A **custom field** of your own: a number, text or `true`/`false`. `nil` until set; setting `nil` removes it. |

## Part (and SpawnLocation)

| Property | | |
|---|---|---|
| `position` | vector | Where its center is. |
| `size` | vector | How big it is (at least 0.05). |
| `rotation` | vector, degrees | How it's turned. |
| `color` | color | |
| `material` | text | `"plastic"`, `"wood"`, `"brick"`, `"metal"`, `"grass"`, `"concrete"`, `"neon"` |
| `shape` | text | `"block"`, `"wedge"`, `"cylinder"`, `"ball"` |
| `transparency` | 0 to 1 | 0 solid, 1 invisible. |
| `anchored` | true/false | Stays put (true) or falls and gets pushed around (false). |
| `can_collide` | true/false | Solid (true) or pass-through (false). Pass-through parts still fire `touched`. |
| `velocity` | vector, studs/s | Its speed. On an anchored part: a conveyor, carrying what's on it. |
| `floating` | true/false | A loose part with no gravity. |
| `bounce` | 0 to 1 | Bounciness. |
| `hinge` | text | What it turns around: `"off"`, `"y"` (its height), `"x"` (its width), `"z"` (its depth). Setting one loosens the part. See [Hinges](howto-hinges). |
| `hinge_at` | text | Where the hinge is: `"middle"`, `"left"`, `"right"`, `"top"`, `"bottom"`, `"front"`, `"back"`. |
| `motor_speed` | degrees/s | Keeps it turning. 0 swings freely. |
| `swing_to` | degrees or `nil` | Turns to this angle and holds it. `nil` swings freely. |
| `hinge_angle` | degrees (read) | How far it's turned from where it started. |

A **SpawnLocation** is a part players appear on. Give it a `team` custom field and players on that team respawn on it.

## Player

| Property | | |
|---|---|---|
| `name` | read | Their username. |
| `position` | vector | Where their middle is. Setting it teleports them. |
| `velocity` | vector (set) | Setting it launches them. |
| `health` | number | 0 to `max_health`. At 0 they're knocked out. |
| `max_health` | number | 100 to start. |
| `walk_speed` | number | 16 to start. |
| `jump_power` | number | 39 to start. |
| `face` | text | `"smile"`, `"happy"`, `"surprised"`, `"determined"` |
| `skin_color`, `shirt_color`, `pants_color` | color | Their avatar's colors. |
| `camera_mode` | text | `"default"`, `"first_person"`, `"third_person"` |
| `look` | vector (read) | Which way they face, flat and 1 long. |
| `mouse` | vector (read) | Where they last clicked with a tool. |
| `equipped` | object (read) | The tool in their hand, or `nil`. |
| `swinging` | true/false (read) | Just used a tool. |
| `children` | list (read) | Their tools, and their own GUI. |
| `kart` | kart or `nil` | The kart they're driving. Set it to put them in one (a kart or any part of it), `nil` to get out. |
| `bot` | true/false (read) | A computer player, made by `add_bot`. |
| `lane` | number | A bot's distance to the left of the racing line (negative: right). |

`camera` is also a variable in Studio's single-player Play: `camera.mode` is the same as your own `camera_mode`.

## TextLabel, TextButton, Frame

| Property | | |
|---|---|---|
| `text` | text | What it says. Numbers are fine too. (Frames can have text as well.) |
| `text_size` | number | In pixels, 4 to 200. 20 to start. |
| `text_color` | color | |
| `background` | true/false | Whether a box is drawn behind it. |
| `background_color` | color | |
| `x`, `y` | 0 to 1 | Its top-left corner, as a fraction of the screen. |
| `width`, `height` | 0 to 1 | Its size, as a fraction of the screen. |
| `visible` | true/false | `false` hides it and everything in it. |
| `attached_to` | part or `nil` | Floats over that part in the world. `x`, `y` then nudge it. |

In the Workspace everyone sees it; inside a player, only they do. A TextButton's script hears `on clicked(p)`.

## Kart

A **Model** with `kart = true`. Its `driver` (read) is who's driving it. Brixo keeps `speed`, `drift`, `boosting` and `spinning` up to date on it; set `top_speed` or `locked = true` to change how it drives. See [Karts](howto-karts).

## Tool

A tool's first part is the handle. It has the usual `name`, `parent` and `children`. With a custom field `grip = "up"` it's held straight up (a sword). Scripts inside it hear `on activated(p)`. Inside a player, it's in their hotbar.

## Sound

| Property | | |
|---|---|---|
| `volume` | 0 to 1 | How loud. |

Made by dropping an audio file on Studio. Play it with `play_sound(sound)` or `play_music(sound)`.

## Model, Folder, Script, Workspace

These have just the properties every object has, except the **Workspace**, which also holds the [lighting](lighting): `time_of_day`, `brightness`, `fog_start`, `fog_end`, `fog_color` and `sky_color`, and `racing_line`, the name of the folder bots drive along (see [Bots](howto-karts#bots)). The Workspace is the top of everything: `find("Workspace")`. A script's `self` is the object it's inside.

## Custom fields Brixo understands

| Field | On | What it does |
|---|---|---|
| `team` | player, SpawnLocation | Players respawn on the SpawnLocation of their team. Brixo's weapons don't hurt teammates. |
| `breakable = true` | anchored part | Explosions knock it loose; bricks left unconnected to the ground fall. |
| `carried_by = "Name"` | part | Follows that player, placed by `carry_x`, `carry_y`, `carry_z`. |
| `beacon = true` | part | A marker everyone sees through walls, with `beacon_text`. |
| `grip = "up"` | tool | Held straight up, chops forward. |
