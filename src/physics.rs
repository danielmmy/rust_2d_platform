//! Kinematic AABB-vs-tile collision.
//!
//! A hand-rolled, per-axis resolver (rather than a physics engine) — platformers
//! live or die on precise, predictable collision, and this keeps full control.

use std::collections::HashSet;

use bevy::prelude::*;

/// World size of one tile, in pixels.
pub const TILE: f32 = 32.0;
const EPS: f32 = 0.01;

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

/// Push the AABB out of any platform it overlaps on the X axis (call after the static
/// pass; `center` is already moved). `dx` gives the travel direction. Returns whether hit.
pub fn resolve_platforms_x(platforms: &Platforms, center: &mut Vec2, half: Vec2, dx: f32) -> bool {
    if dx == 0.0 {
        return false;
    }
    let mut hit = false;
    for b in &platforms.0 {
        if !overlaps(*center, half, b) {
            continue;
        }
        // A rider resting on (or within CARRY_EPS of) the platform's top isn't hitting its
        // side. Skipping it stops a walking rider from being ejected sideways: carrying the
        // rider can leave its feet a hair inside the box top, and this X pass runs before the
        // Y pass that re-seats the feet, so the overlap would otherwise look like a wall hit.
        if center.y - half.y >= b.center.y + b.half.y - CARRY_EPS {
            continue;
        }
        center.x = if dx > 0.0 {
            b.center.x - b.half.x - half.x
        } else {
            b.center.x + b.half.x + half.x
        };
        hit = true;
    }
    hit
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

/// Push the AABB out of any platform it overlaps on the Y axis. Returns
/// `(blocked, landed_on_top)`.
pub fn resolve_platforms_y(
    platforms: &Platforms,
    center: &mut Vec2,
    half: Vec2,
    dy: f32,
) -> (bool, bool) {
    let mut blocked = false;
    let mut landed = false;
    for b in &platforms.0 {
        if dy != 0.0 && overlaps(*center, half, b) {
            if dy < 0.0 {
                center.y = b.center.y + b.half.y + half.y;
                landed = true;
            } else {
                center.y = b.center.y - b.half.y - half.y;
            }
            blocked = true;
        }
    }
    (blocked, landed)
}

/// Whether the AABB is supported from directly below — static ground just under the feet,
/// or a platform top within [`CARRY_EPS`]. Used to tell a *squish* (no room to escape down)
/// apart from a platform merely pressing a falling player downward.
pub fn supported_below(solids: &Solids, platforms: &Platforms, center: Vec2, half: Vec2) -> bool {
    let feet = center.y - half.y;
    let row = to_tile(feet - EPS);
    let col0 = to_tile(center.x - half.x + EPS);
    let col1 = to_tile(center.x + half.x - EPS);
    if (col0..=col1).any(|col| solids.is_solid(col, row)) {
        return true;
    }
    platforms.0.iter().any(|b| {
        (center.x - b.center.x).abs() < half.x + b.half.x
            && (feet - (b.center.y + b.half.y)).abs() <= CARRY_EPS
    })
}

/// Whether an AABB at `center` overlaps any solid tile or moving platform. The `EPS` insets
/// mean a box resting flush on the floor (or flush against a wall) doesn't count — only a real
/// interior overlap does. Used to keep a crouched player from standing up into a low ceiling.
pub fn blocked(solids: &Solids, platforms: &Platforms, center: Vec2, half: Vec2) -> bool {
    aabb_in_solid(solids, center.x, center.y, half)
        || platforms.0.iter().any(|b| overlaps(center, half, b))
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

/// Detect a **squish**: a *descending* platform (`delta.y < 0`) whose underside has pressed
/// down *into* the player's body (between the feet and head — i.e. not one the player is
/// riding on top of), overlapping in X. If so, return the x to shove the player to — out a
/// side of the whole squishing span — and that span's centre (a knockback source). The nearer
/// side is preferred, but a side walled off by static tiles is avoided where possible.
///
/// The caller should only act on this when the player is [`supported_below`]; otherwise a
/// platform landing on an airborne player would shove them sideways instead of letting them
/// be pushed down.
pub fn squish_push_x(
    solids: &Solids,
    platforms: &Platforms,
    center: Vec2,
    half: Vec2,
) -> Option<(f32, Vec2)> {
    let (feet, head) = (center.y - half.y, center.y + half.y);
    let (mut left, mut right) = (f32::INFINITY, f32::NEG_INFINITY);
    let mut found = false;
    for b in &platforms.0 {
        if b.delta.y >= 0.0 || !overlaps(center, half, b) {
            continue;
        }
        let p_bottom = b.center.y - b.half.y;
        if p_bottom <= feet || p_bottom >= head {
            continue; // resting on top (or no vertical bite) — not a squish
        }
        left = left.min(b.center.x - b.half.x);
        right = right.max(b.center.x + b.half.x);
        found = true;
    }
    if !found {
        return None;
    }
    let group_center = (left + right) * 0.5;
    let (out_left, out_right) = (left - half.x, right + half.x);
    let prefer_left = center.x < group_center;
    let left_ok = !aabb_in_solid(solids, out_left, center.y, half);
    let right_ok = !aabb_in_solid(solids, out_right, center.y, half);
    // Nearer side first; if it's walled and the other isn't, use the other. If both (or
    // neither) are clear, keep the nearer one.
    let new_x = match (prefer_left, left_ok, right_ok) {
        (true, false, true) => out_right,
        (false, true, false) => out_left,
        (true, _, _) => out_left,
        (false, _, _) => out_right,
    };
    Some((new_x, Vec2::new(group_center, center.y)))
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
        let hit = resolve_platforms_x(&platforms, &mut center, Vec2::new(11.0, 19.0), 5.0);
        assert!(!hit, "a rider on top must not be pushed sideways");
        assert_eq!(center.x, 16.0);
    }

    #[test]
    fn descending_platform_squishes_a_grounded_player() {
        // Platform tile centred at (16, 40) (underside at y 24) coming down; player on the
        // floor (feet 0, head 38), so the underside has bitten into the body → squish.
        let platforms = Platforms(vec![PlatformBox {
            center: Vec2::new(16.0, 40.0),
            half: Vec2::splat(16.0),
            delta: Vec2::new(0.0, -3.0),
        }]);
        let half = Vec2::new(11.0, 19.0);
        let open = solids(&[]);
        let (push_x, _src) =
            squish_push_x(&open, &platforms, Vec2::new(16.0, 19.0), half).expect("squish");
        // Player is at/!left-of the span centre → shoved out the right side (32 + 11).
        assert!((push_x - 43.0).abs() < 0.1);
    }

    #[test]
    fn rider_on_descending_platform_is_not_squished() {
        // Same descending platform, but the player rests on its top (feet just at the top) —
        // the underside is below the feet, so it must not register as a squish.
        let platforms = Platforms(vec![PlatformBox {
            center: Vec2::new(16.0, 16.0),
            half: Vec2::splat(16.0),
            delta: Vec2::new(0.0, -3.0),
        }]);
        let half = Vec2::new(11.0, 19.0);
        let open = solids(&[]);
        assert!(squish_push_x(&open, &platforms, Vec2::new(16.0, 50.5), half).is_none());
    }

    #[test]
    fn squish_pushes_away_from_a_wall() {
        // Same descending squish, but a wall sits just left of the span — the player (nearer
        // the left) must be shoved out the right instead of into the wall.
        let platforms = Platforms(vec![PlatformBox {
            center: Vec2::new(16.0, 40.0),
            half: Vec2::splat(16.0),
            delta: Vec2::new(0.0, -3.0),
        }]);
        let half = Vec2::new(11.0, 19.0);
        let walled = solids(&[(-1, 0), (-1, 1)]);
        let (push_x, _) =
            squish_push_x(&walled, &platforms, Vec2::new(8.0, 19.0), half).expect("squish");
        assert!(
            (push_x - 43.0).abs() < 0.1,
            "shoved right, away from the left wall"
        );
    }

    #[test]
    fn supported_below_sees_ground_not_air() {
        let solids = solids(&[(0, 0)]); // floor tile [0,32]×[0,32]
        let empty = Platforms(vec![]);
        let half = Vec2::new(11.0, 19.0);
        assert!(supported_below(
            &solids,
            &empty,
            Vec2::new(16.0, 51.0),
            half
        )); // feet on top
        assert!(!supported_below(
            &solids,
            &empty,
            Vec2::new(16.0, 59.0),
            half
        )); // floating
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
        let mut center = Vec2::new(30.0, 19.0);
        let hit = resolve_platforms_x(&platforms, &mut center, Vec2::new(11.0, 19.0), 5.0);
        assert!(hit);
        assert!(
            (center.x - (48.0 - 16.0 - 11.0)).abs() < 0.1,
            "stop at the platform's left face"
        );
    }
}
