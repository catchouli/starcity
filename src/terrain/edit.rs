//! Terraforming: moving terrain corners and rewriting the tiles around them.
//!
//! Terrain is described by integer heights at tile corners, where neighbouring
//! corners (diagonals included) differ by at most one block, and each tile's
//! surface follows from its four corners. Edits are planned on a [`Corners`]
//! overlay first, so a plan can be priced, checked and thrown away without
//! touching the world, then written with [`apply`].

use super::{Block, Material, Shape, World};
use std::collections::BTreeMap;

/// No corner may move further than this, and a plan may only move this many
/// corners in all: extreme terrain can't be flattened.
pub const MAX_SHIFT: i32 = 3;
pub const MAX_CORNERS: usize = 160;

/// Corner heights: the world's, with planned changes on top.
pub struct Corners<'a> {
    world: &'a World,
    changed: BTreeMap<(i32, i32), i32>,
    max_shift: i32,
    max_corners: usize,
    /// Corners that must not move (they belong to something built).
    frozen: std::collections::HashSet<(i32, i32)>,
}

impl<'a> Corners<'a> {
    pub fn new(world: &'a World) -> Corners<'a> {
        Corners::with_limits(world, MAX_SHIFT, MAX_CORNERS)
    }

    /// An overlay that may move corners further and in greater number, for
    /// long graded cuttings and embankments.
    pub fn with_limits(world: &'a World, max_shift: i32, max_corners: usize) -> Corners<'a> {
        Corners {
            world,
            changed: BTreeMap::new(),
            max_shift,
            max_corners,
            frozen: Default::default(),
        }
    }

    /// Forbids moving this corner.
    pub fn freeze(&mut self, c: (i32, i32)) {
        self.frozen.insert(c);
    }

    /// A corner's height in the world as it is now (ignoring planned changes).
    pub fn world_height(&self, c: (i32, i32)) -> Option<i32> {
        self.original(c)
    }

    /// A corner's height in the world as it is now.
    fn original(&self, (cx, cz): (i32, i32)) -> Option<i32> {
        // A corner belongs to up to four tiles; any of them will do.
        [(0, 0, 0, 0), (-1, 0, 1, 0), (-1, -1, 1, 1), (0, -1, 0, 1)]
            .iter()
            .find_map(|&(dx, dz, ox, oz)| {
                if !self.world.in_bounds(cx + dx, cz + dz) {
                    return None;
                }
                // Seabed at height 0 has no blocks at all.
                let (base, shape) = self.world.surface(cx + dx, cz + dz).unwrap_or((0, Shape(0)));
                Some(base as i32 + shape.corner(ox, oz) as i32)
            })
    }

    pub fn get(&self, c: (i32, i32)) -> Option<i32> {
        self.changed.get(&c).copied().or_else(|| self.original(c))
    }

    /// The four corner heights of a tile (NW, NE, SE, SW).
    pub fn tile(&self, x: i32, z: i32) -> Option<[i32; 4]> {
        let mut out = [0; 4];
        for (i, &(cx, cz)) in Shape::CORNERS.iter().enumerate() {
            out[i] = self.get((x + cx as i32, z + cz as i32))?;
        }
        Some(out)
    }

    /// A tile's surface (floor height and top shape) under the planned heights.
    pub fn surface(&self, x: i32, z: i32) -> Option<(u16, Shape)> {
        let moved = Shape::CORNERS
            .iter()
            .any(|&(cx, cz)| self.changed.contains_key(&(x + cx as i32, z + cz as i32)));
        if !moved {
            return self
                .world
                .surface(x, z)
                .or(Some((0, Shape(0))))
                .filter(|_| self.world.in_bounds(x, z));
        }
        let corners = self.tile(x, z)?;
        let base = *corners.iter().min().unwrap();
        let mask = (0..4).filter(|&i| corners[i] > base).fold(0u8, |m, i| m | 1 << i);
        Some((base.max(0) as u16, Shape(mask)))
    }

    /// Levels a tile to height `h` (in blocks), then moves surrounding corners
    /// just enough to keep every step to a neighbour at one block or less.
    /// Fails, leaving the overlay partly changed, if that takes too much.
    pub fn level(&mut self, x: i32, z: i32, h: i32) -> bool {
        let mut work = Vec::new();
        for &(cx, cz) in &Shape::CORNERS {
            let c = (x + cx as i32, z + cz as i32);
            if !self.set(c, h) {
                return false;
            }
            work.push(c);
        }
        while let Some(c) = work.pop() {
            let height = self.get(c).unwrap();
            for dz in -1..=1 {
                for dx in -1..=1 {
                    let n = (c.0 + dx, c.1 + dz);
                    let Some(cur) = self.get(n) else { continue };
                    let wanted = cur.clamp(height - 1, height + 1);
                    if wanted != cur {
                        if !self.set(n, wanted) {
                            return false;
                        }
                        work.push(n);
                    }
                }
            }
        }
        true
    }

    fn set(&mut self, c: (i32, i32), h: i32) -> bool {
        let Some(old) = self.original(c) else { return false };
        if (h != old && self.frozen.contains(&c))
            || (h - old).abs() > self.max_shift
            || h < 1
            || h as usize + 2 >= self.world.height
        {
            return false;
        }
        if h == old {
            self.changed.remove(&c);
        } else {
            self.changed.insert(c, h);
        }
        self.changed.len() <= self.max_corners
    }

    /// Corners that differ from the world, with their new heights.
    pub fn changes(&self) -> Vec<((i32, i32), u16)> {
        self.changed.iter().map(|(&c, &h)| (c, h as u16)).collect()
    }

    /// Total height moved, in blocks summed over corners.
    pub fn steps(&self) -> u32 {
        self.changed
            .iter()
            .map(|(&c, &h)| (h - self.original(c).unwrap()).unsigned_abs())
            .sum()
    }

    /// Tiles touching a changed corner.
    pub fn affected_tiles(&self) -> Vec<(i32, i32)> {
        let mut tiles: Vec<_> = self
            .changed
            .keys()
            .flat_map(|&(cx, cz)| [(cx - 1, cz - 1), (cx, cz - 1), (cx - 1, cz), (cx, cz)])
            .filter(|&(x, z)| self.world.in_bounds(x, z))
            .collect();
        tiles.sort();
        tiles.dedup();
        tiles
    }
}

/// Moves the given corners and rewrites every tile touching them. Returns
/// the tiles that were rewritten (to re-mesh).
pub fn apply(world: &mut World, corners: &[((i32, i32), u16)]) -> Vec<(i32, i32)> {
    let mut overlay = Corners::new(world);
    for &(c, h) in corners {
        overlay.changed.insert(c, h as i32);
    }
    let tiles = overlay.affected_tiles();
    let rewrites: Vec<_> = tiles
        .iter()
        .filter_map(|&(x, z)| Some((x, z, overlay.tile(x, z)?)))
        .collect();
    for (x, z, heights) in rewrites {
        write_tile(world, x as usize, z as usize, heights);
    }
    tiles
}

/// Rebuilds a column from its four corner heights, the way generation does.
fn write_tile(world: &mut World, x: usize, z: usize, corners: [i32; 4]) {
    let old = world.top_block(x as i32, z as i32).map(|(_, b)| b.material);
    let base = *corners.iter().min().unwrap();
    let mask = (0..4).filter(|&i| corners[i] > base).fold(0u8, |m, i| m | 1 << i);
    let surface = match old {
        _ if base <= world.sea_level as i32 => Material::Sand,
        Some(Material::Sand) | None => Material::Grass,
        Some(m) => m,
    };
    let base = base as usize;
    for y in 0..world.height {
        let block = if y < base {
            let material = if mask == 0 && y + 1 == base {
                surface
            } else if y + 4 >= base {
                Material::Dirt
            } else {
                Material::Rock
            };
            Block::full(material)
        } else if y == base && mask != 0 {
            Block {
                material: surface,
                shape: Shape(mask),
            }
        } else {
            Block::AIR
        };
        world.set(x, y, z, block);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::network::{Blocked, Layer};
    use crate::rails::{self, Signals};
    use crate::terrain::generation::{self, GenParams};

    /// Every corner is shared by its tiles and steps by at most one to its neighbours.
    fn assert_consistent(world: &World) {
        let n = world.size_x as i32;
        let corners = Corners::new(world);
        for z in 0..n {
            for x in 0..n {
                let c = corners.tile(x, z).unwrap();
                assert!(
                    c.iter().max().unwrap() - c.iter().min().unwrap() <= 1,
                    "tile ({x}, {z}) {c:?}"
                );
                if x + 1 < n {
                    let r = corners.tile(x + 1, z).unwrap();
                    assert_eq!(
                        (c[1], c[2]),
                        (r[0], r[3]),
                        "tiles ({x}, {z}) and ({}, {z}) disagree",
                        x + 1
                    );
                }
                if z + 1 < n {
                    let d = corners.tile(x, z + 1).unwrap();
                    assert_eq!(
                        (c[3], c[2]),
                        (d[0], d[1]),
                        "tiles ({x}, {z}) and ({x}, {}) disagree",
                        z + 1
                    );
                }
            }
        }
    }

    #[test]
    fn track_across_slopes_levels_the_ground() {
        let mut world = generation::generate(&GenParams::default());
        let (roads, rails, buildings, signals) = (
            Layer::new(512, 512),
            Layer::new(512, 512),
            Layer::new(512, 512),
            Signals::default(),
        );
        // Find hillside runs the plain planner refuses but levelling can do.
        let mut tried = 0;
        let mut levelled = 0;
        for z in (100..400).step_by(7) {
            for x in (100..380).step_by(13) {
                let (a, b) = ((x, z), (x + 10, z + 3 * (x % 2)));
                let b = if b.1 == a.1 { (b.0, a.1) } else { (a.0 + 10, a.1) };
                let plain = rails::plan(&world, &rails, &roads, &buildings, &signals, a, b);
                if plain.blocked != Some(Blocked::Slope) {
                    continue;
                }
                tried += 1;
                let plan = rails::plan_terraforming(&world, &rails, &roads, &buildings, &signals, a, b);
                if plan.blocked.is_some() {
                    continue;
                }
                assert!(plan.terraform_steps > 0 && plan.cost() > plan.pieces.len() as u32 * 100);
                let tiles = apply(&mut world, &plan.terraform);
                assert!(!tiles.is_empty());
                // The ground now takes the track as planned.
                let after = rails::plan(&world, &rails, &roads, &buildings, &signals, a, b);
                assert_eq!(after.blocked, None, "{a:?} -> {b:?} still blocked after levelling");
                assert_eq!(after.terraform_steps, 0);
                levelled += 1;
                if levelled == 6 {
                    break;
                }
            }
            if levelled == 6 {
                break;
            }
        }
        assert!(tried > 5 && levelled > 0, "tried {tried}, levelled {levelled}");
        assert_consistent(&world);
    }

    #[test]
    fn a_whole_hillside_is_too_much_to_level() {
        // A ramp rising one block per tile all the way up: a bench across it
        // would drag the entire hillside with it.
        let mut world = World::new(64, 64, 80, 2);
        for z in 0..64usize {
            for x in 0..64usize {
                let row = z as i32 + 4;
                write_tile(&mut world, x, z, [row, row, row + 1, row + 1]);
            }
        }
        assert_consistent(&world);
        let (roads, rails, buildings, signals) = (
            Layer::new(64, 64),
            Layer::new(64, 64),
            Layer::new(64, 64),
            Signals::default(),
        );
        let plan = rails::plan_terraforming(&world, &rails, &roads, &buildings, &signals, (20, 30), (26, 30));
        assert_eq!(plan.blocked, Some(Blocked::Slope));
        // Track up the slope needs no levelling at all.
        let up = rails::plan_terraforming(&world, &rails, &roads, &buildings, &signals, (20, 20), (20, 26));
        assert_eq!((up.blocked, up.terraform_steps), (None, 0));
    }
}
