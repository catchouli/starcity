//! Clouds: a separate, seamlessly tiling voxel layer that drifts over the map.
//!
//! The cloud field is a small `World` the same size as the map holding a
//! scattering of small clouds: clusters of domes shaped with the same corner
//! heights and slope limiter as the terrain, with flat bottoms. Cloud blocks
//! are drawn as cubes (one world unit tall), so they look chunky. It tiles, so the renderer can
//! drift it with the wind and wrap it around.

use crate::terrain::{Block, Material, Shape, World, mesh};

/// Cloud field height in blocks, and the level the flat bottoms hang from.
const HEIGHT: usize = 10;
const MID: u16 = 2;

/// Map area per cloud, in tiles²: fewer, smaller clouds that drift by now and then.
const AREA_PER_CLOUD: usize = 5000;

/// Corner heights (in blocks) of the cloud tops and each corner's cloud tone,
/// `(size + 1)²` each. Each cloud is a cluster of a few overlapping domes with
/// one tone. Everything wraps around, so the field tiles.
fn tops(size: usize, seed: u32) -> (Vec<u16>, Vec<u8>) {
    // Work on the wrapped size x size grid, then add the duplicate last row and column.
    let mut tops = vec![MID; size * size];
    let mut tones = vec![0u8; size * size];
    let mut rng = Rng((seed as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15) ^ 0xc10d);
    let clouds = (size * size / AREA_PER_CLOUD).max(1);
    for _ in 0..clouds {
        let (cx, cz) = (rng.below(size as u32) as i32, rng.below(size as u32) as i32);
        let tone = rng.below(4) as u8;
        let puffs = 2 + rng.below(3);
        for p in 0..puffs {
            // The first puff is the biggest; others sit around it.
            let r = if p == 0 {
                4.0 + rng.below(3) as f32
            } else {
                2.5 + rng.below(3) as f32
            };
            let (ox, oz) = if p == 0 {
                (0, 0)
            } else {
                (rng.below(9) as i32 - 4, rng.below(7) as i32 - 3)
            };
            let peak = if p == 0 {
                3.0 + rng.below(2) as f32
            } else {
                2.0 + rng.below(2) as f32
            };
            let reach = r.ceil() as i32;
            for dz in -reach..=reach {
                for dx in -reach..=reach {
                    let e = ((dx * dx + dz * dz) as f32 / (r * r)).min(1.0);
                    if e >= 1.0 {
                        continue;
                    }
                    let h = MID + (peak * (1.0 - e).sqrt()).ceil() as u16;
                    let x = (cx + ox + dx).rem_euclid(size as i32) as usize;
                    let z = (cz + oz + dz).rem_euclid(size as i32) as usize;
                    let i = z * size + x;
                    if h > tops[i] {
                        tops[i] = h;
                        tones[i] = tone;
                    }
                }
            }
        }
    }
    limit_slopes_wrapped(&mut tops, size);

    (unwrap_grid(&tops, size), unwrap_grid(&tones, size))
}

/// Expands a wrapped `size²` grid to `(size + 1)²` by repeating the first row and column.
fn unwrap_grid<T: Copy>(v: &[T], size: usize) -> Vec<T> {
    let n = size + 1;
    (0..n * n).map(|i| v[(i / n % size) * size + i % n % size]).collect()
}

/// Lowers corners until none is more than one block above any of its eight
/// neighbours, on a grid that wraps around at the edges.
fn limit_slopes_wrapped(h: &mut [u16], size: usize) {
    let s = size as i32;
    let at = |x: i32, z: i32| (z.rem_euclid(s) * s + x.rem_euclid(s)) as usize;
    loop {
        let mut changed = false;
        for z in 0..s {
            for x in 0..s {
                let mut v = h[at(x, z)];
                for (dx, dz) in [(-1, -1), (0, -1), (1, -1), (-1, 0), (1, 0), (-1, 1), (0, 1), (1, 1)] {
                    v = v.min(h[at(x + dx, z + dz)] + 1);
                }
                if v != h[at(x, z)] {
                    h[at(x, z)] = v;
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }
}

/// Generates a cloud field `size` x `size` tiles that tiles seamlessly.
pub fn generate(size: usize, seed: u32) -> World {
    let n = size + 1;
    let (tops, tones) = tops(size, seed);
    let mut world = World::new(size, size, HEIGHT, 0);
    for z in 0..size {
        for x in 0..size {
            let corners = Shape::CORNERS.map(|(cx, cz)| tops[(z + cz as usize) * n + x + cx as usize]);
            let top_max = *corners.iter().max().unwrap();
            if top_max <= MID {
                continue;
            }
            let base = *corners.iter().min().unwrap();
            let mut mask = 0;
            for (i, &c) in corners.iter().enumerate() {
                if c > base {
                    mask |= 1 << i;
                }
            }
            let material = Material::cloud(tones[z * n + x].max(tones[(z + 1) * n + x + 1]));
            // Thicker parts hang a block lower underneath.
            let bottom = if base >= MID + 2 { MID - 1 } else { MID };
            for y in bottom..base {
                world.set(x, y as usize, z, Block::full(material));
            }
            if mask != 0 {
                world.set(
                    x,
                    base as usize,
                    z,
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
    fn a_few_small_clouds_that_tile() {
        let size = 256;
        let w = generate(size, 5);
        let covered = (0..size as i32)
            .flat_map(|z| (0..size as i32).map(move |x| (x, z)))
            .filter(|&(x, z)| w.top_block(x, z).is_some())
            .count();
        let fraction = covered as f32 / (size * size) as f32;
        assert!((0.01..0.15).contains(&fraction), "cloud cover {fraction}");
        // Opposite edges of the corner grid match, so the field wraps without a seam.
        let (t, _) = tops(size, 5);
        let n = size + 1;
        for i in 0..n {
            assert_eq!(t[i * n], t[i * n + size]);
            assert_eq!(t[i], t[size * n + i]);
        }
    }
}
