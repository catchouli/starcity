//! Fixed-tick world simulation. For now: bulldozing and grass regrowth.

use crate::terrain::{Block, Material, World};

/// Simulation tick length.
pub const TICK_SECONDS: f32 = 0.25;

/// Chance per tick, per grass neighbour, that a bare dirt tile grows grass.
/// With one grass neighbour that's about 25 s on average; fully surrounded, about 6 s.
const GROW_CHANCE: f32 = 0.01;

pub struct Sim {
    accumulator: f32,
    rng: Rng,
    /// Bare dirt tiles that may regrow, in a stable order for determinism.
    regrowing: Vec<(u16, u16)>,
    is_regrowing: Vec<bool>,
    size_x: usize,
}

impl Sim {
    pub fn new(world: &World, seed: u64) -> Sim {
        Sim {
            accumulator: 0.0,
            rng: Rng(seed | 1),
            regrowing: Vec::new(),
            is_regrowing: vec![false; world.size_x * world.size_z],
            size_x: world.size_x,
        }
    }

    /// Whether ticking could change anything (so the app knows to keep waking up).
    pub fn is_active(&self) -> bool {
        !self.regrowing.is_empty()
    }

    /// Clears an inclusive rectangle of tiles to bare ground. Grass becomes dirt
    /// and starts regrowing; other surfaces are left as they are (there is
    /// nothing built on them yet). Returns the tiles that changed.
    pub fn bulldoze(&mut self, world: &mut World, a: (i32, i32), b: (i32, i32)) -> Vec<(i32, i32)> {
        let mut changed = Vec::new();
        for z in a.1.min(b.1)..=a.1.max(b.1) {
            for x in a.0.min(b.0)..=a.0.max(b.0) {
                let Some((y, block)) = world.top_block(x, z) else {
                    continue;
                };
                if block.material != Material::Grass {
                    continue;
                }
                world.set(
                    x as usize,
                    y,
                    z as usize,
                    Block {
                        material: Material::Dirt,
                        ..block
                    },
                );
                changed.push((x, z));
                let i = z as usize * self.size_x + x as usize;
                if !self.is_regrowing[i]
                    && world
                        .surface(x, z)
                        .is_some_and(|(base, _)| base > world.sea_level)
                {
                    self.is_regrowing[i] = true;
                    self.regrowing.push((x as u16, z as u16));
                }
            }
        }
        changed
    }

    /// Runs as many ticks as `dt` covers. Returns the tiles that changed.
    pub fn advance(&mut self, world: &mut World, dt: f32) -> Vec<(i32, i32)> {
        let mut changed = Vec::new();
        self.accumulator += dt;
        // Don't try to catch up on long stalls (e.g. a backgrounded tab).
        self.accumulator = self.accumulator.min(TICK_SECONDS * 8.0);
        while self.accumulator >= TICK_SECONDS {
            self.accumulator -= TICK_SECONDS;
            self.tick(world, &mut changed);
        }
        changed
    }

    fn tick(&mut self, world: &mut World, changed: &mut Vec<(i32, i32)>) {
        let is_grass = |x: i32, z: i32| {
            world
                .top_block(x, z)
                .is_some_and(|(_, b)| b.material == Material::Grass)
        };
        // Decide every tile against the state at the start of the tick, then apply.
        let mut grow = Vec::new();
        for (i, &(x, z)) in self.regrowing.iter().enumerate() {
            let (x, z) = (x as i32, z as i32);
            let neighbours = [(1, 0), (-1, 0), (0, 1), (0, -1)]
                .iter()
                .filter(|(dx, dz)| is_grass(x + dx, z + dz))
                .count();
            if neighbours == 0 {
                continue;
            }
            let chance = 1.0 - (1.0 - GROW_CHANCE).powi(neighbours as i32);
            if self.rng.next_f32() < chance {
                grow.push(i);
            }
        }
        // Remove from the back so earlier indices stay valid.
        for &i in grow.iter().rev() {
            let (x, z) = self.regrowing.swap_remove(i);
            self.is_regrowing[z as usize * self.size_x + x as usize] = false;
            let (x, z) = (x as i32, z as i32);
            if let Some((y, block)) = world.top_block(x, z) {
                if block.material == Material::Dirt {
                    world.set(
                        x as usize,
                        y,
                        z as usize,
                        Block {
                            material: Material::Grass,
                            ..block
                        },
                    );
                    changed.push((x, z));
                }
            }
        }
    }
}

/// xorshift64*: tiny, fast, deterministic and identical on every platform.
struct Rng(u64);

impl Rng {
    fn next_u64(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    fn next_f32(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grass_world(n: usize) -> World {
        let mut w = World::new(n, n, 4, 0);
        for x in 0..n {
            for z in 0..n {
                w.set(x, 0, z, Block::full(Material::Dirt));
                w.set(x, 1, z, Block::full(Material::Grass));
            }
        }
        w
    }

    #[test]
    fn bulldozed_patch_regrows_from_edges() {
        let mut w = grass_world(12);
        let mut sim = Sim::new(&w, 7);
        let changed = sim.bulldoze(&mut w, (3, 3), (8, 8));
        assert_eq!(changed.len(), 36);
        assert_eq!(w.top_block(5, 5).unwrap().1.material, Material::Dirt);

        let mut ticks = 0;
        while sim.is_active() && ticks < 20_000 {
            sim.advance(&mut w, TICK_SECONDS);
            ticks += 1;
        }
        assert!(!sim.is_active(), "patch should fully regrow");
        assert_eq!(w.top_block(5, 5).unwrap().1.material, Material::Grass);
    }

    #[test]
    fn isolated_dirt_never_regrows() {
        let mut w = grass_world(5);
        let mut sim = Sim::new(&w, 3);
        sim.bulldoze(&mut w, (0, 0), (4, 4));
        for _ in 0..1000 {
            assert!(sim.advance(&mut w, TICK_SECONDS).is_empty());
        }
    }
}
