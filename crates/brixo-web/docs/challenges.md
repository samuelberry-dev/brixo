**Brix** are what players spend in the Catalog on the website: faces, shirts, hats, capes. Nobody can buy Brix. They get a few for visiting each day, and the rest by completing **challenges** in games. So a good challenge is a reason to come back to your game.

## Completing a challenge

One function does it:

```rovik run
on player_joined(p)
    p.stage = 1
end

-- Put this inside the last checkpoint pad of an obby.
on touched(other)
    if other.class == "player" then
        complete_challenge(other, "finish_obby")
    end
end
```

`complete_challenge(player, "finish_obby")`:

- counts **once** for each player (a daily challenge counts once a day),
- tells everyone in the chat: **Sam completed Finish the Obby! (+25 Brix)**,
- gives back how many Brix it paid, in case you want to show it.

Calling it again for someone who's already done it does nothing, so it's fine to call it every time they reach the end.

## Names and what they pay

The name is yours to pick: letters, numbers and `_`, like `"win_round"` or `"five_knockouts"`. Players never type it; they see a title.

The first time your game uses a name, the website adds the challenge **waiting for an admin**. A waiting challenge doesn't pay anything yet (and doesn't show in the chat). An admin gives it a title players see, the Brix it pays, and whether it's daily, and then it shows on your game's page:

| | |
|---|---|
| Title | "Finish the Obby" |
| Brix | Up to 500 |
| Daily | Counts again every day, or only once ever |

This is so nobody can make a "free Brix" button. Every player can earn at most **200 Brix a day** from challenges, across all games.

## Good challenges

- **Finishing** something: an obby, a race, a tycoon.
- **Winning**: a round, a match. Give it to the whole winning team with a loop over `players()`.
- **Getting good**: five knockouts in a round, a lap under a minute.

Keep them fair: something a player does, not something that happens to them. Check it in the script (the time, the score) before calling `complete_challenge`.

```rovik run
-- The winning team, at the end of a round.
fn round_won(team)
    for p in players() do
        if p.team == team then
            complete_challenge(p, "win_round")
        end
    end
end
```

## Testing in Studio

In Studio (and in games not on the website), `complete_challenge` works the same way (it says so in the chat, once per player) but pays 0 Brix. Brix only come from games on the website.

The sample games all have challenges: see [the samples](samples).
