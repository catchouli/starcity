//! Rising seas (GAME-DESIGN.md §10.9): from the 2030s the sea climbs a block at a time. Each rise is
//! announced years ahead; when it comes, the low ground the sea can reach floods over several months,
//! nearest the water first, taking whatever stands there. Sea walls stop it: the sea only spreads
//! through low tiles it can reach without crossing a wall, so land behind a wall stays dry.

use crate::network::Layer;
use crate::terrain::World;
use std::collections::VecDeque;

/// Blocks the sea has risen by at `date`.
pub fn rise(date: f32) -> u16 {
    RISES.iter().filter(|&&y| date >= y).count() as u16
}

/// When each block of rise arrives.
pub const RISES: [f32; 3] = [2036.0, 2042.0, 2048.0];
/// Warnings start this long before a rise.
pub const WARNING_YEARS: f32 = 3.0;
/// A flood takes this many months to reach everything it will.
pub const FLOOD_MONTHS: usize = 6;
/// What a tile of sea wall costs.
pub const WALL: u32 = 600;

/// The next rise after `date`, if one is within the warning time: its year.
pub fn coming(date: f32) -> Option<f32> {
    RISES.iter().copied().find(|&y| y > date && y - date <= WARNING_YEARS)
}

/// Every tile the sea covers at `level` (in blocks), in the order it reaches them: starting from what
/// is underwater now and from the map's edge, it spreads through tiles lower than `level`, never
/// across a wall.
pub fn reach(world: &World, level: u16, walls: &Layer) -> Vec<(i32, i32)> {
    let (sx, sz) = (world.size_x as i32, world.size_z as i32);
    let low = |x: i32, z: i32| world.surface(x, z).is_none_or(|(base, _)| base < level);
    let mut seen = vec![false; world.size_x * world.size_z];
    let mut queue = VecDeque::new();
    let idx = |x: i32, z: i32| z as usize * world.size_x + x as usize;
    for z in 0..sz {
        for x in 0..sx {
            let edge = x == 0 || z == 0 || x == sx - 1 || z == sz - 1;
            if (world.underwater(x, z) || (edge && low(x, z))) && walls.get(x, z) == 0 {
                seen[idx(x, z)] = true;
                queue.push_back((x, z));
            }
        }
    }
    let mut out = Vec::new();
    while let Some((x, z)) = queue.pop_front() {
        out.push((x, z));
        for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let (nx, nz) = (x + dx, z + dz);
            if nx < 0 || nz < 0 || nx >= sx || nz >= sz || seen[idx(nx, nz)] {
                continue;
            }
            if walls.get(nx, nz) == 0 && low(nx, nz) {
                seen[idx(nx, nz)] = true;
                queue.push_back((nx, nz));
            }
        }
    }
    out
}

/// The sea's state as it rises.
#[derive(Default)]
pub struct Flood {
    /// The level the game began at.
    pub start: u16,
    /// Tiles the sea will take, nearest first, a share each month.
    pub pending: VecDeque<(i32, i32)>,
    per_month: usize,
}

impl Flood {
    pub fn new(start: u16) -> Flood {
        Flood {
            start,
            ..Default::default()
        }
    }

    /// Raises the sea to `level` if it's lower: everything below the new level the sea can't reach,
    /// or hasn't reached yet, is kept dry for now; what it will reach is queued. Returns the tiles
    /// queued.
    pub fn raise(&mut self, world: &mut World, level: u16, walls: &Layer) -> usize {
        if world.sea_level >= level {
            return 0;
        }
        let reached = reach(world, level, walls);
        // Low ground that's dry now stays dry until the flood gets to it (or for good, behind walls).
        for z in 0..world.size_z as i32 {
            for x in 0..world.size_x as i32 {
                if !world.underwater(x, z) && world.surface(x, z).is_some_and(|(b, _)| b < level) {
                    world.dry.insert((x, z));
                }
            }
        }
        world.sea_level = level;
        let newly: Vec<(i32, i32)> = reached.into_iter().filter(|t| world.dry.contains(t)).collect();
        self.per_month = newly.len().div_ceil(FLOOD_MONTHS).max(1);
        self.pending.extend(newly);
        self.pending.len()
    }

    /// A month of flooding: the next share of tiles goes under. Returns them (to clear what stands
    /// there, and redraw).
    pub fn month(&mut self, world: &mut World) -> Vec<(i32, i32)> {
        let n = self.per_month.min(self.pending.len());
        let tiles: Vec<(i32, i32)> = self.pending.drain(..n).collect();
        for t in &tiles {
            world.dry.remove(t);
        }
        tiles
    }

    /// A wall was built: tiles the sea can no longer reach come off the queue (and stay dry).
    pub fn walls_changed(&mut self, world: &World, walls: &Layer) {
        if self.pending.is_empty() {
            return;
        }
        let reached: std::collections::HashSet<(i32, i32)> = reach(world, world.sea_level, walls).into_iter().collect();
        self.pending.retain(|t| reached.contains(t) || !world.dry.contains(t));
        self.pending.retain(|t| walls.get(t.0, t.1) == 0);
    }

    /// Tiles the next rise would take, for warnings: what the sea would reach at the next level that
    /// is dry land now.
    pub fn at_risk(&self, world: &World, walls: &Layer) -> Vec<(i32, i32)> {
        reach(world, world.sea_level + 1, walls)
            .into_iter()
            .filter(|&(x, z)| !world.underwater(x, z))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terrain::generation::{self, GenParams};

    #[test]
    fn the_sea_rises_in_three_steps_with_warnings() {
        assert_eq!(rise(2030.0), 0);
        assert_eq!(rise(2037.0), 1);
        assert_eq!(rise(2050.0), 3);
        assert_eq!(coming(2034.0), Some(2036.0));
        assert_eq!(coming(2031.0), None, "too early to warn");
    }

    #[test]
    fn a_rise_floods_low_coast_gradually_and_a_wall_keeps_land_dry() {
        let mut world = generation::generate(&GenParams::default());
        let walls = Layer::new(world.size_x, world.size_z);
        let mut flood = Flood::new(world.sea_level);
        let level = world.sea_level + 1;
        let risk = flood.at_risk(&world, &walls);
        assert!(!risk.is_empty(), "some coast is low enough to flood");
        let queued = flood.raise(&mut world, level, &walls);
        assert_eq!(queued, risk.len());
        // Nothing has flooded yet: it's all still dry, and goes under over the months.
        assert!(risk.iter().all(|&(x, z)| !world.underwater(x, z)));
        let first = flood.month(&mut world);
        assert!(!first.is_empty() && first.len() < queued);
        assert!(first.iter().all(|&(x, z)| world.underwater(x, z)));
        for _ in 0..FLOOD_MONTHS {
            flood.month(&mut world);
        }
        assert!(flood.pending.is_empty());
        assert!(risk.iter().all(|&(x, z)| world.underwater(x, z)));
    }

    #[test]
    fn a_ring_of_wall_keeps_the_sea_out() {
        let mut world = generation::generate(&GenParams::default());
        let mut walls = Layer::new(world.size_x, world.size_z);
        let flood0 = Flood::new(world.sea_level);
        let risk = flood0.at_risk(&world, &walls);
        // Wall in one at-risk tile with a ring around it (on dry land only).
        let &(x, z) = risk.iter().find(|&&(x, z)| {
            (-2..=2).all(|dz| (-2..=2).all(|dx| !world.underwater(x + dx, z + dz)))
        }).expect("an inland at-risk tile");
        for dz in -2i32..=2 {
            for dx in -2i32..=2 {
                if dx.abs() == 2 || dz.abs() == 2 {
                    walls.set(x + dx, z + dz, 1);
                }
            }
        }
        let mut flood = Flood::new(world.sea_level);
        let level = world.sea_level + 1;
        flood.raise(&mut world, level, &walls);
        for _ in 0..=FLOOD_MONTHS {
            flood.month(&mut world);
        }
        assert!(!world.underwater(x, z), "behind the wall it stays dry");
    }
}
