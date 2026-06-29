//! Kinematic AABB-vs-tile collision.
//!
//! A hand-rolled, per-axis resolver (rather than a physics engine) — platformers
//! live or die on precise, predictable collision, and this keeps full control.

use std::collections::HashSet;

use bevy::prelude::*;

/// World size of one tile, in pixels.
pub const TILE: f32 = 32.0;
const EPS: f32 = 0.01;

/// A straight **ramp** surface — the diagonal an inclined tile presents. `left`/`right` are its
/// world endpoints, ordered so `left.x < right.x`; the player rests on the line between them.
/// The angle is free (any `rise`/`run`): built per-room by [`crate::world`] from a
/// `(col, row, run, rise, dir)` spec, where the surface is the diagonal of that tile box.
#[derive(Clone, Copy)]
pub struct Ramp {
    pub left: Vec2,
    pub right: Vec2,
}

impl Ramp {
    /// Surface gradient (Δy per Δx); positive rises to the right.
    fn grad(self) -> f32 {
        (self.right.y - self.left.y) / (self.right.x - self.left.x)
    }
    /// Surface height at world-x `x` (assumes `left.x <= x <= right.x`).
    fn surface(self, x: f32) -> f32 {
        self.left.y + (x - self.left.x) * self.grad()
    }
}

/// The room's ramp surfaces. Kept **out of** [`Solids`] so the square resolver never treats a
/// ramp as a full block; instead the player ground-snaps onto them (see [`slope_ground`]).
#[derive(Resource, Default)]
pub struct Slopes(pub Vec<Ramp>);

/// How far above a ramp surface the feet may be and still snap down to it, so walking/sliding
/// **down** a ramp sticks instead of launching off each step.
const SLOPE_STICK: f32 = 14.0;

/// Ramp surface height under world-x `x` (for the ramp whose surface is nearest `near_y`), plus
/// its gradient (Δy/Δx — its sign says which way it descends). `None` if no ramp spans `x`.
pub fn slope_surface_at(slopes: &Slopes, x: f32, near_y: f32) -> Option<(f32, f32)> {
    let mut best: Option<(f32, f32)> = None;
    for r in &slopes.0 {
        if x < r.left.x || x > r.right.x {
            continue;
        }
        let surf = r.surface(x);
        if best.is_none_or(|(b, _)| (surf - near_y).abs() < (b - near_y).abs()) {
            best = Some((surf, r.grad()));
        }
    }
    best
}

/// Largest below-surface penetration that snaps the feet up **while grounded** — enough for the
/// per-frame rise of walking/sliding up a ramp, but well under a tile, so standing on a floor a
/// full tile below a ramp's high end never teleports you up onto it.
const SLOPE_CLIMB: f32 = 16.0;

/// If the player should rest on a ramp, the new centre `y` (feet on the surface). When grounded,
/// snaps for a small upward penetration ([`SLOPE_CLIMB`], walking up) or within [`SLOPE_STICK`]
/// above (walking down). When airborne, catches a deeper landing from above but needs the feet
/// to actually reach the surface. `None` off any ramp, or airborne above one.
pub fn slope_ground(slopes: &Slopes, center: Vec2, half: Vec2, was_grounded: bool) -> Option<f32> {
    let feet = center.y - half.y;
    let (surf, _) = slope_surface_at(slopes, center.x, feet)?;
    let (climb, stick) = if was_grounded {
        (SLOPE_CLIMB, SLOPE_STICK)
    } else {
        (TILE, 0.0)
    };
    (feet >= surf - climb && feet <= surf + stick).then_some(surf + half.y)
}

/// The set of solid tile cells `(col, row)` for the current map. `row` counts
/// up from the bottom, so a cell occupies `[col*TILE, (col+1)*TILE] ×
/// [row*TILE, (row+1)*TILE]` in world space (y up).
#[derive(Resource, Default)]
pub struct Solids(pub HashSet<(i32, i32)>);

impl Solids {
    fn is_solid(&self, col: i32, row: i32) -> bool {
        self.0.contains(&(col, row))
    }

    /// Whether the tile containing world point `(x, y)` is solid (used by enemy AI
    /// for ledge detection).
    pub(crate) fn solid_at(&self, x: f32, y: f32) -> bool {
        self.is_solid(to_tile(x), to_tile(y))
    }
}

fn to_tile(p: f32) -> i32 {
    (p / TILE).floor() as i32
}

/// One moving-platform tile this frame: its world centre + half-extents, and how far
/// the platform moved (`delta`), so a rider standing on it can be carried along.
#[derive(Clone, Copy)]
pub struct PlatformBox {
    pub center: Vec2,
    pub half: Vec2,
    pub delta: Vec2,
}

/// Every moving-platform tile box for the current room, rebuilt each frame by
/// [`crate::movers`]. The player resolves against these *after* the static grid.
#[derive(Resource, Default)]
pub struct Platforms(pub Vec<PlatformBox>);

/// How close the feet must be to a platform's top to count as "standing on it".
const CARRY_EPS: f32 = 4.0;

fn overlaps(center: Vec2, half: Vec2, b: &PlatformBox) -> bool {
    (center.x - b.center.x).abs() < half.x + b.half.x
        && (center.y - b.center.y).abs() < half.y + b.half.y
}

/// If the AABB is resting on top of a platform (its feet within [`CARRY_EPS`] of the
/// platform's *pre-move* top, horizontally over it), return that platform's `delta` —
/// the rider should be carried by it. Checks the pre-move position (`center - delta`).
pub fn carry_delta(platforms: &Platforms, center: Vec2, half: Vec2) -> Option<Vec2> {
    for b in &platforms.0 {
        let prev = b.center - b.delta; // where the tile was last frame
        let over_x = (center.x - prev.x).abs() < half.x + b.half.x;
        let feet = center.y - half.y;
        let top = prev.y + b.half.y;
        if over_x && (feet - top).abs() <= CARRY_EPS {
            return Some(b.delta);
        }
    }
    None
}

/// Resolve the AABB against platforms on the X axis, by **position** (push out the side the
/// player is on — so a platform moving into a standing-still player shoves them, not just one
/// they walk into). Only side hits act; a vertical overlap (rider on top, or hitting from
/// below) is left to the Y pass. If the push is into a static wall *and the platform is the
/// one driving the player that way*, the player is pinned — a **horizontal crush**, returned
/// as a squish source (the caller hurts them) rather than clipped into the wall. Pressing
/// against a still (or receding) platform next to a wall just blocks. Returns
/// `(blocked, squish_source)`.
pub fn resolve_platforms_x(
    solids: &Solids,
    platforms: &Platforms,
    center: &mut Vec2,
    half: Vec2,
) -> (bool, Option<Vec2>) {
    let mut blocked = false;
    let mut squish = None;
    for b in &platforms.0 {
        if !overlaps(*center, half, b) {
            continue;
        }
        // A rider resting on (or within CARRY_EPS of) the platform's top isn't hitting its
        // side — skipping it keeps a walking rider (including one near the platform's edge)
        // from being ejected sideways.
        if center.y - half.y >= b.center.y + b.half.y - CARRY_EPS {
            continue;
        }
        // Horizontal crush: a moving platform whose **leading face** has driven *into* the
        // player's body, shoving them toward a wall with no room to go. Hurt them — regardless
        // of how the overlap splits between axes, so even a head-height platform crushes a
        // standing player. Ducking clears it: the shorter box no longer overlaps, so the loop
        // skips this platform entirely.
        let drive = b.delta.x.signum();
        if drive != 0.0 {
            let lead = b.center.x + drive * b.half.x; // face leading the platform's motion
            let pushed_to = b.center.x + drive * (b.half.x + half.x);
            if lead > center.x - half.x
                && lead < center.x + half.x
                && aabb_in_solid(solids, pushed_to, center.y, half)
            {
                squish = Some(b.center);
                continue;
            }
        }
        // Otherwise push out the side — but only for a genuine side hit (more embedded
        // horizontally than vertically); a vertical overlap (from below/above) is the Y pass's
        // job, so a player below a platform isn't flung out to its edge.
        let x_pen = (half.x + b.half.x) - (center.x - b.center.x).abs();
        let y_pen = (half.y + b.half.y) - (center.y - b.center.y).abs();
        if y_pen <= x_pen {
            continue;
        }
        let push = (center.x - b.center.x).signum(); // shove out the side the player is on
        let target = b.center.x + push * (b.half.x + half.x);
        if !aabb_in_solid(solids, target, center.y, half) {
            center.x = target;
        }
        blocked = true; // walled (pressed into it) just blocks; clear push moves and blocks too
    }
    (blocked, squish)
}

/// Whether there's a wall (static tile or platform) immediately to `dir` (-1 left,
/// +1 right) of the AABB — used for wall slide / wall jump. Samples low/mid/high so a
/// short ledge still counts.
pub fn wall_at(solids: &Solids, platforms: &Platforms, center: Vec2, half: Vec2, dir: f32) -> bool {
    let x = center.x + dir * (half.x + 2.0);
    for off in [-half.y + EPS, 0.0, half.y - EPS] {
        if solids.solid_at(x, center.y + off) {
            return true;
        }
    }
    platforms.0.iter().any(|b| {
        (x - b.center.x).abs() <= b.half.x && (center.y - b.center.y).abs() < half.y + b.half.y
    })
}

/// Resolve the AABB against platforms on the Y axis, by **position** (not travel direction):
/// a player above a platform lands on its top; one below is pushed down (head-bump). Only
/// vertical hits are handled — a side hit (embedded more horizontally) is left to the X pass.
/// If pushing a player down is blocked by a static solid (they're floored under a platform
/// pressing down), that's a **vertical crush**: returned as a squish source (the caller hurts
/// them) and *not* resolved — so a descending platform never seats the player up onto itself
/// or shoves them sideways. Returns `(blocked, landed_on_top, squish_source)`.
pub fn resolve_platforms_y(
    solids: &Solids,
    platforms: &Platforms,
    center: &mut Vec2,
    half: Vec2,
) -> (bool, bool, Option<Vec2>) {
    let mut blocked = false;
    let mut landed = false;
    let mut squish = None;
    for b in &platforms.0 {
        if !overlaps(*center, half, b) {
            continue;
        }
        // Vertical hits only — a side hit (more embedded horizontally) is the X pass's job.
        let x_pen = (half.x + b.half.x) - (center.x - b.center.x).abs();
        let y_pen = (half.y + b.half.y) - (center.y - b.center.y).abs();
        if x_pen < y_pen {
            continue;
        }
        if center.y >= b.center.y {
            center.y = b.center.y + b.half.y + half.y; // above it → land on top
            landed = true;
            blocked = true;
        } else {
            let target = b.center.y - b.half.y - half.y; // below it → push down (head-bump)
            if !aabb_in_solid(solids, center.x, target, half) {
                center.y = target;
                blocked = true;
            } else if b.delta.y < 0.0 && (center.x - b.center.x).abs() < b.half.x {
                // Nowhere to go *and* a platform is actively pressing **down** from directly
                // overhead (descending, you under its span) → a real **crush**: hurt.
                squish = Some(b.center);
            } else {
                // Bonked the underside with no room below (a too-small gap, or a side graze, or
                // a platform just moving past) — block the rise, but don't hurt and don't clip in.
                blocked = true;
            }
        }
    }
    (blocked, landed, squish)
}

/// Whether an AABB at `center` overlaps any solid tile or moving platform. The `EPS` insets
/// mean a box resting flush on the floor (or flush against a wall) doesn't count — only a real
/// interior overlap does. Used to keep a crouched player from standing up into a low ceiling.
pub fn blocked(solids: &Solids, platforms: &Platforms, center: Vec2, half: Vec2) -> bool {
    aabb_in_solid(solids, center.x, center.y, half)
        || platforms.0.iter().any(|b| overlaps(center, half, b))
}

/// Whether a **descending** platform (`delta.y < 0`) is pressing down into the player's box
/// from directly overhead — the player's centre is under its span and its underside has bitten
/// into the body. Used to **force a crouch** (duck under it) rather than squish, as long as the
/// crouched box still fits; if it doesn't, [`resolve_platforms_y`] reports the crush. This is
/// deliberately *not* "anything overlapping the standing box": a platform you ride **up**, or
/// one you press against the **side** of, must not force a crouch.
pub fn ducking_under(platforms: &Platforms, center: Vec2, half: Vec2) -> bool {
    let (feet, head) = (center.y - half.y, center.y + half.y);
    platforms.0.iter().any(|b| {
        b.delta.y < 0.0
            && (center.x - b.center.x).abs() < b.half.x // player is under its span
            && {
                let underside = b.center.y - b.half.y;
                underside > feet && underside < head // pressing into the body from above
            }
    })
}

/// Whether an AABB centred at `(x, center.y)` would overlap a static solid (used to avoid
/// shoving a squished player straight into a wall).
fn aabb_in_solid(solids: &Solids, x: f32, y: f32, half: Vec2) -> bool {
    let row0 = to_tile(y - half.y + EPS);
    let row1 = to_tile(y + half.y - EPS);
    let col0 = to_tile(x - half.x + EPS);
    let col1 = to_tile(x + half.x - EPS);
    (row0..=row1).any(|row| (col0..=col1).any(|col| solids.is_solid(col, row)))
}

/// Move an AABB (centre + half-extents) along X by `dx`, stopping at solids.
/// Returns whether it was blocked.
pub fn collide_x(solids: &Solids, center: &mut Vec2, half: Vec2, dx: f32) -> bool {
    center.x += dx;
    if dx == 0.0 {
        return false;
    }
    let row0 = to_tile(center.y - half.y + EPS);
    let row1 = to_tile(center.y + half.y - EPS);
    if dx > 0.0 {
        let col = to_tile(center.x + half.x - EPS);
        for row in row0..=row1 {
            if solids.is_solid(col, row) {
                center.x = col as f32 * TILE - half.x;
                return true;
            }
        }
    } else {
        let col = to_tile(center.x - half.x + EPS);
        for row in row0..=row1 {
            if solids.is_solid(col, row) {
                center.x = (col + 1) as f32 * TILE + half.x;
                return true;
            }
        }
    }
    false
}

/// Move along Y by `dy`. Returns `(blocked, landed_on_ground)`.
pub fn collide_y(solids: &Solids, center: &mut Vec2, half: Vec2, dy: f32) -> (bool, bool) {
    center.y += dy;
    if dy == 0.0 {
        return (false, false);
    }
    let col0 = to_tile(center.x - half.x + EPS);
    let col1 = to_tile(center.x + half.x - EPS);
    if dy < 0.0 {
        let row = to_tile(center.y - half.y + EPS);
        for col in col0..=col1 {
            if solids.is_solid(col, row) {
                center.y = (row + 1) as f32 * TILE + half.y;
                return (true, true);
            }
        }
    } else {
        let row = to_tile(center.y + half.y - EPS);
        for col in col0..=col1 {
            if solids.is_solid(col, row) {
                center.y = row as f32 * TILE - half.y;
                return (true, false);
            }
        }
    }
    (false, false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solids(cells: &[(i32, i32)]) -> Solids {
        Solids(cells.iter().copied().collect())
    }

    #[test]
    fn lands_on_a_floor_tile() {
        // Floor at row 0 (y in [0, 32]); player half-height 19 falls onto it.
        let solids = solids(&[(0, 0)]);
        let mut center = Vec2::new(16.0, 60.0);
        let (blocked, landed) = collide_y(&solids, &mut center, Vec2::new(11.0, 19.0), -20.0);
        assert!(blocked && landed);
        assert!(
            (center.y - (TILE + 19.0)).abs() < 0.1,
            "rest on top of the tile"
        );
    }

    #[test]
    fn stops_at_a_wall() {
        // Wall at col 1 (x in [32, 64]); moving right is blocked.
        let solids = solids(&[(1, 1)]);
        let mut center = Vec2::new(28.0, 48.0);
        let blocked = collide_x(&solids, &mut center, Vec2::new(11.0, 19.0), 20.0);
        assert!(blocked);
        assert!(
            (center.x - (TILE - 11.0)).abs() < 0.1,
            "stop at the wall face"
        );
    }

    #[test]
    fn passes_through_open_space() {
        let solids = solids(&[]);
        let mut center = Vec2::new(16.0, 48.0);
        assert!(!collide_x(
            &solids,
            &mut center,
            Vec2::new(11.0, 19.0),
            10.0
        ));
        assert_eq!(center.x, 26.0);
    }

    #[test]
    fn rider_on_platform_is_not_ejected_sideways() {
        // Platform tile [0,32]×[0,32]; player (half 11×19) riding on top, walking right.
        let platforms = Platforms(vec![PlatformBox {
            center: Vec2::new(16.0, 16.0),
            half: Vec2::splat(16.0),
            delta: Vec2::ZERO,
        }]);
        // Feet a hair inside the top (y 31.5, just under the box top at 32) — the case that
        // used to teleport the player to the platform's edge.
        let mut center = Vec2::new(16.0, 50.5);
        let open = solids(&[]);
        let (blocked, _) =
            resolve_platforms_x(&open, &platforms, &mut center, Vec2::new(11.0, 19.0));
        assert!(!blocked, "a rider on top must not be pushed sideways");
        assert_eq!(center.x, 16.0);
    }

    #[test]
    fn lands_on_a_platform_top_from_above() {
        // Platform [0,32]×[0,32]; player overlapping it from above (centre above the box centre)
        // → seated on top, grounded; centre.x untouched.
        let platforms = Platforms(vec![PlatformBox {
            center: Vec2::new(16.0, 16.0),
            half: Vec2::splat(16.0),
            delta: Vec2::ZERO,
        }]);
        let open = solids(&[]);
        let mut center = Vec2::new(16.0, 48.0); // a touch low; feet 29 < top 32
        let (_, landed, squish) =
            resolve_platforms_y(&open, &platforms, &mut center, Vec2::new(11.0, 19.0));
        assert!(landed && squish.is_none());
        assert!((center.y - (32.0 + 19.0)).abs() < 0.1, "seated on the top");
    }

    #[test]
    fn jumping_into_a_platform_from_below_is_not_flung_sideways() {
        // Wide platform [0,96]×[32,64]; player centred under it, head just poking its underside.
        // The X pass must not eject them to the platform's edge (the jump-teleport bug); the Y
        // pass pushes them straight down instead.
        let platforms = Platforms(vec![PlatformBox {
            center: Vec2::new(48.0, 48.0),
            half: Vec2::new(48.0, 16.0),
            delta: Vec2::ZERO,
        }]);
        let half = Vec2::new(11.0, 19.0);
        let open = solids(&[]);
        let mut center = Vec2::new(48.0, 15.0); // head 34 pokes the underside (32)
        let (blocked, _) = resolve_platforms_x(&open, &platforms, &mut center, half);
        assert!(!blocked, "a player below a platform is not side-pushed");
        assert_eq!(
            center.x, 48.0,
            "centre.x unchanged — no teleport to the edge"
        );

        let (_, _, squish) = resolve_platforms_y(&open, &platforms, &mut center, half);
        assert!(squish.is_none() && center.y < 15.0, "pushed straight down");
    }

    #[test]
    fn descending_platform_on_a_floored_player_is_a_crush() {
        // Platform [0,32] underside biting a player on the floor, with the floor right below →
        // pushing them down is blocked, so it's reported as a squish (the caller hurts them).
        let platforms = Platforms(vec![PlatformBox {
            center: Vec2::new(16.0, 36.0), // underside at 20
            half: Vec2::splat(16.0),
            delta: Vec2::new(0.0, -3.0),
        }]);
        let floor = solids(&[(0, 0)]); // [0,32]×[0,32]
        let mut center = Vec2::new(16.0, 19.0); // feet 0 on the floor, head 38
        let (_, _, squish) =
            resolve_platforms_y(&floor, &platforms, &mut center, Vec2::new(11.0, 19.0));
        assert!(
            squish.is_some(),
            "floored under a descending platform → crush"
        );
    }

    #[test]
    fn crouch_box_clears_a_one_tile_gap_a_stand_box_does_not() {
        // Floor at row 0; a ceiling tile at row 2 (y in [64,96]) leaves a one-tile gap (row 1).
        // Feet at y=32: a standing box (half-h 19, head 70) bites the ceiling; a crouch box
        // (half-h 12, head 56) clears it.
        let solids = solids(&[(0, 2)]);
        let empty = Platforms(vec![]);
        let stand_center = Vec2::new(16.0, 32.0 + 19.0);
        let crouch_center = Vec2::new(16.0, 32.0 + 12.0);
        assert!(blocked(
            &solids,
            &empty,
            stand_center,
            Vec2::new(11.0, 19.0)
        ));
        assert!(!blocked(
            &solids,
            &empty,
            crouch_center,
            Vec2::new(11.0, 12.0)
        ));
    }

    #[test]
    fn side_hit_on_platform_is_ejected() {
        // Platform tile [32,64]×[0,32]; player beside it (feet at y 0, well below the top),
        // walking right into its face — still gets pushed out.
        let platforms = Platforms(vec![PlatformBox {
            center: Vec2::new(48.0, 16.0),
            half: Vec2::splat(16.0),
            delta: Vec2::ZERO,
        }]);
        let open = solids(&[]);
        let mut center = Vec2::new(30.0, 19.0);
        let (blocked, _) =
            resolve_platforms_x(&open, &platforms, &mut center, Vec2::new(11.0, 19.0));
        assert!(blocked);
        assert!(
            (center.x - (48.0 - 16.0 - 11.0)).abs() < 0.1,
            "stop at the platform's left face"
        );
    }

    #[test]
    fn rider_near_the_edge_is_not_ejected() {
        // A wide moving platform [0,96]×[0,32]; the player rides hanging past its right edge
        // (centre beyond the span, feet on top), walking right. It must not be side-pushed —
        // only fall off naturally — so the rider-skip is feet-near-top alone, not "centre over
        // the span" (which used to eject edge riders).
        let platforms = Platforms(vec![PlatformBox {
            center: Vec2::new(48.0, 16.0),
            half: Vec2::new(48.0, 16.0),
            delta: Vec2::new(2.0, 0.0),
        }]);
        let open = solids(&[]);
        let mut center = Vec2::new(100.0, 49.0); // hanging past the right edge, feet ~ on top
        let (blocked, _) =
            resolve_platforms_x(&open, &platforms, &mut center, Vec2::new(11.0, 19.0));
        assert!(!blocked, "an edge rider must not be ejected sideways");
        assert_eq!(center.x, 100.0);
    }

    #[test]
    fn a_moving_platform_drives_you_into_a_wall_and_crushes() {
        // Player pinned against a left wall (col -1); a platform at body height moving **left**
        // into them, with nowhere to go (and standing, so no room to duck) → a horizontal crush.
        let platforms = Platforms(vec![PlatformBox {
            center: Vec2::new(35.0, 24.0),
            half: Vec2::splat(16.0),
            delta: Vec2::new(-3.0, 0.0),
        }]);
        let walls = solids(&[(-1, 0), (-1, 1)]);
        let half = Vec2::new(11.0, 19.0);
        let mut center = Vec2::new(11.0, 24.0); // left edge at 0, flush against the wall
        let (_, squish) = resolve_platforms_x(&walls, &platforms, &mut center, half);
        assert!(squish.is_some(), "driven into the wall → crush");
    }

    #[test]
    fn pressing_a_still_platform_by_a_wall_just_blocks() {
        // The same pin, but the platform isn't moving — you pressed into it. Block, no crush.
        let platforms = Platforms(vec![PlatformBox {
            center: Vec2::new(35.0, 24.0),
            half: Vec2::splat(16.0),
            delta: Vec2::ZERO,
        }]);
        let walls = solids(&[(-1, 0), (-1, 1)]);
        let half = Vec2::new(11.0, 19.0);
        let mut center = Vec2::new(11.0, 24.0);
        let (_, squish) = resolve_platforms_x(&walls, &platforms, &mut center, half);
        assert!(
            squish.is_none(),
            "a still platform doesn't crush — just blocks"
        );
    }

    #[test]
    fn ducking_under_only_for_a_descending_overhead_platform() {
        let half = Vec2::new(11.0, 19.0);
        let center = Vec2::new(16.0, 19.0); // feet 0, head 38, centre x 16

        // Descending platform overhead (underside at 30, inside the body) → duck.
        let descending = |dy: f32, cx: f32| {
            Platforms(vec![PlatformBox {
                center: Vec2::new(cx, 46.0), // half 16 → underside at 30
                half: Vec2::splat(16.0),
                delta: Vec2::new(0.0, dy),
            }])
        };
        assert!(ducking_under(&descending(-2.0, 16.0), center, half));
        // Rising (you're riding it up) → not a duck.
        assert!(!ducking_under(&descending(2.0, 16.0), center, half));
        // Descending but off to the side (centre not under it) → not a duck.
        assert!(!ducking_under(&descending(-2.0, 60.0), center, half));
    }

    #[test]
    fn ramp_surface_rises_along_its_run() {
        // A 2-wide, 1-tall ramp rising to the right: low on the left, high on the right.
        let s = Slopes(vec![Ramp {
            left: Vec2::new(0.0, 0.0),
            right: Vec2::new(64.0, 32.0),
        }]);
        let (lo, grad) = slope_surface_at(&s, 0.0, 0.0).unwrap();
        let (mid, _) = slope_surface_at(&s, 32.0, 0.0).unwrap();
        let (hi, _) = slope_surface_at(&s, 64.0, 0.0).unwrap();
        assert!((lo - 0.0).abs() < 0.1 && (mid - 16.0).abs() < 0.1 && (hi - 32.0).abs() < 0.1);
        assert!(grad > 0.0, "rises to the right");
        assert!(slope_surface_at(&s, 80.0, 0.0).is_none(), "off the end");
    }

    #[test]
    fn slope_ground_snaps_feet_to_the_surface() {
        let s = Slopes(vec![Ramp {
            left: Vec2::new(0.0, 0.0),
            right: Vec2::new(32.0, 32.0), // 45°: surface y == x
        }]);
        let half = Vec2::new(11.0, 19.0);
        // Centre at x=16 (surface 16); feet a touch below → snap so feet seat at 16.
        let center = Vec2::new(16.0, 16.0 + 19.0 - 5.0);
        let y = slope_ground(&s, center, half, true).expect("on the ramp");
        assert!(
            (y - (16.0 + 19.0)).abs() < 0.1,
            "feet seated on the surface"
        );
        // Far above the ramp while airborne → no snap (still falling).
        assert!(slope_ground(&s, Vec2::new(16.0, 200.0), half, false).is_none());
        // Grounded on a floor a full tile below the surface → not teleported up onto the ramp.
        let on_floor_below = Vec2::new(30.0, (30.0 - 32.0) + 19.0); // surface 30, feet a tile under
        assert!(slope_ground(&s, on_floor_below, half, true).is_none());
    }

    #[test]
    fn ramp_gradient_sign_tracks_direction() {
        let right = Ramp {
            left: Vec2::new(0.0, 0.0),
            right: Vec2::new(32.0, 32.0),
        };
        let left = Ramp {
            left: Vec2::new(0.0, 32.0),
            right: Vec2::new(32.0, 0.0),
        };
        // Downhill when grad * travel-direction < 0.
        assert!(
            right.grad() > 0.0,
            "/ rises rightward (downhill going left)"
        );
        assert!(
            left.grad() < 0.0,
            "\\ rises leftward (downhill going right)"
        );
    }
}
