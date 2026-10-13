//! Clouds: a separate, seamlessly tiling voxel layer that drifts over the map.
//!
//! The cloud field is a small `World` the same size as the map holding a
//! scattering of small clouds: clusters of tall, lumpy puffs built from
//! columns of blocks, with a one-block chamfer on their top edges. Cloud
//! blocks are drawn as cubes (one world unit tall), so they look chunky. It tiles, so the renderer can
//! drift it with the wind and wrap it around.

use crate::terrain::{Block, Material, Shape, World, mesh};

/// Cloud field height in blocks, and the level clouds are centred on.
const HEIGHT: usize = 16;
const MID: i32 = 6;

/// Map area per cloud, in tiles²: a few small clouds that drift by now and then.
const AREA_PER_CLOUD: usize = 5000;

/// Per-tile cloud columns on the wrapped `size²` grid: (bottom, top, tone),
/// with top == bottom meaning no cloud. Each cloud is a cluster of puffs:
/// rounded blobs about as tall as they are wide, with lumpy tops.
fn columns(size: usize, seed: u32) -> Vec<(i32, i32, u8)> {
    let mut cols = vec![(MID, MID, 0u8); size * size];
    let mut rng = Rng((seed as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15) ^ 0xc10d);
    let clouds = (size * size / AREA_PER_CLOUD).max(1);
    for _ in 0..clouds {
        let (cx, cz) = (rng.below(size as u32) as i32, rng.below(size as u32) as i32);
        let tone = rng.below(4) as u8;
        let puffs = 3 + rng.below(4);
        for p in 0..puffs {
            // A big puff in the middle, smaller ones strung out around it.
            let r = if p == 0 {
                3.5 + rng.below(2) as f32
            } else {
                2.0 + rng.below(3) as f32 * 0.75
            };
            let (ox, oz) = if p == 0 {
                (0, 0)
            } else {
                (rng.below(11) as i32 - 5, rng.below(7) as i32 - 3)
            };
            let reach = r.ceil() as i32;
            for dz in -reach..=reach {
                for dx in -reach..=reach {
                    let e = (dx * dx + dz * dz) as f32 / (r * r);
                    if e >= 1.0 {
                        continue;
                    }
                    let k = (1.0 - e).sqrt();
                    let lump = rng.below(2) as i32;
                    let top = MID + (r * 0.9 * k).ceil() as i32 + lump;
                    let bottom = MID - (r * 0.35 * k).floor() as i32;
                    let x = (cx + ox + dx).rem_euclid(size as i32) as usize;
                    let z = (cz + oz + dz).rem_euclid(size as i32) as usize;
                    let c = &mut cols[z * size + x];
                    if c.1 == c.0 {
                        *c = (bottom, top, tone);
                    } else {
                        *c = (c.0.min(bottom), c.1.max(top), c.2);
                    }
                }
            }
        }
    }
    cols
}

/// Generates a cloud field `size` x `size` tiles that tiles seamlessly.
///
/// Columns are stacks of full blocks. The top block of each column is
/// chamfered towards lower neighbours (corners drop one block where any
/// adjacent column is lower), so puffs get rounded edges but keep steep sides.
pub fn generate(size: usize, seed: u32) -> World {
    let cols = columns(size, seed);
    let s = size as i32;
    let col = |x: i32, z: i32| cols[(z.rem_euclid(s) * s + x.rem_euclid(s)) as usize];
    let top = |x: i32, z: i32| {
        let (b, t, _) = col(x, z);
        if t > b { t } else { 0 }
    };
    let mut world = World::new(size, size, HEIGHT, 0);
    for z in 0..s {
        for x in 0..s {
            let (bottom, t, tone) = col(x, z);
            if t <= bottom {
                continue;
            }
            let material = Material::cloud(tone);
            // A corner stays raised only if every column sharing it is at least as tall.
            let mut mask = 0u8;
            for (i, (cx, cz)) in Shape::CORNERS.iter().enumerate() {
                let (cx, cz) = (*cx as i32, *cz as i32);
                let lowest = [(cx - 1, cz - 1), (cx, cz - 1), (cx - 1, cz), (cx, cz)]
                    .iter()
                    .map(|&(dx, dz)| top(x + dx, z + dz))
                    .min()
                    .unwrap();
                if lowest >= t {
                    mask |= 1 << i;
                }
            }
            let full_to = if mask == 0b1111 { t } else { t - 1 };
            for y in bottom..full_to {
                world.set(x as usize, y as usize, z as usize, Block::full(material));
            }
            if mask != 0 && mask != 0b1111 && t > bottom {
                world.set(
                    x as usize,
                    (t - 1) as usize,
                    z as usize,
                    Block {
                        material,
                        shape: Shape(mask),
                    },
                );
            }
        }
    }
    world
}

/// splitmix-ish generator; deterministic on every platform.
struct Rng(u64);

impl Rng {
    fn below(&mut self, n: u32) -> u32 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        ((z ^ (z >> 31)) % n as u64) as u32
    }
}

/// Meshes the whole cloud field. Returns each chunk's mesh with its origin in tiles.
pub fn mesh(world: &World) -> Vec<(mesh::ChunkMesh, (u32, u32))> {
    let mut out = Vec::new();
    for cz in 0..world.chunks_z() {
        for cx in 0..world.chunks_x() {
            let m = mesh::mesh_chunk(world, cx, cz);
            if !m.terrain.is_empty() {
                out.push((
                    m,
                    (
                        (cx * crate::terrain::CHUNK_SIZE) as u32,
                        (cz * crate::terrain::CHUNK_SIZE) as u32,
                    ),
                ));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_few_small_puffy_clouds() {
        let size = 256;
        let w = generate(size, 5);
        let mut covered = 0;
        let mut thickest = 0;
        for z in 0..size {
            for x in 0..size {
                let solid = w.column(x, z).iter().filter(|b| !b.is_air()).count();
                if solid > 0 {
                    covered += 1;
                    thickest = thickest.max(solid);
                }
            }
        }
        let fraction = covered as f32 / (size * size) as f32;
        assert!((0.01..0.15).contains(&fraction), "cloud cover {fraction}");
        assert!(
            thickest >= 5,
            "clouds should be tall puffs, not discs (thickest {thickest})"
        );
    }
}
