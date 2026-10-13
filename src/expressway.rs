//! The hyper expressway (GAME-DESIGN.md §7.6): an elevated, autonomous road on pillars, from 2035.
//! It is drawn like a road (the same pieces: halves and diagonals) but ignores the ground, the
//! towns and every other network, so it's a straight line across anything. Autonomous pods
//! stream along it. A city it reaches has a big new reason to grow.

use crate::network::{Layer, Plan};
use crate::roads::HALVES;
use crate::terrain::World;
use crate::vehicles::{RoadSeg, next_road_segments};
use glam::{Vec2, Vec3};
use std::collections::HashMap;

/// The first year it can be built.
pub const FROM: i32 = 2035;
/// What a piece costs.
pub const PIECE: u32 = 400;
/// How far above the ground (the highest point around each tile) the deck runs, and the steepest
/// it climbs or falls per tile.
const CLEARANCE: f32 = 2.0;
const GRADE: f32 = 0.35;
const POD_SPEED: f32 = 10.0;

#[derive(Clone, Copy, Debug)]
pub struct Pod {
    pub seg: RoadSeg,
    pub pos: f32,
}

pub struct Expressway {
    pub layer: Layer,
    /// The deck's height over each tile.
    pub height: HashMap<(i32, i32), f32>,
    pub pods: Vec<Pod>,
    rng: u64,
}

impl Expressway {
    pub fn new(size_x: usize, size_z: usize) -> Expressway {
        Expressway {
            layer: Layer::new(size_x, size_z),
            height: HashMap::new(),
            pods: Vec::new(),
            rng: 0x2545_F491_4F6C_DD1D,
        }
    }

    /// The pieces a drag from `a` to `b` would add: road-shaped, never blocked.
    pub fn plan(&self, a: (i32, i32), b: (i32, i32)) -> Plan {
        crate::roads::shape(&self.layer, a, b)
    }

    /// Builds a plan: the deck follows the ground a little above it, never dipping into a hill and
    /// never climbing or falling faster than a gentle grade; where it meets a deck already there it
    /// keeps that height. Returns the tiles built on.
    pub fn build(&mut self, plan: &Plan, world: &World) -> Vec<(i32, i32)> {
        // Tiles in the order the drag lays them.
        let mut tiles: Vec<(i32, i32)> = Vec::new();
        for p in &plan.pieces {
            if !tiles.contains(&p.tile) {
                tiles.push(p.tile);
            }
        }
        let clear = |t: (i32, i32)| {
            let mut top = f32::MIN;
            for dz in -1..=1 {
                for dx in -1..=1 {
                    top = top.max(world.height_at((t.0 + dx) as f32 + 0.5, (t.1 + dz) as f32 + 0.5));
                }
            }
            top + CLEARANCE
        };
        let mut h: Vec<f32> = tiles
            .iter()
            .map(|&t| self.height.get(&t).copied().unwrap_or_else(|| clear(t)))
            .collect();
        for i in 1..h.len() {
            h[i] = h[i].max(h[i - 1] - GRADE);
        }
        for i in (0..h.len().saturating_sub(1)).rev() {
            h[i] = h[i].max(h[i + 1] - GRADE);
        }
        for (t, y) in tiles.iter().zip(h) {
            self.height.entry(*t).or_insert(y);
        }
        plan.build(&mut self.layer)
    }

    /// Removes the deck in an inclusive rectangle. Returns whether anything went.
    pub fn clear(&mut self, a: (i32, i32), b: (i32, i32)) -> bool {
        let mut gone = false;
        for z in a.1.min(b.1)..=a.1.max(b.1) {
            for x in a.0.min(b.0)..=a.0.max(b.0) {
                if self.layer.get(x, z) != 0 {
                    self.layer.set(x, z, 0);
                    self.height.remove(&(x, z));
                    gone = true;
                }
            }
        }
        if gone {
            let layer = &self.layer;
            self.pods
                .retain(|p| layer.get(p.seg.tile.0, p.seg.tile.1) & p.seg.piece != 0);
        }
        gone
    }

    pub fn pieces(&self) -> usize {
        self.height
            .keys()
            .map(|&(x, z)| self.layer.get(x, z).count_ones() as usize)
            .sum()
    }

    /// Whether the deck comes within `r` tiles (square) of a point.
    pub fn near(&self, at: (i32, i32), r: i32) -> bool {
        self.height
            .keys()
            .any(|&(x, z)| (x - at.0).abs() <= r && (z - at.1).abs() <= r)
    }

    /// Where a pod is drawn, and which way it faces.
    pub fn pod_place(&self, p: &Pod) -> (Vec3, Vec2) {
        let (a, _) = p.seg.ends();
        let dir = p.seg.dir();
        let xz = a + dir * p.pos + Vec2::new(-dir.y, dir.x) * 0.14;
        let h = self.height.get(&p.seg.tile).copied().unwrap_or(0.0);
        (Vec3::new(xz.x, h + 0.08, xz.y), dir)
    }

    fn roll(&mut self, n: usize) -> usize {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        (self.rng % n.max(1) as u64) as usize
    }

    /// Keeps about one pod for every three pieces (at most 300), and moves them along at speed,
    /// taking a random way at each junction. Returns whether any moved.
    pub fn update(&mut self, dt: f32) -> bool {
        let want = (self.pieces() / 3).min(300);
        if self.pods.len() > want {
            self.pods.truncate(want);
        }
        if self.pods.len() < want {
            let tiles: Vec<(i32, i32)> = self.height.keys().copied().collect();
            if !tiles.is_empty() {
                let t = tiles[self.roll(tiles.len())];
                let bits = self.layer.get(t.0, t.1);
                let pieces: Vec<u8> = (0..8).map(|i| 1u8 << i).filter(|p| bits & p != 0).collect();
                if !pieces.is_empty() {
                    let piece = pieces[self.roll(pieces.len())];
                    let forward = self.roll(2) == 0;
                    self.pods.push(Pod {
                        seg: RoadSeg {
                            tile: t,
                            piece,
                            forward,
                        },
                        pos: 0.0,
                    });
                }
            }
        }
        for i in 0..self.pods.len() {
            let mut p = self.pods[i];
            p.pos += POD_SPEED * dt;
            while p.pos >= p.seg.len() {
                p.pos -= p.seg.len();
                let next = next_road_segments(&self.layer, p.seg);
                if next.is_empty() {
                    p.pos = 0.0;
                    break;
                }
                p.seg = next[self.roll(next.len())];
            }
            self.pods[i] = p;
        }
        !self.pods.is_empty()
    }
}

/// Whether a piece is a half (centre to edge) rather than a diagonal.
pub fn is_half(piece: u8) -> bool {
    piece & HALVES != 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terrain::generation::{self, GenParams};

    #[test]
    fn a_drag_builds_a_level_deck_above_the_hills_and_pods_run_along_it() {
        let world = generation::generate(&GenParams::default());
        let mut ex = Expressway::new(world.size_x, world.size_z);
        let plan = ex.plan((100, 200), (160, 200));
        assert!(plan.blocked.is_none() && plan.pieces.len() >= 60);
        let built = ex.build(&plan, &world);
        assert_eq!(built.len(), 61);
        for x in 100..=160 {
            let deck = ex.height[&(x, 200)];
            assert!(
                deck >= world.height_at(x as f32 + 0.5, 200.5) + 1.9,
                "clear of the ground at {x}"
            );
            if x > 100 {
                assert!((deck - ex.height[&(x - 1, 200)]).abs() <= 0.36, "a gentle grade at {x}");
            }
        }
        for _ in 0..600 {
            ex.update(1.0 / 60.0);
        }
        assert!(ex.pods.len() >= 10);
        for p in &ex.pods {
            assert!(
                ex.layer.get(p.seg.tile.0, p.seg.tile.1) & p.seg.piece != 0,
                "pods stay on the deck"
            );
        }
        assert!(ex.near((130, 205), 6) && !ex.near((130, 230), 6));
        assert!(ex.clear((90, 190), (170, 210)));
        assert_eq!(ex.pieces(), 0);
        assert!(ex.pods.is_empty());
    }
}
