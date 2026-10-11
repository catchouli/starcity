//! Clouds: a separate, seamlessly tiling voxel layer that drifts over the map.
//!
//! The cloud field is a small `World` the same size as the map, filled from
//! periodic noise: puffy tops shaped with the same corner heights and slope
//! limiter as the terrain, and flat bottoms. It tiles, so the renderer can
//! drift it with the wind and wrap it around.

use crate::terrain::generation::limit_slopes;
use crate::terrain::{Block, Material, Shape, World, mesh};
use noise::{NoiseFn, Perlin};
use std::f64::consts::TAU;

/// Cloud field height in blocks, and the level the flat bottoms hang from.
const HEIGHT: usize = 12;
const MID: u16 = 4;

/// Approximate size of a cloud, in tiles.
const FEATURE: f64 = 28.0;
/// Noise density above which there is cloud. Higher means fewer clouds.
const THRESHOLD: f64 = 0.24;

/// Cloud density at a point. Sampling on a torus in 4D makes it periodic in x
/// and z with period `size`, so the cloud field tiles seamlessly.
fn density(noise: &Perlin, size: usize, x: f64, z: f64) -> f64 {
    let (a, b) = (x / size as f64 * TAU, z / size as f64 * TAU);
    let mut d = 0.0;
    let mut amp = 1.0;
    for octave in 0..3 {
        let r = size as f64 / TAU / FEATURE * 2f64.powi(octave);
        d += noise.get([
            r * a.cos(),
            r * a.sin(),
            r * b.cos() + 31.7 * octave as f64,
            r * b.sin(),
        ]) * amp;
        amp *= 0.45;
    }
    d
}

/// Generates a cloud field `size` x `size` tiles that tiles seamlessly.
pub fn generate(size: usize, seed: u32) -> World {
    let noise = Perlin::new(seed ^ 0xc10d);
    let density = |x: f64, z: f64| density(&noise, size, x, z);

    let n = size + 1;
    let mut tops = vec![MID; n * n];
    for z in 0..n {
        for x in 0..n {
            let d = density(x as f64, z as f64) - THRESHOLD;
            tops[z * n + x] = MID + ((d * 16.0).round().clamp(0.0, (HEIGHT as u16 - MID - 1) as f64) as u16);
        }
    }
    limit_slopes(&mut tops, n);

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
            // Thicker clouds hang a little lower underneath.
            let d = density(x as f64 + 0.5, z as f64 + 0.5) - THRESHOLD;
            let bottom = MID - ((d * 6.0).round().clamp(0.0, MID as f64) as u16);
            for y in bottom..base {
                world.set(x, y as usize, z, Block::full(Material::Cloud));
            }
            if mask != 0 {
                world.set(
                    x,
                    base as usize,
                    z,
                    Block {
                        material: Material::Cloud,
                        shape: Shape(mask),
                    },
                );
            }
        }
    }
    world
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
    fn clouds_cover_part_of_the_sky_and_tile() {
        let w = generate(128, 5);
        let covered = (0..128)
            .flat_map(|z| (0..128).map(move |x| (x, z)))
            .filter(|&(x, z)| w.top_block(x, z).is_some())
            .count();
        let fraction = covered as f32 / (128.0 * 128.0);
        assert!((0.05..0.6).contains(&fraction), "cloud cover {fraction}");
        // Density is periodic, so the field wraps without a seam.
        let noise = Perlin::new(5);
        for i in 0..20 {
            let z = i as f64 * 6.3;
            assert!((density(&noise, 128, 0.0, z) - density(&noise, 128, 128.0, z)).abs() < 1e-9);
            assert!((density(&noise, 128, z, 0.0) - density(&noise, 128, z, 128.0)).abs() < 1e-9);
        }
    }
}
