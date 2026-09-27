Open Brixo Studio. This page shows you around. It takes five minutes, and everything later in the guide assumes you know where these things are.

![Brixo Studio: Play and Publish along the top, the tools under them, the 3D world in the middle, the Explorer on the left, Properties on the right, and Output along the bottom](img/studio-overview.png)

## Along the top

The navy bar at the very top has:

| Button | What it does |
|---|---|
| **▶ Play (F5)** | Runs your game so you can play it. Press again (**■ Stop**) to go back to building. Nothing you do while playing changes your game: Stop puts everything back. |
| **Players** | How many players Play starts. Set it to 2 or more to test multiplayer: you get a window per player. |
| **Name** and **Publish** | On the right: the name players will see, and the button that puts your game on the website. |

The blue bar under it has the building tools, from left to right:

| Button | What it does |
|---|---|
| **File** | **New**, **Open...**, **Save**, **Save As...**, and **Open a sample game**. See [Saving your game](#saving-your-game). |
| **Move (1)**, **Rotate (2)**, **Scale (3)** | Which handles appear on the selected part. The number keys switch too. |
| **Snap** | Moves in whole studs and turns in 15° steps. Leave it on for neat building. |
| **Undo**, **Redo** | Ctrl+Z and Ctrl+Y (or Ctrl+Shift+Z). |
| **Add Part** | Adds a Part (a block), Wedge, Cylinder or Ball in front of the camera. Also has **Tool**, for things players hold. |
| **Add GUI** | Adds a TextLabel, TextButton or Frame: text and buttons on the players' screens. |
| **Edit** | Copy, paste, duplicate, group into a Model, ungroup, focus. |
| **Add Spawn** | Adds a SpawnLocation, where players appear. |
| **Add Script** | Adds a Script inside whatever's selected, and opens it. |
| **Add Folder** | Adds a Folder, for keeping things organized. |
| **Delete** | Deletes what's selected (so does the Delete key). |

## Saving your game

**File > Save** (Ctrl+S) keeps your game in a `.brixo` file on your computer. The first time, it asks what to call it and where to put it: it starts in the **My Games** folder inside your **Brixo** folder. After that, Save just saves. **Save As...** (Ctrl+Shift+S) saves a copy under a new name.

**File > Open...** (Ctrl+O) opens a game you saved, and **New** (Ctrl+N) starts a fresh one: a big grey baseplate and a spawn pad. You can also **drag a .brixo file onto the Studio window** to open it. **Open a sample game** opens one of Brixo's [sample games](samples) to look around in.

The window's title shows your game's name, with a `*` when it has changes you haven't saved. If you try to open another game or close Studio with unsaved changes, Studio asks whether to save them first.

## Moving around

While building, the camera flies freely:

| Keys | Camera |
|---|---|
| **W A S D** or **↑ ↓** | Fly forward, left, back, right |
| **Space** / **Shift** | Up / down |
| **Hold Ctrl** | Fly faster |
| **Right-drag** the mouse | Look around |
| **← →** and **Page Up / Page Down** | Turn and tilt (handy on a trackpad) |
| **Scroll** or pinch | Fly forward and back |
| **F** | Jump the camera to whatever's selected |

## The 3D view

**Click** a part to select it. Handles appear on it, for whichever tool is picked:

- **Move (1)**: drag an arrow to slide the part along it. Or drag the part itself (away from the arrows) and it slides over whatever's under the mouse, resting on top of it.
- **Rotate (2)**: drag a ring to turn the part.
- **Scale (3)**: drag a handle to make the part bigger or smaller on that side.

**Ctrl-click** selects more than one part, so you can move or copy them together. Or **drag a box** across empty space: everything fully inside it is selected (hold Ctrl to add to what's selected). **Alt-click** picks a single part inside a Model.

**Snap** makes moves and turns jump in steps: pick how far next to it (¼ stud to 4 studs, and 5° to 90° for turning). Untick it to move freely.

With several things selected, Properties can **line them up**: pick an axis (X, Y or Z), then line up their low sides, middles or high sides, or **space them evenly** so the gaps between them match.

## The Explorer

![The Explorer: every object in the game, inside what it's inside](img/studio-explorer.png)

The Explorer lists **every object in your game** as a tree: things inside other things are indented under them. Each kind has its own colored badge.

- Click to select (the same as clicking in the 3D view).
- **Drag** an object onto another to move it inside. That's how you put a script into a part, or parts into a folder.
- **Double-click** or **F2** to rename. Names matter: scripts find things by name. (Double-clicking a **Script** opens it instead; F2 still renames it.)
- **Right-click** for more: rename, duplicate, copy, paste into, group into a Model, ungroup and delete.

## Properties

![Properties: everything about the selected part, changeable](img/studio-properties.png)

Properties shows **everything about the selected object**, and lets you change it: name, position, size, rotation, color, material, shape, and switches like **Anchored** (stays put instead of falling) and **Can Collide** (solid, or something you walk through). Hover over any of them for a hint. Every property here can also be read and changed by scripts, with the same names in lowercase: `anchored`, `can_collide`, `color` and so on. See [Parts](parts). Parts can also have a [hinge](howto-hinges); select the **Workspace** for its [lighting](lighting).

## Scripts

![A script open in its own tab. Rovik is colored as you type, and a line with a mistake turns red straight away, with what is wrong written underneath](img/studio-script.png)

**Double-click a Script** in the Explorer (or select it and press **Edit script** in Properties) and it opens in its own **tab**, filling the middle of Studio. The **World** tab is the 3D view. Keep as many scripts open as you like and click between them; **×** (or Ctrl+W, or a middle-click) closes a tab. Pressing **Play** switches back to the World.

The editor colors your code, numbers the lines, and **checks it as you type**: a mistake shows up in red, on its line, with what's wrong written underneath, before you even press Play. It also suggests names as you type: use the arrow keys to choose one and **Tab** to accept. Above the code is where the script is (`Workspace › Lava › Script`), and **Enabled**: switch it off and the script doesn't run.

## Output

![Output: what your scripts print, and their errors](img/studio-output.png)

Output is along the bottom. Anything a script `print`s shows up here, and so do errors, in red, with the script's name and the line number. **When something doesn't work, look here first.** See [Reading errors](errors).

## Sounds

To add music or a sound effect, **drag an mp3, wav or ogg file onto the Studio window**. It becomes a **Sound** in your game. See [Sounds and music](sounds).

Next: [Your first game](first-game).
