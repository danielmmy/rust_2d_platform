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
| World map | `M` | `Start` |
| Pause | `Esc` | `Select` |

The game opens on a **main menu** (Start / Quit); during play, **`Esc`** (or
`Select`) brings up a **pause menu** (Continue / Quit). Menus are navigated with
up/down and confirmed with jump / `Enter`. In **debug builds** both menus gain a
**Level Builder** entry (see below).

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

## Rooms, traversal, and danger

The world is a 4×3 grid of tall rooms (each larger than the screen, so the
**camera scrolls within a room and is bounded to it**). A central vertical shaft
with zig-zag ledges gives the climbing; **ceiling/floor gaps** are the up/down
doors and **side corridors** are the left/right doors. Hazards are sparse and
avoidable: **ground spikes** in dead-end corners and **falling rocks** in the
open. Touching one respawns you at the room's entry, instantly (Celeste-style).

## Extending it

The structure is plugin-per-concern:

| Module | Responsibility |
| --- | --- |
| [`input`](src/input.rs) | Keyboard + gamepad → one `PlayerIntent`. |
| [`physics`](src/physics.rs) | Hand-rolled AABB-vs-tile collision (unit-tested). |
| [`player`](src/player.rs) | Movement + jump feel; `MovementConfig`. |
| [`world`](src/world.rs) | Rooms, edge transitions, the 4-way neighbour graph. |
| [`ron`](src/ron.rs) | A tiny, self-contained RON reader for the map files. |
| [`hazards`](src/hazards.rs) | Spikes + falling rocks → instant respawn. |
| [`camera`](src/camera.rs) | Follow camera, bounded to the room; zooms in on small rooms. |
| [`worldmap`](src/worldmap.rs) | Pause-screen world map (`M`): overview + per-room zoom. |
| [`menu`](src/menu.rs) | Main menu + pause menu (`Esc`); `MainMenu`/`Paused` states. |
| [`editor`](src/editor.rs) | **Dev-only** level builder (`F2`): a tile view + a room-manager map. |

The crate's **only dependency is `bevy`** — the maps are `.map.ron` files read by
our own [`ron`](src/ron.rs) parser (a small `AssetLoader` in [`world`](src/world.rs)
plugs it into Bevy's asset pipeline), so there's no `serde`/`ron` crate to pull in.

### Add or edit a room

Rooms are ASCII grids in `assets/maps/<name>.map.ron`. There are no portals — a
room just names the neighbour on each side (empty = a wall / bottomless edge):

```ron
(
    name:   "Forest Glade",      // display name (empty → shows the file key)
    solid:  "#",                 // solid tiles
    spikes: "^",                 // deadly ground spikes
    rocks:  "R",                 // falling-rock spawners
    north:  "r0_1",              // room reached off the top edge   (empty = none)
    south:  "",                  // …bottom edge
    east:   "r1_0",              // …right edge
    west:   "",                  // …left edge
    bg:     [0.32, 0.16, 0.16],  // background colour [r, g, b] in 0..1
    tiles: [ "######", "#.@..#", "######" ],   // grid, top to bottom; `@` = start
)
```

Each room has an optional **display name** (e.g. "Forest Glade", "Meadow") shown
on the world map and in the builder; when empty it falls back to the file key.

Rooms are **discovered** from `assets/maps/` at startup, so just dropping a new
`.map.ron` adds it — no code change. Rooms are named `r{col}_{row}` (`r0_0`
bottom-left, the start); the grid is **unbounded** (columns/rows can be any
non-negative integer). Doors line up because rooms share the same size and
shaft/corridor positions.

When a room is **smaller than the screen**, the camera zooms in so the room fills
the viewport; larger rooms stay at 1:1 and scroll.

### Level builder (debug builds only)

In a dev build (`make game-run` / `cargo run`), open the **level builder** with
**`F2`** while playing — or pick **Level Builder** from the main or pause menu
(both show that entry only in debug builds). It has two views; saving writes the
`.map.ron` files and updates the running game, so leaving the builder shows your
edits.

**Tiles** — paint the selected room with the game's own sprites:

| Key | Action | Key | Action |
| --- | --- | --- | --- |
| arrows | move cursor | `[` / `]` | width − / + |
| `Space` | paint brush | `-` / `=` | height − / + |
| `X` | erase | `B` | recolour |
| `Tab` | cycle brush | `Enter` | rename (type a name) |
| `S` | save | `M` | room manager |
| `Esc` | leave the builder | | |

**Rooms** (`M`) — manage the world as a grid:

| Key | Action | Key | Action |
| --- | --- | --- | --- |
| arrows | move selection | `A` | add a room here |
| `Enter` | edit the room | `D` | delete the room |
| `G` | grab / drop (reorder) | `R` `R` | reset to the default 12 |
| `M` / `Esc` | back to tiles | | |

The room manager scrolls, so you can place **unlimited** rooms. There are **no
link controls**: connectivity is derived from the grid, so a room named
`r{col}_{row}` is linked to its existing N/S/E/W neighbours automatically, and
standard-size (40×22) rooms get their doors opened/sealed to match. Rooms can still
be **any size** in the tile view, but a custom-sized room manages its own doors.
The builder is `#[cfg(debug_assertions)]`, so it's compiled out of `--release`.

### Replace the art

Drop your own PNGs over the placeholders in `assets/sprites/`
(`player.png`, `tile.png`, `spikes.png`, `rock.png`). Sizes are set in code via
`custom_size`, so any resolution works — the world keeps the same scale.

## Status

Compiles against Bevy 0.19 (debug and release); the collision logic, the room
graph (every room parses and links to real rooms), the RON round-trip, and the
builder's default-room generator are unit-tested. The **feel and visuals are yours
to judge by running it** — they can't be verified headlessly, and the room layouts
are deliberately simple scaffolds to build on.

## Changelog

- **2026-06-25** — World map: now has three zoom levels — a scrollable 4×3
  **Window** of rooms (the new default, so the map no longer shrinks to fit as
  rooms are added), the full-**World** overview, and the single-**Room** detail
  view. Jump zooms in; `X` (gamepad `B`) zooms out.
- **2026-06-25** — World map: room-name labels are now light grey backed by a
  darker-grey silhouette (an outline of offset copies) instead of plain white, so
  they stay legible over bright room thumbnails.
