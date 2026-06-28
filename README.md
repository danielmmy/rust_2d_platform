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
| Jump / confirm | `Space`, `W`, `↑`, `Z`, or `Enter` | `A` (south) |
| Attack (sword) | `J` | `X` (west) |
| Interact / bench shop | `E` | `Y` (north) |
| Character screen (view stats) | `C` | left bumper |
| World map | `M` | `Start` |
| Pause | `Esc` | `Select` |

The game opens on a **main menu** (New Game / Load Game / **Options** / Quit). **New Game**
and **Load Game** open a **ten-slot** picker (each labelled with its `[Story]` or
`[Builder]` type) — pick a slot to load, or to start fresh. A new game asks you to
**choose a mode**, then (after confirming any overwrite) **type a name** for the save:

- **Story** plays the **shipped, read-only levels** — the designed campaign.
- **Builder** starts from a **private, editable copy** of those levels; you can paint,
  resize, add, delete, and relink rooms at will (see [Level builder](#level-builder)),
  and your edits stay in that save only.

**Options** (from the main menu or the pause menu) chooses the **window mode** —
**Windowed** or **Fullscreen (borderless)** — applied instantly and remembered across
launches (saved to `saves/settings.ron`).

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
levels** — press **jump** to zoom in and **`X`** (gamepad **`B`**) to zoom out:

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
facing, and presses chain a **3-hit combo** (the finisher flashes gold). A killed
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
| [`player`](src/player.rs) | Movement + jump feel; `MovementConfig`; rides moving platforms. |
| [`movers`](src/movers.rs) | Generic moving things — carries whatever the grid authors at a mover's cells along a path (loop/pingpong/once); solid ones are ridable. |
| [`scenery`](src/scenery.rs) | Parallax backdrops — looping, multi-speed layers per room from one of 12 themed sets. |
| [`anim`](src/anim.rs) | Extensible sprite-sheet animation: imports N×M grids; player / portal / bench / enemy / boss clips. |
| [`world`](src/world.rs) | Rooms, edge transitions, the 4-way neighbour graph, teleporters, benches; loads each save's world from its [`LevelRoot`]. |
| [`ron`](src/ron.rs) | A tiny, self-contained RON reader for the map files. |
| [`hazards`](src/hazards.rs) | Spikes + falling rocks → a `Hurt` on contact. |
| [`health`](src/health.rs) | Health (sized by Vitality), i-frames, the colour-graded health-bar HUD, death → bloodstain + last bench. |
| [`combat`](src/combat.rs) | Data-driven enemy kinds (stats/AI/animation), energy drops/pickup, bloodstain recovery, the 3-hit sword combo. |
| [`stats`](src/stats.rs) | Character stats (Vitality/Strength/Poise), the upgrade shop, and the character screen. |
| [`boss`](src/boss.rs) | The boss fight: fog-gate arena lock, attack patterns, projectiles, HUD, and the double-jump reward. |
| [`save`](src/save.rs) | Ten-slot save system (mode + room + bench + progression), RON files under `saves/`. |
| [`camera`](src/camera.rs) | Follow camera, bounded to the room; zooms in on small rooms. |
| [`worldmap`](src/worldmap.rs) | Pause-screen world map (`M`): overview + per-room zoom. |
| [`menu`](src/menu.rs) | Main menu (mode + slot picker) + pause menu (`Esc`) + Options (window mode); `MainMenu`/`Paused` states. |
| [`editor`](src/editor.rs) | Level builder (`F2` / pause **Edit Levels**, Builder saves): a tile view + a room-manager map. |

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
lifts); non-solid cells keep their own behaviour (a moving spike still hurts). The starter
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
| `P` | mover (moving tile) / delete | `M` | room manager |
| `Enter` | rename (type a name) | `Esc` | leave the builder |
| `Space` (Portal/Door brush) | start a portal / door link | | |

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
`orb.png`, `slash.png`). Sizes are set in code via `custom_size`, so any resolution
works — the world keeps the same scale. The enemy sheets (`enemy.png` walkers,
`jumper.png` leapers, `flyer.png` winged flyers) are **near-white** so each kind tints
them to its colour.

**`player.png`, `portal.png`, `bench.png`, and `boss.png` are sprite sheets** — each an
N×M grid of equal frames that [`anim`](src/anim.rs) imports into a texture atlas (sizing
every frame from the image ÷ grid, so a re-drawn sheet of the same grid just works):

- `player.png` is **6×4** — a **side-profile** character facing right (the
  [`player`](src/player.rs) flips it to face left, so it faces the way you walk).
  Rows: 0 = idle (last frame blinks), 1 = walk, 2 = jump, 3 = damage. Driven by
  state — damage → jump (airborne) → walk (moving) → idle; idle/walk/damage **loop**,
  the jump plays **once across the arc** (launch → apex → fall), keyed to velocity.
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

## Status

Compiles against Bevy 0.19 (debug and release); the collision logic, the room
graph (every room parses and links to real rooms), the RON round-trip, and the
builder's default-room generator are unit-tested. The **feel and visuals are yours
to judge by running it** — they can't be verified headlessly, and the room layouts
are deliberately simple scaffolds to build on.

## Changelog

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
