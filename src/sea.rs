//! The sea as a navigation grid, and the pathfinding boats use on it.
//!
//! A tile is water if its surface is below sea level. [`Sea`] keeps one flag
//! byte per tile plus a label for each connected body of water, so "can a boat
//! get from here to there?" is a lookup rather than a search. It's built on
//! first use (the whole scan costs a few milliseconds) and only patched
//! where terrain edits change a tile.
//!
//! [`Pathfinder`] is an A* that runs a fixed number of expansions per call, so
//! a long route is spread over several frames instead of stalling one. Boats
//! cache the routes they find, so a ferry line searches once.

use crate::terrain::World;
use glam::Vec2;
use std::cmp::Reverse;
use std::collections::{BinaryHeap, VecDeque};

const WATER: u8 = 1;
/// Water with land (or the map's edge of land) in one of its eight neighbours.
const SHORE: u8 = 2;

/// Orthogonal and diagonal step costs, and the extra cost of hugging the shore.
const STRAIGHT: u32 = 10;
const DIAGONAL: u32 = 14;
const SHORE_COST: u32 = 6;

pub fn tile_is_water(world: &World, x: i32, z: i32) -> bool {
    world.in_bounds(x, z) && world.underwater(x, z)
}

pub struct Sea {
    pub size_x: usize,
    pub size_z: usize,
    flags: Vec<u8>,
    /// Connected body of water (4-connected) each tile belongs to; 0 on land.
    comp: Vec<u16>,
    /// Tiles in each body of water, indexed by label.
    sizes: Vec<u32>,
    /// Bumped whenever a tile changes between land and water, which
    /// invalidates routes.
    pub version: u32,
}

impl Sea {
    pub fn build(world: &World) -> Sea {
        let mut sea = Sea {
            size_x: world.size_x,
            size_z: world.size_z,
            flags: vec![0; world.size_x * world.size_z],
            comp: Vec::new(),
            sizes: Vec::new(),
            version: 1,
        };
        for z in 0..world.size_z as i32 {
            for x in 0..world.size_x as i32 {
                if tile_is_water(world, x, z) {
                    let i = sea.idx(x, z);
                    sea.flags[i] |= WATER;
                }
            }
        }
        sea.mark_shores();
        sea.label();
        sea
    }

    fn idx(&self, x: i32, z: i32) -> usize {
        z as usize * self.size_x + x as usize
    }

    fn in_bounds(&self, x: i32, z: i32) -> bool {
        x >= 0 && z >= 0 && (x as usize) < self.size_x && (z as usize) < self.size_z
    }

    pub fn is_water(&self, x: i32, z: i32) -> bool {
        self.in_bounds(x, z) && self.flags[self.idx(x, z)] & WATER != 0
    }

    pub fn is_shore(&self, x: i32, z: i32) -> bool {
        self.in_bounds(x, z) && self.flags[self.idx(x, z)] & SHORE != 0
    }

    /// Label of the body of water at a tile, or 0 for land.
    pub fn component(&self, x: i32, z: i32) -> u16 {
        if self.in_bounds(x, z) {
            self.comp[self.idx(x, z)]
        } else {
            0
        }
    }

    /// Tiles in the body of water at a tile.
    pub fn body_size(&self, x: i32, z: i32) -> u32 {
        self.sizes.get(self.component(x, z) as usize).copied().unwrap_or(0)
    }

    fn mark_shores(&mut self) {
        for z in 0..self.size_z as i32 {
            for x in 0..self.size_x as i32 {
                self.refresh_shore(x, z);
            }
        }
    }

    fn refresh_shore(&mut self, x: i32, z: i32) {
        let i = self.idx(x, z);
        self.flags[i] &= !SHORE;
        if self.flags[i] & WATER == 0 {
            return;
        }
        let touches_land = (-1..=1).any(|dz| {
            (-1..=1).any(|dx| {
                // Beyond the map counts as water: the border is open sea.
                self.in_bounds(x + dx, z + dz) && !self.is_water(x + dx, z + dz)
            })
        });
        if touches_land {
            self.flags[i] |= SHORE;
        }
    }

    /// Labels connected bodies of water with a flood fill.
    fn label(&mut self) {
        self.comp = vec![0; self.flags.len()];
        self.sizes = vec![0];
        let mut stack = Vec::new();
        for start in 0..self.flags.len() {
            if self.flags[start] & WATER == 0 || self.comp[start] != 0 {
                continue;
            }
            // Past 65535 bodies of water they all share the last label.
            let label = (self.sizes.len()).min(u16::MAX as usize) as u16;
            if label as usize == self.sizes.len() {
                self.sizes.push(0);
            }
            self.comp[start] = label;
            stack.push(start);
            while let Some(i) = stack.pop() {
                self.sizes[label as usize] += 1;
                let (x, z) = ((i % self.size_x) as i32, (i / self.size_x) as i32);
                for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                    let (nx, nz) = (x + dx, z + dz);
                    if self.is_water(nx, nz) && self.comp[self.idx(nx, nz)] == 0 {
                        let n = self.idx(nx, nz);
                        self.comp[n] = label;
                        stack.push(n);
                    }
                }
            }
        }
    }

    /// Re-reads tiles after terrain edits. Returns whether any tile changed
    /// between land and water.
    pub fn refresh(&mut self, world: &World, tiles: &[(i32, i32)]) -> bool {
        let mut changed = false;
        for &(x, z) in tiles {
            if !self.in_bounds(x, z) {
                continue;
            }
            let i = self.idx(x, z);
            let water = tile_is_water(world, x, z);
            if water != (self.flags[i] & WATER != 0) {
                self.flags[i] = if water { WATER } else { 0 };
                changed = true;
            }
        }
        if changed {
            for &(x, z) in tiles {
                for dz in -1..=1 {
                    for dx in -1..=1 {
                        if self.in_bounds(x + dx, z + dz) {
                            self.refresh_shore(x + dx, z + dz);
                        }
                    }
                }
            }
            self.label();
            self.version += 1;
        }
        changed
    }

    /// The nearest water tile to a tile within `radius`, if any.
    pub fn nearest_water(&self, (x, z): (i32, i32), radius: i32) -> Option<(i32, i32)> {
        (-radius..=radius)
            .flat_map(|dz| (-radius..=radius).map(move |dx| (x + dx, z + dz)))
            .filter(|&(tx, tz)| self.is_water(tx, tz))
            .min_by_key(|&(tx, tz)| (tx - x).pow(2) + (tz - z).pow(2))
    }
}

/// A route between two water tiles: tile coordinates at each change of direction.
pub type Route = Vec<(i32, i32)>;
pub type RouteKey = ((i32, i32), (i32, i32));

struct Job {
    key: RouteKey,
    goal: usize,
}

/// Resumable A* over water tiles. Eight-way moves, but never cutting a
/// corner of land; entering shore tiles costs extra so routes keep a little
/// way off the coast when they can.
#[derive(Default)]
pub struct Pathfinder {
    /// Allocated on first search so games without boats pay nothing.
    g: Vec<u32>,
    parent: Vec<u32>,
    /// Which search last touched each tile (so nothing is cleared between searches).
    seen: Vec<u32>,
    stamp: u32,
    open: BinaryHeap<Reverse<(u32, u32)>>,
    job: Option<Job>,
    queue: VecDeque<RouteKey>,
    /// Expansions spent on the most recent `work` call (for the debug window).
    pub last_expansions: usize,
}

impl Pathfinder {
    /// Queues a search unless it's already queued or running.
    pub fn request(&mut self, key: RouteKey) {
        if self.job.as_ref().is_some_and(|j| j.key == key) || self.queue.contains(&key) {
            return;
        }
        self.queue.push_back(key);
    }

    pub fn pending(&self) -> usize {
        self.queue.len() + self.job.is_some() as usize
    }

    /// Drops everything in progress (the sea changed under it).
    pub fn reset(&mut self) {
        self.job = None;
        self.queue.clear();
        self.open.clear();
    }

    /// Spends up to `budget` node expansions, finishing at most one search
    /// per call into `done` (`None` if the two tiles aren't connected).
    pub fn work(&mut self, sea: &Sea, budget: usize, done: &mut Vec<(RouteKey, Option<Route>)>) {
        self.last_expansions = 0;
        if self.job.is_none() {
            let Some(key) = self.queue.pop_front() else { return };
            let (from, to) = key;
            let connected = sea.is_water(from.0, from.1)
                && sea.is_water(to.0, to.1)
                && sea.component(from.0, from.1) == sea.component(to.0, to.1);
            if !connected {
                done.push((key, None));
                return;
            }
            if from == to {
                done.push((key, Some(vec![from])));
                return;
            }
            if self.g.len() != sea.size_x * sea.size_z {
                let n = sea.size_x * sea.size_z;
                self.g = vec![0; n];
                self.parent = vec![0; n];
                self.seen = vec![0; n];
                self.stamp = 0;
            }
            self.stamp += 1;
            self.open.clear();
            let start = from.1 as usize * sea.size_x + from.0 as usize;
            self.g[start] = 0;
            self.parent[start] = start as u32;
            self.seen[start] = self.stamp;
            let goal = to.1 as usize * sea.size_x + to.0 as usize;
            self.open.push(Reverse((heuristic(from, to), start as u32)));
            self.job = Some(Job { key, goal });
        }
        let Some(job) = self.job.as_ref() else { return };
        let (key, goal) = (job.key, job.goal);
        let target = key.1;
        let width = sea.size_x as i32;
        while self.last_expansions < budget {
            let Some(Reverse((_, node))) = self.open.pop() else {
                // Same body of water, so this shouldn't happen; fail safe.
                self.job = None;
                done.push((key, None));
                return;
            };
            let node = node as usize;
            self.last_expansions += 1;
            if node == goal {
                done.push((key, Some(self.finish(sea, goal))));
                self.job = None;
                return;
            }
            let (x, z) = ((node % sea.size_x) as i32, (node / sea.size_x) as i32);
            let g = self.g[node];
            for dz in -1i32..=1 {
                for dx in -1i32..=1 {
                    if dx == 0 && dz == 0 {
                        continue;
                    }
                    let (nx, nz) = (x + dx, z + dz);
                    if !sea.is_water(nx, nz) {
                        continue;
                    }
                    let diagonal = dx != 0 && dz != 0;
                    if diagonal && !(sea.is_water(x + dx, z) && sea.is_water(x, z + dz)) {
                        continue;
                    }
                    let n = (nz * width + nx) as usize;
                    let mut cost = g + if diagonal { DIAGONAL } else { STRAIGHT };
                    if sea.is_shore(nx, nz) {
                        cost += SHORE_COST;
                    }
                    if self.seen[n] != self.stamp || cost < self.g[n] {
                        self.seen[n] = self.stamp;
                        self.g[n] = cost;
                        self.parent[n] = node as u32;
                        self.open.push(Reverse((cost + heuristic((nx, nz), target), n as u32)));
                    }
                }
            }
        }
    }

    /// Walks the parents back from the goal, keeping only the turning points.
    fn finish(&self, sea: &Sea, goal: usize) -> Route {
        let tile = |i: usize| ((i % sea.size_x) as i32, (i / sea.size_x) as i32);
        let mut tiles = vec![tile(goal)];
        let mut at = goal;
        while self.parent[at] as usize != at {
            at = self.parent[at] as usize;
            tiles.push(tile(at));
        }
        tiles.reverse();
        let mut route = vec![tiles[0]];
        for w in tiles.windows(3) {
            let (a, b, c) = (w[0], w[1], w[2]);
            if (b.0 - a.0, b.1 - a.1) != (c.0 - b.0, c.1 - b.1) {
                route.push(b);
            }
        }
        route.push(*tiles.last().unwrap());
        route
    }
}

/// Octile distance: exact on open water, and never more than the true cost.
fn heuristic(a: (i32, i32), b: (i32, i32)) -> u32 {
    let (dx, dz) = ((a.0 - b.0).unsigned_abs(), (a.1 - b.1).unsigned_abs());
    let (lo, hi) = (dx.min(dz), dx.max(dz));
    lo * DIAGONAL + (hi - lo) * STRAIGHT
}

/// A tile's centre.
pub fn centre((x, z): (i32, i32)) -> Vec2 {
    Vec2::new(x as f32 + 0.5, z as f32 + 0.5)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terrain::generation::{self, GenParams};
    use crate::terrain::{Block, Material};

    /// Sea everywhere except a land wall with one gap, plus a sealed pond.
    fn walled_world() -> World {
        let n = 64;
        let mut w = World::new(n, n, 8, 4);
        for z in 0..n {
            for x in 0..n {
                let wall = x == 32 && z != 10;
                let pond_rim =
                    (50..=54).contains(&x) && (50..=54).contains(&z) && (x == 50 || x == 54 || z == 50 || z == 54);
                if wall || pond_rim {
                    for y in 0..5 {
                        w.set(x, y, z, Block::full(Material::Grass));
                    }
                } else {
                    w.set(x, 0, z, Block::full(Material::Sand));
                }
            }
        }
        w
    }

    fn solve(sea: &Sea, from: (i32, i32), to: (i32, i32)) -> Option<Route> {
        let mut pf = Pathfinder::default();
        pf.request((from, to));
        let mut done = Vec::new();
        for _ in 0..1000 {
            pf.work(sea, 500, &mut done);
            if let Some((_, route)) = done.pop() {
                return route;
            }
        }
        panic!("search never finished");
    }

    #[test]
    fn routes_go_through_the_gap_and_never_over_land() {
        let w = walled_world();
        let sea = Sea::build(&w);
        let route = solve(&sea, (5, 40), (60, 40)).expect("connected through the gap");
        assert_eq!((route[0], *route.last().unwrap()), ((5, 40), (60, 40)));

        // Every step between waypoints crosses water only.
        for pair in route.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            let n = (b.0 - a.0).abs().max((b.1 - a.1).abs()).max(1);
            for i in 0..=n {
                let p = (a.0 + (b.0 - a.0) * i / n, a.1 + (b.1 - a.1) * i / n);
                assert!(sea.is_water(p.0, p.1), "{p:?} is land");
                assert!(p.0 != 32 || p.1 == 10, "crossed the wall at {p:?}");
            }
        }
    }

    #[test]
    fn sealed_water_is_a_separate_body() {
        let w = walled_world();
        let sea = Sea::build(&w);
        assert_ne!(sea.component(52, 52), sea.component(5, 5));
        assert_eq!(sea.component(32, 20), 0, "land has no body of water");
        assert!(solve(&sea, (5, 5), (52, 52)).is_none());
        assert!(sea.body_size(52, 52) == 9);
    }

    #[test]
    fn refresh_reconnects_when_the_land_goes() {
        let mut w = walled_world();
        let mut sea = Sea::build(&w);
        let v = sea.version;
        // Dig a channel through the pond rim.
        for y in 0..5 {
            w.set(50, y, 52, Block::AIR);
        }
        w.set(50, 0, 52, Block::full(Material::Sand));
        assert!(sea.refresh(&w, &[(50, 52)]));
        assert!(sea.version > v);
        assert_eq!(sea.component(52, 52), sea.component(5, 5), "the pond joined the sea");
    }

    #[test]
    fn routes_on_the_real_map_are_fast_and_budgeted() {
        let world = generation::generate(&GenParams::default());
        let t = std::time::Instant::now();
        let sea = Sea::build(&world);
        let build = t.elapsed();
        // Two far-apart points on the ocean ring.
        let ring = |x: i32, z: i32| sea.nearest_water((x, z), 30).unwrap();
        let (a, b) = (ring(3, 3), ring(508, 508));
        assert_eq!(sea.component(a.0, a.1), sea.component(b.0, b.1));
        let mut pf = Pathfinder::default();
        pf.request((a, b));
        let (mut done, mut calls, mut worst) = (Vec::new(), 0, std::time::Duration::ZERO);
        let mut times = Vec::new();
        let t = std::time::Instant::now();
        while done.is_empty() {
            let c = std::time::Instant::now();
            pf.work(&sea, 4000, &mut done);
            worst = worst.max(c.elapsed());
            times.push(c.elapsed());
            calls += 1;
        }
        eprintln!(
            "sea build {build:?}; route in {calls} calls of 4000 expansions, {:?} total, worst call {worst:?}",
            t.elapsed()
        );
        let route = done.pop().unwrap().1.expect("route");
        assert!(route.len() >= 2);
        // The median call, not the single worst: a test run in parallel gets descheduled now and then.
        times.sort();
        let median = times[times.len() / 2];
        assert!(median.as_millis() < 20, "a typical call took {median:?} (worst {worst:?})");
    }
}
