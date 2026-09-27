Spend coins on upgrades and gear. This builds on [Coins and a leaderboard](howto-coins): players have `p.coins`, and a label showing it.

## Keep the coin label up to date

Every script is separate: a shop button can't call a function in the coin script. The easy way to keep a player's coin label right, whichever script changes their coins, is to **refresh it from `p.coins` every half second**. Add this to your Coins script in the Workspace:

```rovik run
every 0.5 seconds
    for p in players() do
        if p.coins != nil then
            for c in p.children do
                if c.name == "Coin Label" then
                    c.text = "Coins: " + p.coins
                end
            end
        end
    end
end
```

Now any script can just change `p.coins` and the label follows.

> **Tip:** Scripts share things through **objects**, never directly: custom fields on players and parts are the way to pass information from one script to another.

## Shop buttons

Each item is a **TextButton** with its own script. A speed upgrade:

```rovik run in=textbutton:Buy_Speed
price = 25
self.text = "Speed boost: " + price + " coins"
self.x = 0.78
self.y = 0.5
self.width = 0.2
self.height = 0.06
self.background = true
self.background_color = {r = 40, g = 120, b = 200}

on clicked(p)
    if p.coins == nil or p.coins < price then
        play_sound("error", p)
        return
    end
    p.coins -= price
    p.walk_speed += 4
    play_sound("cash", p)
end
```

Selling a tool works the same way: clone it from Storage into the player. Check they don't have it already, or they'll pay twice:

```rovik run in=textbutton:Buy_Sword with=tool:Sword
price = 50
self.text = "Sword: " + price + " coins"
self.x = 0.78
self.y = 0.58
self.width = 0.2
self.height = 0.06

fn has(p, name)
    for t in p.children do
        if t.name == name then
            return true
        end
    end
    return false
end

on clicked(p)
    if has(p, "Sword") then
        return
    end
    if p.coins == nil or p.coins < price then
        play_sound("error", p)
        return
    end
    p.coins -= price
    sword = clone(find("Sword"))
    sword.parent = p
    play_sound("buy", p)
end
```

## A shop you walk up to

Instead of buttons on the screen, put items on pedestals in the world, with a price floating over each. Touching the pedestal buys it:

```rovik run in=part:Health_Pedestal
price = 15
tag = create("TextLabel", find("Workspace"))
tag.text = "Full health: " + price
tag.attached_to = self
tag.width = 0.14
tag.height = 0.045
tag.background = true

bought_at = {}
on touched(other)
    if other.class != "player" or other.coins == nil then
        return
    end
    -- One purchase per touch, not ten.
    last = bought_at[other.name]
    if last != nil and time() - last < 2 then
        return
    end
    bought_at[other.name] = time()
    if other.coins >= price then
        other.coins -= price
        other.health = other.max_health
        play_sound("cash", other)
    else
        play_sound("error", other)
    end
end
```

## A shop menu you can open and close

Put all the shop's buttons **inside a Frame** called **Shop**. Hiding the Frame hides everything in it. Then one more button toggles it:

```rovik run in=textbutton:Shop_Toggle with=frame:Shop
shop = find("Shop")
shop.visible = false
self.text = "Shop"
self.x = 0.9
self.y = 0.42
self.width = 0.08
self.height = 0.06

on clicked(p)
    shop.visible = not shop.visible
end
```

> **Note:** A shared Frame (in the Workspace) opens for **everyone**. For a menu each player opens on their own, build the shop inside each player when they join, the way the coin label is made.
