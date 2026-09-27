| Event | Where the script goes | When it runs | You get |
|---|---|---|---|
| `on touched(other)` | inside a **part** | something starts touching the part | the player or part that touched it |
| `on clicked(p)` | inside a **TextButton** | a player clicks the button | the player who clicked |
| `on activated(p)` | inside a **Tool** | the player holding it clicks | the player holding it |
| `on key(p, k)` | anywhere | a player presses one of the game's keys: `"e"`, `"q"`, `"f"`, `"r"`, `"g"`, `"z"`, `"x"`, `"c"`, `"v"`, `"b"` | the player, and the key |
| `on player_joined(p)` | anywhere | a player joins | the player |
| `on player_left(p)` | anywhere | a player leaves | the player |
| `on died(p)` | anywhere | a player is knocked out (health 0, or fell below y = -60) | the player |
| `on respawned(p)` | anywhere | a knocked-out player is back at their spawn, 4 seconds later | the player |
| `every N seconds` | anywhere | every N seconds, for as long as the game runs | |

## Details

- **`touched`** fires once when touching **starts**. It fires for parts that don't collide too (`can_collide = false`), which is how coins and trigger zones work. It can fire several times in the same instant (both feet landing), so use a flag if it must only happen once.
- **`clicked`** only reaches buttons the player can see: hidden buttons, and buttons inside *another* player, can't be clicked.
- **`activated`** only fires while the tool is held. When it fires, `p.mouse` is where they clicked, and the character has turned to face it.
- **`player_joined`** runs for every player, including the first one (you, when you press Play in Studio). By then the player is already standing on the first SpawnLocation.
- **`died`**: `p.health` is 0. The player's tools stay with them.
- **`respawned`**: the player has full health and is standing on their spawn (or their team's). Move them to send them somewhere else.
- **`key`** fires for every script with an `on key`, whichever key it was: check `k`. See [Karts](howto-karts#keys-for-your-game).
- **`every`**: the first run is after the first N seconds, not straight away.

Each handler runs on its own, so a `wait` inside one only pauses that one.
