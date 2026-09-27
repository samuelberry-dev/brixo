## Sound effects

`play_sound` plays a sound. Brixo has 16 built in:

```rovik run
play_sound("coin")
```

| | | | |
|---|---|---|---|
| `coin` | `cash` | `buy` | `click` |
| `error` | `pop` | `jump` | `hit` |
| `win` | `whoosh` | `death` | `boom` |
| `twang` | `bonk` | `splat` | `thud` |

By default **everyone** hears it. Add a player to play it **just for them**:

```rovik run in=part:Coin
on touched(other)
    if other.class == "player" then
        play_sound("coin", other)   -- only the one who got it hears it
        destroy(self)
    end
end
```

## Music

`play_music` starts a track that **loops** until something else plays. Starting a new track crossfades from the old one. Brixo has two built in: `"sunny"` (bright and bouncy) and `"rush"` (fast, for action).

```rovik run
play_music("sunny")
wait(2)
play_music("rush")   -- fades over to the other track
wait(2)
stop_music()
```

Like sounds, music can be for one player: `play_music("rush", p)`, `stop_music(p)`.

## Your own sounds and music

**Drag an mp3, wav or ogg file onto the Studio window.** It becomes a **Sound** object in your game, named after the file (rename it to something easy). Then play it by passing the object instead of a name:

```rovik
play_sound(find("Victory Fanfare"))
play_music(find("Boss Theme"))
play_music(find("Boss Theme"), p)   -- for one player
```

A Sound has a `volume`, from 0 (silent) to 1 (full): `find("Boss Theme").volume = 0.5`.

The file is stored inside your game, so it goes with it when you publish. Players download each sound once, when they join.

> **Keep files small:** a game's whole upload can be up to 32 MB. A few minutes of music as an mp3 is usually 2 to 5 MB. Use mp3 or ogg for music: wav files are much bigger.
>
> And only use music and sounds **you're allowed to use**: ones you made, or ones that say you can use them. Songs from artists, games and shows belong to someone else.

## A soundtrack that changes

Swap the music to match what's happening: calm while waiting, fast when a round starts, a fanfare at the end.

```rovik run
fn start_round()
    play_music("rush")
end

fn end_round()
    stop_music()
    play_sound("win")
    wait(3)
    play_music("sunny")
end

play_music("sunny")
wait(1)
start_round()
wait(1)
end_round()
```

Next: [How-tos](howto-kill-brick), starting with kill bricks.
