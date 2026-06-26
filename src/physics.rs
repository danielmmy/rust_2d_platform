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
}
