//! Map generation: a smooth corner heightmap turned into sloped voxel terrain.
//!
//! 1. Low-frequency, domain-warped fBm gives the broad hills.
//! 2. A power curve flattens the lowlands into wide valleys.
//! 3. A second noise field marks "flat" regions where heights are softly terraced
//!    into plateaus, which gives settlements somewhere to go.
//! 4. Heights are quantised to whole blocks, then clamped so that neighbouring
//!    corners (including diagonals) differ by at most one block. That means every
//!    tile is either flat or a single-step slope, as in TTD.
//! 5. Each tile's four corner heights become a column of full blocks topped by
//!    one shaped block.

use super::{Block, Material, Shape, World};
use noise::{NoiseFn, Perlin};

#[derive(Clone, Debug, PartialEq)]
pub struct GenParams {
    pub seed: u32,
    /// Map size in tiles (square).
    pub size: usize,
    /// Highest possible terrain, in blocks.
    pub max_height: u16,
    /// Water level, in blocks.
    pub sea_level: u16,
    /// Wavelength of the largest hills, in tiles.
    pub hill_scale: f32,
    pub octaves: u32,
    /// Amplitude multiplier per octave. Lower means smoother terrain.
    pub persistence: f32,
    /// How far (in tiles) the domain warp pushes sample positions.
    pub warp: f32,
    /// >1 flattens low ground into broad valleys and sharpens peaks.
    pub valley_exponent: f32,
    /// How much of the map is flattened into terraced plateaus (0..1).
    pub flatness: f32,
    /// Height between terrace levels, in blocks.
    pub terrace_step: f32,
}

impl Default for GenParams {
    fn default() -> Self {
        GenParams {
            seed: 1,
            size: 256,
            max_height: 40,
            sea_level: 6,
            hill_scale: 180.0,
            octaves: 4,
            persistence: 0.4,
            warp: 30.0,
            valley_exponent: 1.5,
            flatness: 0.5,
            terrace_step: 5.0,
        }
    }
}

/// Generates the corner heightmap, `(size + 1)^2` heights in blocks, row-major in z.
pub fn generate_heights(p: &GenParams) -> Vec<u16> {
    let n = p.size + 1;
    let base = Perlin::new(p.seed);
    let warp_x = Perlin::new(p.seed.wrapping_add(1));
    let warp_z = Perlin::new(p.seed.wrapping_add(2));
    let flat = Perlin::new(p.seed.wrapping_add(3));

    let amp_total: f64 = (0..p.octaves).map(|o| (p.persistence as f64).powi(o as i32)).sum();
    let mut heights = vec![0u16; n * n];

    for z in 0..n {
        for x in 0..n {
            let px = x as f64 / p.hill_scale as f64;
            let pz = z as f64 / p.hill_scale as f64;

            let w = p.warp as f64 / p.hill_scale as f64;
            let wx = px + warp_x.get([px * 0.7, pz * 0.7]) * w;
            let wz = pz + warp_z.get([px * 0.7, pz * 0.7]) * w;

            let mut e = 0.0;
            let mut amp = 1.0;
            let mut freq = 1.0;
            for _ in 0..p.octaves {
                e += base.get([wx * freq, wz * freq]) * amp;
                amp *= p.persistence as f64;
                freq *= 2.0;
            }
            // Perlin output mostly sits within about ±0.7; stretch it to roughly 0..1.
            let e = (e / amp_total * 0.75 + 0.5).clamp(0.0, 1.0) as f32;
            let mut h = e.powf(p.valley_exponent) * p.max_height as f32;

            let f = flat.get([px * 0.6 + 17.3, pz * 0.6 - 4.1]) as f32 * 0.5 + 0.5;
            let threshold = 1.0 - p.flatness;
            let mask = smoothstep(threshold - 0.08, threshold + 0.08, f);
            if mask > 0.0 && p.terrace_step > 0.0 {
                let t = h / p.terrace_step;
                let terraced = (t.floor() + smoothstep(0.4, 0.6, t.fract())) * p.terrace_step;
                h += (terraced - h) * mask;
            }

            heights[z * n + x] = h.round().clamp(0.0, p.max_height as f32) as u16;
        }
    }

    limit_slopes(&mut heights, n);
    heights
}

/// Lowers corners so no corner is more than one block above any of its eight
/// neighbours. This is a chamfer distance transform with unit weights, which is
/// exact for the 8-neighbour metric after one forward and one backward pass.
pub(crate) fn limit_slopes(h: &mut [u16], n: usize) {
    let at = |x: usize, z: usize| z * n + x;
    for z in 0..n {
        for x in 0..n {
            let mut v = h[at(x, z)];
            if x > 0 {
                v = v.min(h[at(x - 1, z)] + 1);
            }
            if z > 0 {
                v = v.min(h[at(x, z - 1)] + 1);
                if x > 0 {
                    v = v.min(h[at(x - 1, z - 1)] + 1);
                }
                if x + 1 < n {
                    v = v.min(h[at(x + 1, z - 1)] + 1);
                }
            }
            h[at(x, z)] = v;
        }
    }
    for z in (0..n).rev() {
        for x in (0..n).rev() {
            let mut v = h[at(x, z)];
            if x + 1 < n {
                v = v.min(h[at(x + 1, z)] + 1);
            }
            if z + 1 < n {
                v = v.min(h[at(x, z + 1)] + 1);
                if x + 1 < n {
                    v = v.min(h[at(x + 1, z + 1)] + 1);
                }
                if x > 0 {
                    v = v.min(h[at(x - 1, z + 1)] + 1);
                }
            }
            h[at(x, z)] = v;
        }
    }
}

pub fn generate(p: &GenParams) -> World {
    let heights = generate_heights(p);
    let n = p.size + 1;
    let mut world = World::new(p.size, p.size, p.max_height as usize + 16, p.sea_level);
    let rock_line = (p.max_height as f32 * 0.8) as u16;

    for z in 0..p.size {
        for x in 0..p.size {
            let corners = Shape::CORNERS.map(|(cx, cz)| heights[(z + cz as usize) * n + x + cx as usize]);
            let base = *corners.iter().min().unwrap();
            let mut mask = 0u8;
            for (i, &c) in corners.iter().enumerate() {
                if c > base {
                    mask |= 1 << i;
                }
            }

            let surface = if base <= p.sea_level {
                Material::Gravel
            } else if base >= rock_line {
                Material::Rock
            } else {
                Material::Grass
            };

            let base = base as usize;
            for y in 0..base {
                let material = if mask == 0 && y + 1 == base {
                    surface
                } else if y + 4 >= base {
                    Material::Dirt
                } else {
                    Material::Rock
                };
                world.set(x, y, z, Block::full(material));
            }
            if mask != 0 {
                world.set(
                    x,
                    base,
                    z,
                    Block {
                        material: surface,
                        shape: Shape(mask),
                    },
                );
            }
        }
    }
    world
}

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slopes_are_limited() {
        let p = GenParams {
            size: 64,
            ..Default::default()
        };
        let h = generate_heights(&p);
        let n = p.size + 1;
        for z in 0..n - 1 {
            for x in 0..n - 1 {
                let a = h[z * n + x] as i32;
                for (dx, dz) in [(1, 0), (0, 1), (1, 1)] {
                    assert!((a - h[(z + dz) * n + x + dx] as i32).abs() <= 1);
                }
                assert!((h[z * n + x + 1] as i32 - h[(z + 1) * n + x] as i32).abs() <= 1);
            }
        }
    }
}
