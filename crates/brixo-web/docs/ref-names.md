## Sounds

Built-in sounds for `play_sound`:

| | | | |
|---|---|---|---|
| `coin` | `cash` | `buy` | `click` |
| `error` | `pop` | `jump` | `hit` |
| `win` | `whoosh` | `death` | `boom` |
| `twang` | `bonk` | `splat` | `thud` |

## Music

Built-in tracks for `play_music`: `sunny` (bright and bouncy), `rush` (fast, for action).

## Materials

`plastic`, `wood`, `brick`, `metal`, `grass`, `concrete`, `neon`

## Shapes

`block`, `wedge`, `cylinder`, `ball`

## Hinges

`hinge`: `"off"`, `"y"`, `"x"`, `"z"`. `hinge_at`: `"middle"`, `"left"`, `"right"`, `"top"`, `"bottom"`, `"front"`, `"back"`. See [Hinges](howto-hinges).

## Faces

`smile`, `happy`, `surprised`, `determined`, `wink`, `grin`, `silly`, `sleepy`, `angry`, `smirk`, `cat`, `heart_eyes`, `worried`, `laugh`

## Clothes

Shirts (`player.shirt`): `none`, `tee`, `tank`, `long_sleeve`, `striped`, `polo`, `hoodie`, `flannel`, `jacket`, `sweater`, `jersey`, `camo`

Pants (`player.pants`): `none`, `plain`, `jeans`, `shorts`, `cargo`, `track`, `plaid`

T-shirt pictures (`player.tshirt`, or `nil` for none): `brick`, `smiley`, `heart`, `star`, `flame`, `lightning`, `rocket`, `pizza`, `rainbow`, `ghost`, `number_one`

## Accessories

`player.hats` holds one for each place they go:

| Where | Names |
|---|---|
| Head | `cap`, `beanie`, `top_hat`, `cowboy_hat`, `crown`, `headphones`, `party_hat`, `chef_hat`, `viking_helmet`, `hard_hat`, `propeller_cap`, `halo`, `traffic_cone`, `wizard_hat`, `pirate_hat`, `bunny_ears`, `fedora` |
| Face | `sunglasses`, `nerd_glasses`, `eye_patch`, `mustache` |
| Neck | `scarf`, `bow_tie`, `gold_chain`, `necktie` |
| Back | `backpack`, `cape`, `angel_wings`, `jetpack` |

## Camera modes

`default`, `first_person`, `third_person`

## What scripts can create

`Part`, `Model`, `Folder`, `TextLabel`, `TextButton`, `Frame`, `Tool`, `SpawnLocation`

## Keys players use

| Keys | In a game |
|---|---|
| W A S D | Walk |
| Space | Jump (hold to jump higher) |
| 1 to 9 | Hold a tool (again to put it away) |
| Click | Use the tool in your hand, aimed where you click |
| Shift | Shift lock (also in Studio's Play): the mouse turns you and the camera, over your shoulder, and tools aim at the middle of the screen |
| Right-drag, or ← → | Turn the camera |
| Page Up / Page Down | Tilt the camera |
| Scroll, or I / O | Zoom (all the way in for first person) |
| / or Enter | Chat (or click the chat bar, top left) |
| Esc | The Game Menu: Reset Character, Settings (shift lock, camera speed, volume), Help (these keys) and Leave Game |
| F11 | Fullscreen (or the Fullscreen button, top left) |
| F9 | The game's log (prints and errors) |
| E Q F R G Z X C V B | Whatever the game's scripts make them do (`on key`) |

In a [kart](howto-karts): **W**/**S** accelerate and brake, **A**/**D** steer, **Space** drifts.

## Numbers worth knowing

| | |
|---|---|
| Player height | about 5 studs |
| `walk_speed` to start | 16 studs a second |
| `jump_power` to start | 39: a tap jumps about 7 studs, holding about 11 |
| Health to start | 100 |
| Respawn time | 4 seconds |
| Falling off the world | below y = -60 |
| Biggest explosion | radius 100 |
| Biggest game upload | 32 MB |
