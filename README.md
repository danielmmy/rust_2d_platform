# platformer — a tiny 2D Metroidvania (Bevy 0.19)

A small Hollow-Knight/Ori-style demo: **twelve interconnected rooms** laid out in
a 4×3 grid you traverse up/down/left/right, **keyboard and gamepad** input, and a
**responsive jump**. Built as small Bevy plugins so it's easy to extend, with art
and levels under `assets/` that are simple to swap.

> This is a **detached crate** (Bevy is a very heavy dependency), so it's kept
> out of the shared `cargo check --workspace`. Build/run it from here.

## Run it

```bash
cargo run            # from crates/platformer  (or `make game-run` from the repo root)
```

## Controls

| Action | Keyboard | Gamepad |
| --- | --- | --- |
| Move | `A`/`D` or `←`/`→` | left stick / D-pad |
| Jump | `Space` (dedicated button) | `A` (south) |
| Look up / Crouch | `W`/`↑` · `S`/`↓` | D-pad up · down |
| Crouch-walk (reduced hitbox) | `S`/`↓` + `A`/`D` | D-pad down + a direction |
| Attack (sword) | `J` | `X` (west) |
| **Double jump** (once unlocked) | `Space` again in mid-air | `A` again in mid-air |
| **Wall jump** (once unlocked) | cling a wall, then `Space` | cling a wall, then `A` |
| **Dash** (once unlocked) | `Shift` / `L` (hold to **run**) | right bumper |
| **Pogo** (down-slash, once unlocked) | in the air, hold `↓` + `J` | in the air, hold down + `X` |
| Interact / bench shop | `E` | `Y` (north) |
| Character screen (view stats + abilities) | `C` | left bumper |
| World map | `M` | `Start` |
| Pause | `Esc` | `Select` |

Jump is its **own button** (`Space` / south) — `↑` no longer jumps, so holding **Up**
**looks up** and **Down** **crouches** (each with its own pose; hold a moment and the
**camera pans** that way too, clamped to the room). **Crouching shrinks the hitbox** (it
shrinks from the top, feet planted) so you fit under a one-tile gap or a passing platform;
add a direction to **crouch-walk** (slower than a walk, with its own cycle). You stay
crouched under a low ceiling until there's headroom to stand. Menus confirm with
`Enter` / `Space` / `Z` and step **back / cancel** with `Esc` / **Circle** (the pause
menu also closes on **Select**); **Quit** always asks to confirm first.

**Abilities are earned.** A new game starts with only the **single jump** and **slash**;
**double jump, wall jump, dash, and pogo** must each be unlocked — by **beating a boss** or
**opening a chest** (a solid prop; walk up and press **`E`** — it pops open and stays open).
The character screen (`C`) lists what you've acquired, and the **pause menu** (`Esc`) has an
**Abilities** sub-screen to toggle them on/off: in a **Story** save it lists only your
**acquired** abilities (turn them on/off — you can't grant unearned ones); in a **Builder**
save it lists **all** abilities and **grants/removes** them for testing. (Bosses give no
reward by default; place chests / set boss rewards in the builder to hand them out — see
[Level builder](#level-builder).) The pause **Controls** reference follows the same rule:
in a Story save an ability's line stays hidden until you acquire it (no spoilers), while a
Builder save shows them all.

The game opens on a **main menu** (New Game / Load Game / **Options** / Quit). **New Game**
and **Load Game** open a **ten-slot** picker (each labelled with its `[Story]` or
`[Builder]` type) — pick a slot to load, or to start fresh. A new game asks you to
**choose a mode**, then (after confirming any overwrite) **type a name** for the save:

- **Story** plays the **shipped, read-only levels** — the designed campaign.
- **Builder** starts from a **private, editable copy** of those levels; you can paint,
  resize, add, delete, and relink rooms at will (see [Level builder](#level-builder)),
  and your edits stay in that save only.

**Options** (from the main menu or the pause menu) chooses the **window mode** —
**Windowed** or **Fullscreen (borderless)** — and separate **FX** and **Music** volumes
(confirm a volume row to cycle it 0→100%). Everything applies instantly and is remembered
across launches (saved to `saves/settings.ron`).

During play, **`Esc`** (or `Select`) brings up a **pause menu** (Continue /
**Character** / **Edit Levels** / **Options** / **Main Menu** / Quit); **Character** opens a
read-only stat sheet sub-screen (the same one `C` shows), and **Edit Levels** (Builder
saves only) opens the builder. Menus are navigated with up/down and confirmed with
jump / `Enter`.

Your **health** is a **bar** at the top-left that's **green when full and shades
through yellow to red** as it drops (a continuous bar, so it reads cleanly however
high Vitality pushes your max). You start with three points. Spikes, falling rocks,
and falling into a pit each cost a point, with brief invulnerability after a hit; a
non-fatal hit returns you to the room's entrance. **Lose all your health and you
respawn at the last bench** you rested at, fully restored.

Jump is **hold-to-go-higher**, with **coyote time** (jump just after a ledge) and
**jump buffering** (press just before landing). Rooms connect like Hollow Knight:
**walk off an edge** (through a side corridor, or up/down the central shaft) and
the neighbouring room scrolls in. Each room has its own background colour.

Press **`M`** (or **`Start`**) to open the **world map**, which has **three zoom
levels** — `Space` / **R2** to zoom in and `X` / **L2** to zoom out (zoom sits on the
triggers so **Circle** / `Esc` can close the map / go back). Its on-screen hints draw the
actual button **glyphs** on a controller (via the icon font):

- **Window** (default) — a scrollable 4×3 window of rooms, so each stays readable
  no matter how many you add; it scrolls to follow your selection.
- **World** — the whole map at once (every room glued together, shrunk to fit) for
  an overview.
- **Room** — one room blown up to full detail; arrows step to its neighbours.

The room you're in stays highlighted throughout.

## What makes the jump feel nice

All knobs live in [`MovementConfig`](src/player.rs) — tweak and re-run:

- **Coyote time** — a short grace period to still jump after leaving the ground.
- **Jump buffering** — a jump pressed slightly early fires on landing.
- **Variable height** — releasing jump early cuts the rise short.
- **Asymmetric gravity** — you fall faster than you rise (snappy, not floaty).
- **Apex control** — reduced gravity near the peak for better air steering.
- **Wall slide + wall jump** (Hollow‑Knight style) — in mid‑air, **touch a wall to
  auto‑cling** and slow your fall; **press away** to let go. Press jump to
  **launch up and away** from the wall — a brief control lockout makes sure you leave it —
  so you can **zig‑zag up between two walls**. Works on static walls **and** moving
  platforms.

## Rooms, traversal, and danger

The world is a 4×3 grid of tall rooms (each larger than the screen, so the
**camera scrolls within a room and is bounded to it**). A central vertical shaft
with zig-zag ledges gives the climbing; **ceiling/floor gaps** are the up/down
doors and **side corridors** are the left/right doors. Hazards are sparse and
avoidable: **ground spikes** in dead-end corners and **falling rocks** in the
open. Touching one (or an enemy) costs a **heart** (with brief invulnerability) and
**knocks you back**; only falling into a pit — with nowhere to land — sends you
back to the room's entrance. Lose all three hearts and you respawn at the last
bench.

**Enemies** hurt you on contact. They're all one **`E`** glyph; a room's optional
`enemies` array assigns each cell a **type** by coordinate. Each type
([`ENEMY_KINDS`](src/combat.rs)) is **pure data** — hit points, colour, speed,
**AI**, and its **sprite sheet + animation** — so a new type needs no glyph and (if
it reuses a sheet) no new art. The demo has a purple **patroller** (three hits,
walks and turns at walls/ledges) and a faster **red** one (two hits) that
**chases** you when you come within its aggro range. Swing your **sword** with
**`J`** (gamepad `X`): a generous hitbox (a wide arc) in front of the way you're
facing, and presses chain a **3-hit combo** (the finisher flashes gold). In the air,
**hold Down + attack** for a **pogo** (Hollow-Knight style): the slash points straight
down and, if it lands on **anything** — an enemy, the boss, or a hazard like a spike —
**bounces you back up** (and refreshes your air-jump), so you can chain bounces. A killed
enemy drops an **energy orb** — walk over it to bank **energy** (counted on the
HUD). Enemies respawn when the room reloads (re-entering it, or resting at a bench).

**Benches** are checkpoints **and shops** — the start room has one. Stand on a bench
and press **`E`** (gamepad **`Y`**) — a **`[E] bench`** prompt appears — to open the
**bench menu**, which offers:

- **Rest** — **save** your game, **refill** hearts, and **respawn the room's
  enemies**; the bench you last rested at is where death returns you.
- **Upgrade** — spend **energy** to raise a stat (see below).
- **Leave** — back to play.

(Just walking over a bench does nothing.) Benches show on the world map as warm cells.

**Character, stats & upgrades.** The player has three Dark-Souls-flavoured stats —
**Vitality** (more hearts), **Strength** (more sword damage), and **Poise** (shorter
stagger when hit). Press **`C`** (gamepad **left bumper**) anywhere to open the
read-only **character screen** and check them. **Upgrades are bought at a bench**:
pick a stat in the bench menu and confirm to spend **energy** raising its level. Each
level **costs more** than the last, so energy is a real currency. Stat levels and
banked energy persist in the save.

**Death and bloodstains.** Energy is only *banked* at save points (resting,
upgrading). **Lose all your hearts and you drop every carried point of energy as a
bloodstain** right where you fell, then respawn at the last bench. Walk back to the
bloodstain to **reclaim** it — but **die again first and it's gone for good** (a new
death drops a fresh one and erases the old). A pale marker shows in the room you died
in; the character screen reminds you how much is waiting and where.

Besides the edge doors, rooms can be wired together with **teleporters** — pads
that link two distant rooms (or two spots in the same room) directly. Each pad
stores its destination as explicit room + cell coordinates, shows on the world map
as a cyan cell, and in play is an **animated halo** that flares active while you
stand on it.

**The boss & arenas.** A room becomes an **arena** through its map data: a `fog_wall`
list of combatants to fight, plus **`F`** glyphs painting the Dark-Souls-style mist at
its entrances. The listed foes **aren't there until you enter** — walk in (no
interaction) and crossing the threshold **seals the exits**: you can't leave until
every one of them is dead. Each `fog_wall` entry is `(boss: 0|1, kind, col, row)`: with
`boss: 1` the `kind` picks a **boss type** ([`BOSS_KINDS`](src/boss.rs) — `0` the
original, `1` a tougher red brute with double health); with `boss: 0` it's a normal
enemy of that `kind`. A **boss** has a large health pool (shown on a bar across the
top) and cycles three attacks — a **slam** leap, a **fan of throwables**, and
**summoning** lesser foes (up to two alive at once) — turning more aggressive past half
health. Beat it (and any
companions) for a big **energy** payout and a permanent new ability: the **double
jump** (a second jump in mid-air — press jump again while airborne). The default world
ships two: the original boss in `r0_1` and the red brute in the far corner `r3_2`. Each
is **cleared independently**. Whether an arena comes back is set by its **`fog_respawn`**
flag: with `fog_respawn: 1` it **re-arms on a bench rest** (a transient win, so its foes
respawn — e.g. the three patrollers in `r1_1`); left out (the default) the clear is
**permanent**. A **beaten boss persists** regardless — its kill is saved for the reward —
so the boss arenas stay cleared for good. Die and a live arena resets for another attempt
(your dropped energy waits inside). Author your own by editing a room's `fog_wall` list,
setting `fog_respawn` to taste, and painting **Fog** cells for the mist — see
[`r0_1.map.ron`](assets/maps/r0_1.map.ron).

## Extending it

The structure is plugin-per-concern:

| Module | Responsibility |
| --- | --- |
| [`input`](src/input.rs) | Keyboard + gamepad → one `PlayerIntent`. |
| [`physics`](src/physics.rs) | Hand-rolled AABB-vs-tile collision (unit-tested). |
| [`player`](src/player.rs) | Movement + jump feel; `MovementConfig`; dash; rides moving platforms; unlockable `Abilities`. |
| [`movers`](src/movers.rs) | Generic moving things — carries whatever the grid authors at a mover's cells along a path (loop/pingpong/once); solid ones are ridable. |
| [`scenery`](src/scenery.rs) | Parallax backdrops — looping, multi-speed layers per room from one of 12 themed sets. |
| [`anim`](src/anim.rs) | Extensible sprite-sheet animation: imports N×M grids; player / portal / bench / enemy / boss clips. |
| [`world`](src/world.rs) | Rooms, edge transitions, the 4-way neighbour graph, teleporters, benches, ability **chests**; loads each save's world from its [`LevelRoot`]. |
| [`ron`](src/ron.rs) | A tiny, self-contained RON reader for the map files. |
| [`hazards`](src/hazards.rs) | Spikes + falling rocks → a `Hurt` on contact. |
| [`health`](src/health.rs) | Health (sized by Vitality), i-frames, the colour-graded health-bar HUD, death → bloodstain + last bench. |
| [`audio`](src/audio.rs) | Sound effects (`PlaySfx`) + per-room looping **music**; embedded OGG, with FX/Music volumes in Options. |
| [`combat`](src/combat.rs) | Data-driven enemy kinds (stats/AI/animation), energy drops/pickup, bloodstain recovery, the 3-hit sword combo + **pogo** down-slash. |
| [`stats`](src/stats.rs) | Character stats (Vitality/Strength/Poise), the upgrade shop, and the character screen. |
| [`boss`](src/boss.rs) | The boss fight: fog-gate arena lock, attack patterns, projectiles, HUD, and a configurable ability reward. |
| [`save`](src/save.rs) | Ten-slot save system (mode + room + bench + progression), RON files under `saves/`. |
| [`camera`](src/camera.rs) | Follow camera, bounded to the room; zooms in on small rooms; look-up/down pan. |
| [`worldmap`](src/worldmap.rs) | Pause-screen world map (`M`): overview + per-room zoom. |
| [`menu`](src/menu.rs) | Main menu (mode + slot picker) + pause menu (`Esc`) + Options (window mode, FX/Music volume); `MainMenu`/`Paused` states. |
| [`editor`](src/editor.rs) | Level builder (`F2` / pause **Edit Levels**, Builder saves): tile view + room map; places chests, sets boss rewards, ability test menu. |

The crate's **only dependency is `bevy`** — the maps are `.map.ron` files read by
our own [`ron`](src/ron.rs) parser (a small `AssetLoader` in [`world`](src/world.rs)
plugs it into Bevy's asset pipeline), so there's no `serde`/`ron` crate to pull in.

### Add or edit a room

Rooms are ASCII grids in `assets/maps/<name>.map.ron`. Each edge is a **list of doors** —
walk off the edge and you cross through one (empty list = a wall / bottomless edge):

```ron
(
    name:   "Forest Glade",      // display name (empty → shows the file key)
    solid:  "#",                 // solid tiles
    spikes: "^",                 // deadly ground spikes
    rocks:  "R",                 // falling-rock spawners
    // Each door is ((origin_col, origin_row), "to_room", (dest_col, dest_row)):
    //   origin = the cell on THIS edge you walk off; dest = where you land THERE.
    north:  [((9, 0), "r0_1", (9, 19))],   // off the top edge → r0_1
    south:  [],                            // …bottom edge (sealed)
    east:   [((39, 1), "r1_0", (1, 2)),    // a top doorway and…
             ((39, 20), "r1_0", (1, 19))], // …a bottom doorway, both → r1_0
    west:   [],                            // …left edge
    teleports: [                 // teleporter pads (optional)
        // a pad at (col 1, row 1) → arrive at r3_2's cell (col 14, row 20)
        (origin_col: 1, origin_row: 1, to: "r3_2", dest_col: 14, dest_row: 20),
    ],
    enemies: [                   // types for `E` cells (optional; default = kind 0)
        (kind: 1, col: 3, row: 1),   // the `E` at (3, 1) is enemy type 1
    ],
    movers: [                    // movers (optional) — carry whatever is at the cells
        // the 3 tiles authored at (22,18)..(24,18) loop through the path at 70 px/s,
        // pausing 700 ms per stop; solid tiles are ridable and carry you.
        (tiles: [(22,18),(23,18),(24,18)], path: [(22,9),(30,9),(30,18)],
         mode: "loop", speed: 70, rest: 700),
    ],
    bg:     [0.32, 0.16, 0.16],  // background colour [r, g, b] in 0..1
    tiles: [ "######", "#.@E#", "######" ],   // grid, top to bottom; `@` = start, `E` = enemy
)
```

**Doors** carry their own coordinates, so a room places the player **exactly** where it
wants — no shared door layout required. An edge can hold **several** doors (e.g. a top and
a bottom doorway); the one you take is whichever door's `origin` is nearest where you
crossed, and you appear at that door's `dest` cell (grid coords, `row` from the top).
Doors are **one-way** by design: for a two-way passage, give the other room a door back.
The level builder's **Door** brush writes these for you (see below); by grid convention,
adjacent `r{col}_{row}` rooms also get a default door automatically.

A teleporter is **pure coordinate data** — no grid glyph, so pads never use up tile
characters. Each entry names its own cell (`origin_col`/`origin_row`) and its
**destination** room + cell (`to` / `dest_col` / `dest_row`); all are grid
coordinates (`row` counts from the top, like the `tiles` lines). Because nothing is
matched by glyph:

- a room can hold **many** pads, and
- a pad can target **its own room** at another cell (a self-portal).

For a two-way link, give each end a pad pointing at the other's cell. A pad won't
fire again until you've stepped ~1.5 tiles clear of it, so you land safely on the
destination pad and don't bounce back and forth.

**Movers** are a generic "move what's already there" system: a mover **doesn't draw
anything** — it picks up whatever the grid authors at its `tiles` cells (a solid block, a
spike, a bench, …) and carries those entities along a path. So the same data makes a
ridable platform, a sweeping spike, a roving bench, etc. — just put the glyph you want at
the cells. A mover is a rigid group whose **anchor** (`tiles[0]`) travels the `path`; every
other tile keeps its offset and is dragged along, so each stop is written once. It starts
at its `tiles` home, glides to each `path` cell at `speed` **px/s** (uniform motion),
pauses `rest` **ms** at each, then continues by `mode`: **`loop`** (cycle home→stops→home
forever), **`pingpong`** (bounce back and forth), or **`once`** (stop at the last cell).
**Solid** cells become ridable — stand on one and it **carries you** (including up/down
lifts); non-solid cells keep their own behaviour (a moving spike still hurts). A platform
coming **down** onto you first **forces you to crouch** (you duck under it) and only **hurts**
you (a true crush) if it keeps descending into the crouched box with no room left. Platforms
moving **sideways** or **up**, or ones you press against from the side, just block you — they
never crush. The starter
rooms show one of each mode over a 3-tile block: `r0_0` a `loop` patrol, `r1_0` a
`pingpong` slider, `r2_0` a `once` lift. (Collision is a static cell grid, so a mover's
solid tiles are lifted out of it and resolved as dynamic AABBs — see
[`movers`](src/movers.rs) + [`physics`](src/physics.rs).)

**Enemies** use one `E` glyph in the grid; the optional `enemies` array gives a
`kind` (a [`combat::ENEMY_KINDS`](src/combat.rs) index) to the `E` at `(col, row)`.
An `E` with no matching entry uses kind 0, so painting `E` in the builder just works,
and you can define any number of types without spending more glyphs.

Each room has an optional **display name** (e.g. "Forest Glade", "Meadow") shown
on the world map and in the builder; when empty it falls back to the file key.

Rooms are **discovered** from `assets/maps/` at startup, so just dropping a new
`.map.ron` adds it — no code change. Rooms are named `r{col}_{row}` (`r0_0`
bottom-left, the start); the grid is **unbounded** (columns/rows can be any
non-negative integer). Each door records its own destination cell, so rooms line up
regardless of size — and the names only matter for the builder's grid auto-linking.

When a room is **smaller than the screen**, the camera zooms in so the room fills
the viewport; larger rooms stay at 1:1 and scroll.

### Parallax scenery

Rooms get a layered, looping backdrop — **mix-and-match per layer**, so each of the four
layers picks its own set:

```ron
scenery: (far: "snowy_mountains", mid: "forest_meadow", near: "forest_meadow", fg: "misty_swamp"),
```

The layers are **far** (a wide, seamless sky), **mid**/**near** (silhouette bands), and
**fg** (a sparse foreground); [`scenery`](src/scenery.rs) handles them Silksong-style:

- **far/mid/near** sit *behind* gameplay; they **wrap horizontally** (any room width loops)
  and use **vertical parallax** so the backdrop scrolls down as you climb rather than
  riding up with the camera (the far sky stays put behind, always filling).
- **fg** is a real *foreground* drawn **in front** of the player but **anchored to the
  ground**, so its sparse tufts sit at your feet and scroll off as you rise — never
  covering you.

Twelve sets ship — **forest meadow, deep caves, snowy mountains, sandy beach, desolate
desert, mushroom hollow, volcanic depths, sunset cliffs, crystal grotto, autumn woods,
misty swamp, starry void** — each in `assets/scenery/<set>/` (`far/mid/near/fg.png`). In
the builder, **`V`** picks which layer you're editing and **`C`** cycles that layer's set
(including *none*). Regenerate/retheme with `tools/gen_scenery.py`.

### Level builder

In a **Builder** save, open the **level builder** with **`F2`** while playing — or
pick **Edit Levels** from the pause menu. It has two views; saving writes the
`.map.ron` files **in that save's own level directory** (`saves/builder<slot>/maps/`)
and updates the running game, so leaving the builder shows your edits. Story saves
never reach the builder — the shipped `assets/maps/` levels stay read-only.

**Tiles** — paint the selected room with the game's own sprites:

| Key | Action | Key | Action |
| --- | --- | --- | --- |
| arrows | move cursor | `[` / `]` | width − / + |
| `Space` | paint brush | `-` / `=` | height − / + |
| `X` | erase | `B` | recolour |
| `Tab` | cycle brush | `V` / `C` | scenery: pick layer / set |
| `G` | trace stamp shape | `S` | save |
| `N` | room music (cycle track) | `M` | room manager |
| `P` | mover (moving tile) / delete | `Esc` | leave the builder |
| `K` | chest brush's granted ability | `O` | this room's boss reward |
| `Y` | ability test menu (toggle on/off) | `Enter` | rename (type a name) |
| `Space` (Portal/Door brush) | start a portal / door link | | |
| `Space` (Chest brush) | place / remove a chest | | |

**Abilities** (chests & boss rewards) — `Tab` to the **Chest** brush and `Space` to place a
chest (`Space` again on it, or **`X`**, removes it); **`K`** cycles which ability it grants
(Double Jump / Wall Jump / Dash / Pogo). In play a chest is a **solid** prop you open with
**`E`**, leaving an open chest behind. **`O`** cycles the **boss reward** — the ability this
room's boss hands over when beaten (none by default). Press **`Y`** to open an **ability test
menu** that toggles your unlocked abilities on/off (it edits the save, applied when you `F2`
back to play) so you can test gated areas without grinding.

**Rooms** (`M`) — manage the world as a grid:

| Key | Action | Key | Action |
| --- | --- | --- | --- |
| arrows | move selection | `A` | add a room here |
| `Enter` | edit the room | `D` | delete the room |
| `G` | grab / drop (reorder) | `R` `R` | reset to the default 12 |
| `M` / `Esc` | back to tiles | | |

The room manager scrolls, so you can place **unlimited** rooms. Grid adjacency gives
you connectivity for free: a room named `r{col}_{row}` is auto-linked to its existing
N/S/E/W neighbours with default doors, and standard-size (40×22) rooms get those doors
opened/sealed to match. Rooms can still be **any size** in the tile view. For anything
beyond the grid — a door to a non-adjacent room, or a second doorway on one edge — use
the **Door** brush (below). The builder edits a Builder save's own copy on disk; the
shipped Story levels stay read-only.

**Doors** — `Tab` to the **Door** brush and paint an **origin** cell on the edge you
want to leave from; the room manager opens so you pick the destination room (`Enter`) —
then you paint the **landing** cell there. The builder files the door under the nearest
edge, carves an opening at the origin so you can walk off, and saves the source room.
Doors are **one-way** (repeat the other direction for a return trip). Press **`Esc`**
before placing the landing cell to cancel — nothing is written until the link completes.

**Portals** — `Tab` to the **Portal** brush and paint to drop the first endpoint;
the room manager opens so you can pick the destination room (`Enter`) — **including
the same room**, for a self-portal — then you paint the exit. The builder records
each pad's cell and links the two both ways automatically, saving both rooms. Press
**`Esc`** any time before the exit is placed to cancel — the first endpoint is only
written once the link completes, so cancelling leaves nothing behind. Pads show as
cyan cells; erase one (`X` over it) to remove that side. (Destinations are fixed
cell coordinates, so moving a pad's room doesn't update its partner — re-link after
such a move.)

**Benches** — `Tab` to the **Bench** brush and paint to place a checkpoint (the
grid glyph `B`). In play, stand on it and press `E` to rest — saving the game,
refilling hearts, and resetting enemies; it's also where the player respawns after
losing all hearts.

**Stamp** (multigrid brush) — **trace a shape, then paint it with any brush.** Press
**`G`** to start at the cursor, **move** the cursor to draw the shape you want (each cell
you pass over is marked, shown in amber), and press **`G`** / `enter` to finish (`esc`
cancels). Now `space` paints **the whole shape** with the current brush — and `X` erases
it — anchored at the cursor, so you stamp it wherever you like. The footprint previews in
cyan as you move; `Tab` picks the fill item (or Erase) as usual. Trace a single cell again
to go back to normal one-cell painting. The shape lives in the editor only (not saved).

**Movers** (moving tiles) — a mover carries *whatever tiles you select*, so it's a moving
**platform**, a sweeping **spike**, a sliding **door**, a roving **bench**, … Paint the
tiles first (a strip of `#`, a spike, a bench, …), then press **`P`** to author a
[mover](src/movers.rs). It runs in two steps, like the stamp: **(1) Select the area** —
move the cursor to trace the tiles (the **first cell is the home anchor**, drawn hotter);
`P`/`enter` advances. **(2) Mark the stops** — move the cursor and press **`space`** at
each point it should travel to; a cyan preview shows where it'll sit. While marking, `Tab`
cycles the **mode** (loop / once / ping-pong), `-`/`=` change **speed**, and `[`/`]` change
the **pause** at each stop; `P`/`enter` finishes (writing it into the room's `movers`),
`esc` cancels. Existing movers show as orange cells with bright stop dots — press **`P`**
on one to delete it.

### Replace the art

The shipped sprites and Story levels are **baked into the binary** at build time (see
[`build.rs`](build.rs)), so a release exe is self-contained — no `assets/` folder
needed to run. Edit the source files and **rebuild** to embed the new versions.

Drop your own PNGs over the placeholders in `assets/sprites/`
(`tile.png`, `spikes.png`, `rock.png`, `enemy.png`, `jumper.png`, `flyer.png`,
`orb.png`, `slash.png`, `chest.png` / `chest_open.png` — the last two drawn by
[`tools/gen_chest.py`](tools/gen_chest.py)). Sizes are set in code via `custom_size`, so any resolution
works — the world keeps the same scale. The enemy sheets (`enemy.png` walkers,
`jumper.png` leapers, `flyer.png` winged flyers) are **near-white** so each kind tints
them to its colour.

**`player.png`, `portal.png`, `bench.png`, and `boss.png` are sprite sheets** — each an
N×M grid of equal frames that [`anim`](src/anim.rs) imports into a texture atlas (sizing
every frame from the image ÷ grid, so a re-drawn sheet of the same grid just works):

- `player.png` is **6×7** — a **side-profile** character facing right (the
  [`player`](src/player.rs) flips it to face left, so it faces the way you walk).
  Rows: 0 = idle (last frame blinks), 1 = walk, 2 = jump, 3 = damage, **4 = crouch**
  (hold Down), **5 = look-up** (hold Up), **6 = sprint** (running). Driven by state —
  damage → jump (airborne) → run/walk (moving) → crouch/look-up (still) → idle; the jump
  plays **once across the arc** (launch → apex → fall), keyed to velocity. Rows 4–6 are
  generated by [`tools/gen_player_poses.py`](tools/gen_player_poses.py) (squash / stretch /
  forward-lean) — rough placeholders to redraw by hand; the first four rows are untouched.
- `portal.png` is **6×2**: row 0 = idle halo, row 1 = active (while the player is on
  the pad) — an upright vortex, both looping.
- `bench.png` is **6×1**: a static wooden bench whose **fairy lights** drift and
  twinkle (one looping clip).
- `boss.png` is **8×6** — the horned demon in **¾ profile facing right** (the
  [`boss`](src/boss.rs) flips it to face the player). Rows: 0 = idle/hover,
  1 = advance, 2 = slam (wind-up `16-21`, dive/impact `22-23`), 3 = throw wind-up,
  4 = summon wind-up, 5 = recover. The three wind-up clips are **Manual**, their frame
  keyed to the attack's wind-up timer so each attack has a clear, fairly-timed
  **telegraph** (rear-up + roar before a slam, a swelling ember before a throw, a
  growing rune aura before a summon).

For a different grid, change the `*_COLS`/`*_ROWS` and `Clip` constants in
[`anim`](src/anim.rs). To animate something new, load a sheet, attach a
`SpriteAnimation`, and add a small controller that calls `SpriteAnimation::play`.

### Sound

[`audio`](src/audio.rs) handles both effects and music; all clips are **OGG baked into the
binary** (Bevy plays OGG via its default `vorbis` feature). Two volumes — **FX** and
**Music** — live in the **Options** menu (main menu or pause) and persist to
`saves/settings.ron`.

**Sound effects** live in `assets/sounds/*.ogg`, synthesised (no recordings) by
[`tools/gen_sfx.py`](tools/gen_sfx.py) — plain Python plus `ffmpeg`; run
`python3 tools/gen_sfx.py` to regenerate after tweaking the recipes. To add one: add a recipe,
a [`Sfx`](src/audio.rs) variant + file name, and fire `PlaySfx(Sfx::Yours)` from a system.
Wired: footsteps, jump, double-jump, wall-jump, land, slash, the combo finisher, enemy/boss
hit, taking damage, energy pickup, and a **save jingle** when you rest at a bench.

**Music** lives in `assets/music/<theme>.ogg` — one looping track per theme set, real
**CC0 / public-domain** songs from [OpenGameArt](https://opengameart.org) (down-mixed to mono
and length/quality-trimmed to stay small; see [`assets/music/CREDITS.md`](assets/music/CREDITS.md)).
Each room names a track in its `music:` field (one of the 12 sets, or empty for silence); the
shipped rooms default to their theme. The track follows the room and **loops**, only switching
when you enter a room with a different track (so resting at a bench doesn't restart it). In the
level builder, **`N`** cycles the current room's track. Replace a song by dropping your own
`<theme>.ogg` in and rebuilding.

### Fonts

Most UI text uses Bevy's built-in font. The pause **Controls** sheet additionally draws its
key/button glyphs with [PromptFont](https://shinmera.com/promptfont) — a single icon font
whose ASCII alphabet stays readable while modifier keys and gamepad buttons map to glyphs. It
lives at `assets/fonts/promptfont.ttf` (baked into the binary via [`build.rs`](build.rs), like
the sprites and audio) and is licensed **SIL OFL 1.1** — see
[`assets/fonts/LICENSE-PromptFont.txt`](assets/fonts/LICENSE-PromptFont.txt). Bevy's default
font is ASCII-only, so any non-ASCII drawn in it shows up as tofu; that's why glyphs go through
PromptFont and the rest of the on-screen text sticks to plain ASCII.

## Status

Compiles against Bevy 0.19 (debug and release); the collision logic, the room
graph (every room parses and links to real rooms), the RON round-trip, and the
builder's default-room generator are unit-tested. The **feel and visuals are yours
to judge by running it** — they can't be verified headlessly, and the room layouts
are deliberately simple scaffolds to build on.

## Changelog

- **2026-06-29** — **False-crush fixes.** A crush now only hurts when a platform is **actively
  descending onto you from overhead** (and you're under its span) with no room below — so
  jumping up into a *laterally*-moving platform, or pressing against a platform's side, just
  bonks/blocks you instead of dealing damage. The crouch now also **persists in mid-air** while
  a low platform blocks standing, so jumping under one keeps the small hitbox (you bonk it)
  rather than un-crouching into a false crush. Removed the separate horizontal squish (it was
  the source of the side-press damage). ([`physics`](src/physics.rs), [`player`](src/player.rs))
- **2026-06-29** — **Platform teleport fixed (minimum-penetration resolve).** The real cause of
  the platform "teleports": the X pass side-pushed *any* overlap — including a player **below**
  a platform (jumping up into it) or one it was pressing down on — flinging them out to the
  platform's edge. Each platform is now resolved along the axis you're **least embedded in**: a
  vertical overlap is pushed up/down (land on top / bonk your head / get crushed), only a true
  side overlap is pushed sideways. The Y pass is now position-based, so a descending platform
  pushes you **down** (or crushes you in place — hurt, no teleport) instead of seating you up
  onto itself. Replaces the old shove-out squish entirely. ([`physics`](src/physics.rs),
  [`player`](src/player.rs))
- **2026-06-29** — **More squish/ride fixes.** Reverted an over-eager "centre must be over the
  span" rider check that was ejecting riders near a platform's **edge** to the edge; the
  rider-skip is feet-near-top again, so edge riders just walk off naturally. The **horizontal
  squish** is now **damage-only** — a sideways platform pinning you against a wall hurts you and
  the knockback nudges you free, instead of teleporting you up over the platform.
  ([`physics`](src/physics.rs), [`player`](src/player.rs))
- **2026-06-29** — **Squish/crouch fixes.** The forced crouch now triggers **only for a
  platform descending onto you from overhead** (`physics::ducking_under`) — riding a platform
  **up**, or pressing against one's **side**, no longer spuriously auto-crouches. The horizontal
  squish now needs a real side-on bite (≥ half the body), so brushing a platform raised a tile
  overhead no longer shoves you up. ([`player`](src/player.rs), [`physics`](src/physics.rs))
- **2026-06-29** — **Graduated squish + horizontal squish + side-ride fix**
  ([`player`](src/player.rs), [`physics`](src/physics.rs), [`movers`](src/movers.rs)). A
  platform coming **down** now **forces a crouch** first (you duck under it) and only **hurts**
  you once it descends into even the crouched box — instead of immediately damaging. Added a
  **horizontal squish**: a sideways platform pressing you into a wall hurts you and pops you
  out vertically (mirrors the vertical one). Fixed a bug where pressing into the **side** of a
  moving platform could teleport you onto its top — the rider check now requires your centre to
  be over the platform's span, and each mover's contiguous tiles are merged into one span so a
  rider straddling a seam isn't mistaken for a side-hit.
- **2026-06-29** — **Crouch hitbox + crouch-walk** ([`player`](src/player.rs),
  [`physics`](src/physics.rs), [`anim`](src/anim.rs)). Holding **Down** on the ground now
  **shrinks the collision box** (`CROUCH_HALF`, ~⅔ height) — it shrinks from the top with the
  feet planted, so the sprite doesn't move — letting the player fit under a one-tile gap or a
  passing platform (the mover slides over a crouched player instead of squishing them). Adding
  a direction (e.g. `S`+`D` / Down + a direction) does a **crouch-walk**: slower than a walk
  (`crouch_speed`), with its own squashed walk cycle (player sheet is now **6×8**, row 7 from
  [`tools/gen_player_poses.py`](tools/gen_player_poses.py)). You stay crouched under a low
  ceiling until there's headroom to stand, so you can't pop through it. A **riding** player's
  horizontal carry is now collision-checked too, so standing on a moving platform no longer
  slides you through a one-tile gap that should require a crouch.
- **2026-06-29** — **World-map glyphs + trigger zoom** ([`worldmap`](src/worldmap.rs),
  [`input`](src/input.rs)). The map's on-screen hints now render the actual button **icons**
  on a controller (close / move / zoom) instead of words, using the same embedded icon font as
  the Controls sheet (the hint text sets the font at spawn, so it never flashes on a redraw).
  **Zoom moved to the triggers** — **R2** in / **L2** out (still `Space` / `X` on keyboard) —
  which frees **Circle** to act as **back**: `Esc` / **Circle** now closes the map (the `M` /
  **Start** toggle still works too).
- **2026-06-29** — **Menu back/cancel shortcut + quit confirmation** ([`menu`](src/menu.rs)).
  `Esc` / **Circle** now steps back one screen anywhere in the title or pause menus (resuming
  the game from the pause root), so you no longer have to scroll down to a "Back" row; the
  gamepad **Select** button still closes the pause menu outright. Opening the pause menu is
  now open-only (it can't resume on the same press that's meant to navigate). **Quit** (from
  either menu) now routes through a **"QUIT GAME?"** confirmation defaulting to *Back*, so a
  stray click can't exit the game.
- **2026-06-29** — Added a **Controls** reference to the pause menu ([`menu`](src/menu.rs)):
  a read-only screen with the action labels grouped into **Movement / Actions / Menu**
  sections and two glyph columns — **keyboard** and **controller**. The key/button tokens
  render as real icons (keycaps, PlayStation face buttons, shoulders, d-pad, sticks) using
  the embedded [PromptFont](https://shinmera.com/promptfont) icon font (SIL OFL 1.1; see
  [`assets/fonts/`](assets/fonts/)) — Bevy's default font is ASCII-only and would draw such
  symbols as tofu, so the glyph rows are re-fonted on spawn while labels stay in the base font.
  Descriptive only — bindings aren't configurable. The unlockable abilities (double jump,
  wall jump, dash, pogo) are **gated in Story** — each appears only once you've acquired that
  ability, so the screen never spoils what you haven't found. A Builder save lists them all.
  The same icon font also drives the in-world **bench / chest prompts**: on a controller they
  now show the **Triangle button glyph** (`[△] rest` / `[△] open`) instead of the word, and
  fall back to `[E]` on keyboard.
- **2026-06-29** — **Context-sensitive control hints.** A `LastInput` resource
  ([`input`](src/input.rs)) tracks the most recently used device, and on-screen prompts now
  match it: the bench/chest prompts, the character/bench overlay hint, and the world-map
  hints switch between **keyboard keys** and **PlayStation labels** (Cross / Circle / Triangle /
  L1 / R1 / Options) as you swap between keyboard and a controller. (The bench/chest prompts
  later became actual icon glyphs — see the Controls entry above.)
- **2026-06-29** — **Hold the dash button to run**, with its own animation. After the dash
  burst, keeping the dash button held + a direction settles into a sustained **sprint**
  (`sprint_speed`, between walk and dash speed) with a new **sprint cycle** (player sheet now
  **6×7**, row 6 generated by [`tools/gen_player_poses.py`](tools/gen_player_poses.py)). The
  run state is decided on the ground and **carried through jumps/falls**, so a jump keeps the
  sprint **momentum** and landing on solid ground **keeps you running**; release to slow down
  ([`player`](src/player.rs), [`input`](src/input.rs), [`anim`](src/anim.rs)). Requires Dash.
- **2026-06-28** — Added an **Abilities** sub-screen to the **pause menu** ([`menu`](src/menu.rs)).
  **Story** saves toggle only **acquired** abilities active/off; **Builder** saves toggle
  **all** of them (grant/remove for testing, persisted). (The level builder's `Y` menu still
  works too.)
- **2026-06-28** — **Chests** are now **solid props you open with `E`** (was walk-over):
  walk up — an `[E] open` prompt shows — press `E` to take the ability; the chest swaps to an
  **open sprite** ([`chest_open.png`](assets/sprites/chest_open.png)) and stays open. In the
  builder, **`X`** also removes a chest (not just `Space` toggling). ([`world`](src/world.rs))
- **2026-06-28** — **Abilities are now acquired, plus a new Dash.** A fresh game has only the
  **single jump + slash**; **double jump, wall jump, dash, and pogo** are gated
  ([`player::Abilities`](src/player.rs)) and unlocked by **beating a boss** (its `boss_reward`
  is set per room — **no default**; an unset boss gives only energy — [`boss`](src/boss.rs)) or
  **opening a chest**
  ([`world`](src/world.rs); sprite from [`tools/gen_chest.py`](tools/gen_chest.py)); both
  persist in the save. The **dash** is a quick horizontal burst (`Shift`/`L` / right bumper,
  short cooldown, one air-dash per jump). The character screen lists acquired abilities. In
  the level builder: a **Chest** brush (`K` picks its ability), **`O`** sets the room's boss
  reward, and **`Y`** opens an ability test menu. Save format: `double_jump` became an
  `abilities` list (old saves migrate), plus a `chests` field for opened chests.
- **2026-06-28** — Resting at a bench now plays a short **save jingle** (an ascending
  C-major arpeggio) as audible confirmation that the game saved & restored
  ([`audio`](src/audio.rs) `Sfx::Save`, synthesised in [`tools/gen_sfx.py`](tools/gen_sfx.py);
  fired from the bench *Rest* action in [`stats`](src/stats.rs)).
- **2026-06-28** — **Controls + moveset.** Jump is now a **dedicated button** (`Space` /
  south) — `↑` no longer jumps ([`input`](src/input.rs)). Holding **Up** **looks up** and
  **Down** **crouches**, each with its own pose (player sheet extended to **6×6**, new rows
  generated by [`tools/gen_player_poses.py`](tools/gen_player_poses.py); [`anim`](src/anim.rs))
  — and the **camera pans** up/down after a brief hold, clamped to the room
  ([`camera`](src/camera.rs)). Added a **Hollow-Knight pogo** ([`combat`](src/combat.rs)): an
  airborne **Down + attack** slashes downward and bounces you up — with a strong impulse for
  traversal — off any enemy, the boss, or a hazard (**spikes/rocks**), refreshing the air-jump
  so bounces chain. A short post-pogo grace means bouncing across a **spike pit is safe**.
- **2026-06-28** — Added **background music** ([`audio`](src/audio.rs)). 12 real **CC0 /
  public-domain** tracks (one per theme set) from [OpenGameArt](https://opengameart.org),
  down-mixed to mono and trimmed to stay small (each <1.4 MB), **baked into the binary**
  (`assets/music/`, see [`CREDITS`](assets/music/CREDITS.md)). Each room names its track in a
  new `music:` field (`MapData`); the loop **follows the room** and only switches on a real
  change. The level builder's **`N`** key cycles a room's track, and **Options** now has
  separate **FX** and **Music** volume controls (persisted to `saves/settings.ron`).
- **2026-06-28** — Added **sound effects** ([`audio`](src/audio.rs)). Ten synthesised OGG
  clips — footsteps, jump / double-jump / wall-jump / land, slash + the combo finisher,
  enemy & boss hit, taking damage, and energy pickup — generated by
  [`tools/gen_sfx.py`](tools/gen_sfx.py) (pure Python + `ffmpeg`), **baked into the binary**
  via `build.rs`. Gameplay systems fire a `PlaySfx` message; a one-shot player decodes and
  plays it. Bevy's default `vorbis` feature plays OGG, so no new dependency.
- **2026-06-28** — Movers can now **crush** the player ([`physics`](src/physics.rs) +
  [`player`](src/player.rs)). A platform descending onto someone standing on the ground used
  to clip them up through it; now, when a falling platform's underside bites into a player who
  has no room below, they're **shoved out the nearer open side** (away from walls) and **take
  a hit** (knockback + i-frames), instead of teleporting to its top.
- **2026-06-28** — Fixed a **mover riding** bug ([`physics`](src/physics.rs)): walking on a
  moving platform could occasionally **teleport** the player to its side. The X-collision
  pass now ignores a platform the player is resting on top of (the carry can sink the feet a
  hair into the box before the Y pass re-seats them), so it's no longer mistaken for a wall.
- **2026-06-28** — The level builder can now **author movers** (moving tiles — a platform,
  spike, door, …) ([`editor`](src/editor.rs)). Press **`P`** to (1) trace the tiles (first
  cell = home anchor) and (2) mark the stop points it travels through (`space`), tuning **mode**
  (`Tab`: loop / once / ping-pong), **speed** (`-`/`=`) and **pause** (`[`/`]`) live, with a
  cyan preview of each stop. Writes a [`Mover`](src/world.rs) into the room's `movers`;
  existing movers draw as orange cells + stop dots, and `P` on one deletes it. (Movers were
  previously only hand-edited in the `.map.ron` files.)
- **2026-06-28** — Added a **multigrid stamp brush** to the level builder
  ([`editor`](src/editor.rs)). Press **`G`** to **trace a shape** by moving the cursor (each
  cell visited is marked, in amber), `G`/`enter` to finish; then `space` paints **the whole
  shape** with the current brush (and `X` erases it), anchored at the cursor, with a live
  cyan footprint preview. The fill item is just the normal brush selection (`Tab`), so the
  same shape stamps walls, spikes, enemies, … The shape is editor-only (not saved).
- **2026-06-28** — Added an **Options** menu (main menu + pause) to choose the **window
  mode** — windowed or **borderless fullscreen** — applied live and persisted to
  `saves/settings.ron` (new [`Settings`](src/save.rs), loaded at startup and pushed to the
  window by `apply_window_mode`).
- **2026-06-28** — Reworked **scenery** to be mix-and-match and more Silksong-like. Each room
  now picks a set **per layer** (`scenery: (far:…, mid:…, near:…, fg:…)`), so backdrops blend;
  the builder edits a layer with **`V`** and its set with **`C`**. The **fg** is now a real
  foreground — sparse tufts drawn **in front** of the player but **ground-anchored** so it
  never hides you or rides up as you climb. The **far** sky is wider and seamless (no more
  repeating "sun"); mid/near keep horizontal wrap + vertical parallax. Sets moved to
  `assets/scenery/<set>/{far,mid,near,fg}.png` (nested embedding in `build.rs`).
- **2026-06-27** — Added **parallax scenery** ([`scenery`](src/scenery.rs)). Each room can
  name a **scenery set**; the system spawns its four tileable layers (far/mid/near + a
  foreground) and drifts each at its own parallax speed, **wrapping horizontally** so any
  room width loops. **12 themed sets** ship (forest meadow, deep caves, snowy mountains,
  sandy beach, desolate desert, mushroom hollow, volcanic depths, sunset cliffs, crystal
  grotto, autumn woods, misty swamp, starry void), assigned across the 12 default rooms and
  cyclable per-room in the builder (`V`). Art generated by `tools/gen_scenery.py` →
  `assets/scenery/`, embedded via `build.rs`.
- **2026-06-27** — Added **wall slide + wall jump** ([`player`](src/player.rs)),
  Hollow‑Knight style: touching a wall in the air **auto‑clings** and slows the fall
  (`wall_slide_speed`); you let go by pressing **away**. Jumping **launches up and away**
  (`wall_jump_x`) with a short control lockout (`wall_jump_lock`) so you can zig‑zag
  between walls. Jump priority is ground → wall → double‑jump; works against static tiles
  and moving platforms (new `physics::wall_at`).
- **2026-06-27** — Added **movers** ([`movers`](src/movers.rs)), a generic "move what's
  already there" system. A map's `movers` list — `(tiles, path, mode, speed, rest)` —
  **adopts whatever entity the grid authors at its cells** (a solid, spike, bench, …) and
  carries it along a path at `speed` px/s, pausing `rest` ms per stop, cycling by
  **`loop` / `pingpong` / `once`**. **Solid** cells are lifted out of the static grid into
  dynamic AABBs the player resolves against *and is carried by* when standing on top (new
  [`Platforms`](src/physics.rs) pass in player movement); non-solid cells keep their own
  behaviour (a moving spike still hurts). The RON reader, `to_ron`, and the editor preserve
  them. Starter rooms `r0_0`/`r1_0`/`r2_0` demo the three modes over a 3-tile block.
- **2026-06-26** — Boss polish: lifted the sprite so its feet rest on the ground instead
  of clipping into it (an [`Anchor`](src/boss.rs) offset for the hitbox/art height gap),
  and **capped Summon at two live minions** (`MAX_SUMMONS`) so the arena can't be flooded.
- **2026-06-26** — Animated the **boss**: `boss.png` is now an **8×6 sheet** of the horned
  demon (idle/hover, an 8-frame advance, slam, throw, summon, recover — 40+ frames) drawn
  in ¾ profile facing right, so [`boss`](src/boss.rs) flips it to **face the player**.
  Each attack now has a **telegraph** — a Manual wind-up clip whose frame tracks the
  wind-up timer (rear-up + roar before a slam, a swelling ember before a throw, a growing
  aura before a summon) — giving the player a fair window to react. Driven by a new
  `Boss::pose()` + `control_boss`/`attach_boss` in [`anim`](src/anim.rs).
- **2026-06-26** — Reworked room connections into **coordinate doors**. Each edge is now a
  list of [`Door`](src/world.rs)s — `((origin_col, origin_row), "to", (dest_col, dest_row))`
  — replacing the old plain neighbour names. Walking off an edge takes the door nearest the
  crossing and drops you at its `dest`, so a room places you **exactly** where it wants;
  an edge can hold **multiple** doors (e.g. r0_0's two east doorways). The RON reader gained
  positional-tuple parsing; the editor gained a **Door** brush (pick an origin, a room, then
  a landing cell) and still auto-links grid-adjacent rooms with default doors. All 12 shipped
  maps were converted. (Supersedes the same-day `*_entry` experiment.)
- **2026-06-26** — Generalised arena respawning into a per-room **`fog_respawn`** flag
  ([`MapData`](src/world.rs), mirrored into [`BossFight::respawn`](src/boss.rs)). An arena
  with `fog_respawn: 1` re-arms on the next **bench rest** (transient `ClearedArenas`);
  without it the clear is **permanent** (persisted `ClearedBosses`). Replaces the previous
  implicit rule (boss → permanent, boss-less → transient); `r1_1` now sets the flag
  explicitly. The flag is parsed/serialised and preserved by the editor.
- **2026-06-26** — Added a **non-boss arena** to `r1_1` (three patrollers) and made
  enemy arenas **respawn on bench rest**: an arena's foes now stay cleared only until
  the next bench (a transient `ClearedArenas` set, wiped on rest), while beaten **bosses
  persist**. (Previously enemy arenas re-armed on every entry.)
- **2026-06-26** — Added **boss types** ([`BOSS_KINDS`](src/boss.rs)): a `fog_wall` boss
  entry's `kind` now picks one (`0` original, `1` a **red, double-health** brute). The
  original boss moved to `r0_1` and the red brute holds `r3_2`. Bosses are now cleared
  **per room** (the save tracks a set of cleared rooms), so beating one doesn't clear
  the other.
- **2026-06-26** — Reworked arenas to **entry-triggered**, defined by a room's
  **`fog_wall`** list (`(boss, kind, col, row)` combatants) with **`F`** glyphs drawing
  the mist. The listed foes spawn only when you **enter** the room (no interaction);
  entering seals the exits until they're all dead — replacing the press-`E` solid wall.
  `MapData` gained the `fog_wall` field (parsed/serialised, preserved by the editor);
  the `Z` boss glyph is gone (the boss is a `fog_wall` entry now). `r3_2` carries a boss
  + one enemy.
- **2026-06-26** — Added a **boss fight** in the far corner room (`r3_2`, Story mode).
  A **fog gate** (interact with `E`) seals the arena — exits **lock** until the boss or
  the player dies. The boss (big `boss.png` sprite, ~28 HP on a top-of-screen bar)
  cycles three attacks — **slam** leap, **fan of throwables**, and **summoning** mobs —
  and enrages past half health. Beating it grants a big energy reward and unlocks the
  **double jump** ability (a mid-air second jump); both persist in the save, so the
  boss stays dead. New [`boss`](src/boss.rs) module + the `Abilities` resource in
  [`player`](src/player.rs).
- **2026-06-26** — Made release builds **self-contained**: a [`build.rs`](build.rs)
  bakes the **Story levels** (`assets/maps/`) and **all sprites** (`assets/sprites/`)
  into the binary via `include_str!`/`include_bytes!`. The runtime decodes sprites and
  parses the Story campaign straight from the embedded bytes (no `assets/` folder
  needed to run); new Builder saves seed their editable copy from the embedded levels
  too. `LevelRoot` is now `Story` (embedded) or `Builder(dir)` (on disk). A test
  asserts every shipped room parses and every sprite decodes from the embed.
- **2026-06-26** — Reset the shipped **Story campaign** (`assets/maps/`) to the clean
  **default 12-room world** — the same one the builder's Reset generates — as a fresh
  baseline to design on. Regenerate it any time with
  `cargo test reset_story_to_default -- --ignored`.
- **2026-06-26** — Added **Story / Builder game modes** and opened the level builder to
  every player. New Game now asks which mode; **Story** plays the read-only shipped
  `assets/maps/` campaign, while **Builder** seeds a **private editable copy** under
  `saves/builder<slot>/maps/` that you can edit in-game (pause → **Edit Levels**, or
  `F2`). Save **slots went 3 → 10**, each tagged `[Story]`/`[Builder]` in the picker.
  Under the hood, rooms are now loaded straight from the active save's directory
  (`world::LevelRoot`) on entering a game, instead of only the shipped folder at
  startup — so each save edits its own world and the campaign stays pristine.
- **2026-06-26** — **Letterboxed** the view so non-16:9 windows no longer **stretch**
  the picture: a `letterbox` system confines the camera's render viewport to the
  largest centred 16:9 rectangle that fits the window, with bars filling the rest. The
  fitting maths is a small pure function (`letterbox_rect`) with unit tests.
- **2026-06-26** — Fixed the camera **spilling outside the room when the window is
  resized**. The default 2D projection scales its visible world area with the window
  (`ScalingMode::WindowSize`), but the room-fit zoom, the edge clamping, and the HUD
  anchors all assume a fixed 960×540 viewport — so a bigger window revealed area
  outside the room and skipped the small-room zoom. The camera is now locked to a
  **fixed 960×540 logical viewport** (`ScalingMode::Fixed`); resizing scales that
  canvas instead.
- **2026-06-26** — The **pause menu** gained a **Character** entry that opens a
  read-only **status sheet** sub-screen (stats, energy, any pending bloodstain) with a
  *Back* row. It reuses the same source lines as the `C` overlay via a shared
  `stats::character_lines` helper, so the two never drift.
- **2026-06-26** — Replaced the row of heart pips with a single **continuous health
  bar** whose fill **shades green → yellow → red** as health drops (hue mapped to the
  fraction). It scales cleanly with the higher maximums Vitality unlocks, where a
  growing row of icons didn't.
- **2026-06-26** — Moved the **shop to benches**. Interacting with a bench (`E`) now
  opens a **bench menu** — **Rest** (save / restore / respawn), **Upgrade** a stat with
  energy, or **Leave** — instead of resting immediately. The `C` character screen is
  now a **read-only** stat sheet (it points you to a bench to upgrade). One overlay
  backs both via an [`OverlayMode`](src/stats.rs). Opening the world map is now also
  blocked while either overlay is up.
- **2026-06-26** — Added **character stats, upgrades, and a souls-like death loop**.
  Three Dark-Souls-flavoured stats — **Vitality** (hearts), **Strength** (sword
  damage), **Poise** (shorter stagger) — each level from 1 up. A new **character
  screen** (`C` / gamepad left bumper) shows them and doubles as the **shop**: spend
  **energy** to raise a stat, with each level **costing more** than the last. Energy is
  now banked into the save at rest/upgrade points; **dying drops all carried energy as
  a bloodstain** where you fell — reclaim it by walking back, or lose it for good if
  you die again first. New [`stats`](src/stats.rs) module; the [`save`](src/save.rs)
  format gained energy, stat levels, and the pending bloodstain (older saves load with
  base stats). The heart HUD now grows with Vitality.
- **2026-06-26** — New **flying** enemy (`flyer.png`, a winged flapper) that ignores
  gravity, in two tinted variants: a cyan **drifter** (`Drift`) that cruises and bobs,
  bouncing off walls and floors, and a magenta **stalker** (`Hunt`) that homes straight
  in on the player once within aggro range and drifts otherwise. Place them on any `E`
  cell — including mid-air ones — since they don't fall; two patrol the starting room's
  upper space.
- **2026-06-26** — New **leaper** enemy (`jumper.png`, a crouch→stretch hopper) with a
  `Pounce` AI: it ambles until you're in aggro range, then leaps toward your head, and
  **waits out a cooldown after each jump** so it can't spam pounces (it only travels
  horizontally mid-air, telegraphing on the ground). Ships as two tinted variants —
  green (kind 2) and a beefier amber (kind 3, *more health*, a stronger/farther leap,
  slightly longer recovery). Both appear in the starting room.
- **2026-06-26** — Sword hits now register reliably: the strike's hitbox stays **live
  for a short window** (`SWING_ACTIVE`) and is re-checked every frame, hitting each
  enemy at most once per swing. Previously the box was tested on the single press
  frame, so moving enemies could slip through and a swing felt like it whiffed. Enemy
  HP is unchanged (the 3-HP purple still dies to one full 3-hit combo) and tunable via
  `combat::ENEMY_KINDS[*].health`.
- **2026-06-26** — Enemy **kinds** now fully encapsulate behaviour and looks: each
  carries its **AI** (patrol, or **chase** within an aggro range — the red one
  hunts you) and its **sprite sheet + animation** (gridded and played via `anim`),
  alongside stats and colour. Adding a type is one data entry, no new code or glyph.
- **2026-06-26** — Enemy types are now **data-driven**: one `E` glyph plus an
  `enemies: [(kind, col, row)]` array (kind indexes `combat::ENEMY_KINDS`), so any
  number of types fit without per-type glyphs. An `E` with no entry uses kind 0. The
  shared sprite is tinted per kind (the red variant is now kind 1, not `F`).
- **2026-06-26** — Combat feel pass: taking damage now **knocks the player back**
  (with a brief stun) instead of teleporting — pits still respawn you at the room
  entry. The sword's hitbox is **much more generous** (a wide arc), and there's a
  second, **red** enemy (`F`) that dies in two hits.
- **2026-06-26** — Added **combat**: patrolling **enemies** (`E` glyph; turn at
  walls/ledges, hurt on contact), a **sword** (`J`) with a **3-hit combo**, and
  **energy** orbs that enemies drop and the player banks (HUD counter). Enemies
  respawn when the room reloads — including resting at a bench, which now reloads
  the room.
- **2026-06-26** — Added a **Main Menu** option to the pause menu, and **New Game**
  now lets you **type a name** for the save (shown in the slot picker).
- **2026-06-26** — New Game now **confirms before overwriting** an occupied save
  slot (an "OVERWRITE SAVE?" prompt defaulting to Back); empty slots start
  immediately as before.
- **2026-06-25** — Fixed a teleport loop: taking damage after using a portal could
  respawn the player onto the destination pad and immediately fire it back to the
  origin. Damage now disarms teleporters until the player steps clear.
- **2026-06-25** — Redrew the sprites with **side-on** orientation and smoother,
  more-frame animations: the player is a side-profile character (faces the walk
  direction) with idle/**walk**/jump/damage; the portal is an upright vortex; and
  **benches** are now a drawn wooden seat with drifting **fairy lights**.
- **2026-06-25** — Made animation **extensible** (a generic `SpriteAnimation` +
  clip/atlas core in [`anim`](src/anim.rs)) and gave **portals** a sprite sheet
  (`portal.png`, 4×2): an idle waving halo that flares **active** while the player
  stands on it. New animated entities just need a sheet, a `SpriteAnimation`, and a
  tiny controller.
- **2026-06-25** — The jump animation no longer loops: it plays its frames **once
  across the jump arc** (launch → apex → fall, held at the end), mapped to vertical
  velocity so it follows the actual jump rather than a fixed cadence.
- **2026-06-25** — The player is now a **sprite sheet** (`player.png`, a 4×3 grid)
  with a small animation system ([`anim`](src/anim.rs)): it imports an N×M grid into
  a texture atlas and plays **idle** (with a blink), **jump**, and **damage** clips
  by player state. Swap in a finer sheet later by redrawing it (or adjusting the
  grid/clip constants).
- **2026-06-25** — Benches now require an **interact press** (`E` / gamepad `Y`)
  to rest, instead of triggering when you walk over them; a `[E] rest` prompt shows
  while you're standing on one.
- **2026-06-25** — Replaced non-ASCII glyphs (`·`, `—`, `×`, `↔`, `−`) in on-screen
  text with ASCII, since Bevy's default font doesn't include them — menu, HUD, and
  builder labels now render fully.

- **2026-06-25** — Added **hearts, benches, and a three-slot save system**. The
  player has 3 hearts (HUD top-left); hazards/pits cost a heart with brief i-frames,
  and losing all three respawns you at the last bench. **Benches** (`B` glyph, a
  Bench brush in the builder) save the game, refill hearts, and reset enemies. The
  title screen's **New Game** / **Load Game** open a 3-slot picker; saves are RON
  files under `saves/`.
- **2026-06-25** — Portals are stored as pure coordinates — each pad's own cell
  (`origin_col`/`origin_row`) and its destination (`to`/`dest_col`/`dest_row`) —
  with **no grid glyph**, so they never use up tile characters. A room can hold many
  portals, and a portal can target **its own room** at another tile (self-portal);
  the builder authors both. (Existing maps were migrated.)
- **2026-06-25** — Teleporters no longer chain rapid teleports: a pad re-arms only
  once the player is ~1.5 tiles clear of every pad (a dead zone larger than the
  trigger), so you can't bounce back and forth or fire on the pad you arrive on.
- **2026-06-25** — The level builder can now **author portals**: a Portal brush
  drops the first endpoint, the room manager opens to pick the destination room,
  and painting there completes the two-way link (shared auto-assigned glyph, both
  rooms saved). `Esc` cancels before completion and leaves nothing behind.
- **2026-06-25** — Fixed the level builder's **room manager** (`M`): the tile and
  room views are now separate `EditorView` *states* instead of a plain resource,
  so switching no longer ran both input systems on the same frame and bounced the
  `M` toggle straight back (which left the tile view up and eating the arrow keys).
- **2026-06-25** — Added **teleporter pads**: a room can declare
  `teleports: [(glyph, to)]` to link to a distant room, stepping onto a pad warps
  the player to the partner room's pad (a shared glyph + mutual `to` makes it
  two-way). The demo links `r0_0` ↔ `r3_2`; pads show as cyan on the world map.
  The level builder preserves teleports through edits.
- **2026-06-25** — World map: now has three zoom levels — a scrollable 4×3
  **Window** of rooms (the new default, so the map no longer shrinks to fit as
  rooms are added), the full-**World** overview, and the single-**Room** detail
  view. Jump zooms in; `X` (gamepad `B`) zooms out.
- **2026-06-25** — World map: room-name labels are now light grey backed by a
  darker-grey silhouette (an outline of offset copies) instead of plain white, so
  they stay legible over bright room thumbnails.
