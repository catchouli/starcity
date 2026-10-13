//! Chunk meshing.
//!
//! Every terrain vertex sits on the integer block grid, so a vertex is just four
//! `u16`s (8 bytes): x, y, z in block units plus a data word holding the material
//! and a flag marking the top edge of side faces (used for the grass fringe).
//! Normals are not stored; the shader derives flat normals from screen-space
//! derivatives. All geometry is quads (4 vertices) drawn with one shared index
//! buffer.

use super::{Block, CHUNK_SIZE, Shape, World};

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub x: u16,
    pub y: u16,
    pub z: u16,
    pub data: u16,
}

/// Material id used for water surfaces in the vertex data word.
pub const WATER: u16 = 15;
const EDGE_BIT: u16 = 1 << 4;

#[derive(Default)]
pub struct ChunkMesh {
    /// Opaque terrain quads.
    pub terrain: Vec<Vertex>,
    /// Transparent water quads, drawn after terrain.
    pub water: Vec<Vertex>,
    /// Bounding box in block coordinates.
    pub min: [u16; 3],
    pub max: [u16; 3],
}

/// Side faces: neighbour offset and the two top corners of that side, ordered
/// so that [A bottom, B bottom, B top, A top] winds anticlockwise from outside.
/// A side face: the neighbour it faces, and its two bottom corners.
type Side = ((i32, i32), (u16, u16), (u16, u16));

const SIDES: [Side; 4] = [
    ((-1, 0), (0, 0), (0, 1)),
    ((1, 0), (1, 1), (1, 0)),
    ((0, 1), (0, 1), (1, 1)),
    ((0, -1), (1, 0), (0, 0)),
];

struct Builder {
    terrain: Vec<Vertex>,
    water: Vec<Vertex>,
}

impl Builder {
    fn quad(out: &mut Vec<Vertex>, corners: [(u16, u16, u16, bool); 4], data: u16) {
        for (x, y, z, edge) in corners {
            out.push(Vertex {
                x,
                y,
                z,
                data: data | if edge { EDGE_BIT } else { 0 },
            });
        }
    }
}

pub fn mesh_chunk(world: &World, cx: usize, cz: usize) -> ChunkMesh {
    let x0 = cx * CHUNK_SIZE;
    let z0 = cz * CHUNK_SIZE;
    let x1 = (x0 + CHUNK_SIZE).min(world.size_x);
    let z1 = (z0 + CHUNK_SIZE).min(world.size_z);

    let mut b = Builder {
        terrain: Vec::new(),
        water: Vec::new(),
    };
    // Flat tops waiting to be greedily merged: (top y, data word), one per column.
    let mut flat: Vec<Option<(u16, u16)>> = vec![None; CHUNK_SIZE * CHUNK_SIZE];
    let mut water: Vec<Option<(u16, u16)>> = vec![None; CHUNK_SIZE * CHUNK_SIZE];
    let sea = world.sea_level;

    // Per-column stats for the chunk plus a one-column border:
    // (number of full blocks from the bottom up, index above the topmost solid block).
    let bw = x1 - x0 + 2;
    let mut stats = vec![(0usize, 0usize); bw * (z1 - z0 + 2)];
    for bz in 0..z1 - z0 + 2 {
        for bx in 0..bw {
            let (x, z) = (x0 as i32 + bx as i32 - 1, z0 as i32 + bz as i32 - 1);
            if x < 0 || z < 0 || x as usize >= world.size_x || z as usize >= world.size_z {
                continue;
            }
            let col = world.column(x as usize, z as usize);
            let solid = col.iter().take_while(|b| !b.is_air() && b.shape.is_full()).count();
            let top = col.iter().rposition(|b| !b.is_air()).map_or(0, |y| y + 1);
            stats[bz * bw + bx] = (solid, top);
        }
    }

    for z in z0..z1 {
        for x in x0..x1 {
            let (xi, zi) = (x as i32, z as i32);
            let (xu, zu) = (x as u16, z as u16);
            // Blocks below every neighbour's solid core (and with a full block above)
            // can't have a visible face, so start scanning just below the lowest one.
            let at = |dx: usize, dz: usize| stats[(z - z0 + dz) * bw + (x - x0 + dx)];
            let (own_solid, own_top) = at(1, 1);
            let start = [
                at(0, 1).0,
                at(2, 1).0,
                at(1, 0).0,
                at(1, 2).0,
                own_solid.saturating_sub(1),
            ]
            .into_iter()
            .min()
            .unwrap()
            .saturating_sub(1);
            for y in start..own_top {
                let yi = y as i32;
                let yu = y as u16;
                let block = world.get(xi, yi, zi);
                if block.is_air() {
                    continue;
                }
                let data = block.material as u16;
                let shape = block.shape;

                // Top
                if shape.is_full() {
                    if world.get(xi, yi + 1, zi).is_air() {
                        let cell = &mut flat[(z - z0) * CHUNK_SIZE + (x - x0)];
                        if let Some((ty, td)) = cell.replace((yu + 1, data)) {
                            // A lower flat top in the same column (under an overhang): emit as-is.
                            flat_quad(&mut b.terrain, xu, zu, xu + 1, zu + 1, ty, td);
                        }
                    }
                } else {
                    top_quad(&mut b.terrain, xu, yu, zu, shape, data);
                }

                // Bottom
                if y > 0 {
                    let below = world.get(xi, yi - 1, zi);
                    if below.is_air() || !below.shape.is_full() {
                        Builder::quad(
                            &mut b.terrain,
                            [
                                (xu, yu, zu, false),
                                (xu + 1, yu, zu, false),
                                (xu + 1, yu, zu + 1, false),
                                (xu, yu, zu + 1, false),
                            ],
                            data,
                        );
                    }
                }

                // Sides
                for ((dx, dz), a, bc) in SIDES {
                    let nb = world.get(xi + dx, yi, zi + dz);
                    let ha = shape.corner(a.0, a.1);
                    let hb = shape.corner(bc.0, bc.1);
                    if !nb.is_air() && covers(nb, (dx, dz), a, ha) && covers(nb, (dx, dz), bc, hb) {
                        continue;
                    }
                    Builder::quad(
                        &mut b.terrain,
                        [
                            (xu + a.0, yu, zu + a.1, false),
                            (xu + bc.0, yu, zu + bc.1, false),
                            (xu + bc.0, yu + hb, zu + bc.1, hb > 0),
                            (xu + a.0, yu + ha, zu + a.1, ha > 0),
                        ],
                        data,
                    );
                }
            }

            // Water
            // Water surface over anything below sea level. (No side faces at the
            // map edge: the endless ocean continues beyond it.)
            let base = world.surface(xi, zi).map_or(0, |(base, _)| base);
            if base < sea && !world.dry.contains(&(xi, zi)) {
                water[(z - z0) * CHUNK_SIZE + (x - x0)] = Some((sea, WATER));
            }
        }
    }

    greedy_flat(&mut b.terrain, &mut flat, x0 as u16, z0 as u16);
    greedy_flat(&mut b.water, &mut water, x0 as u16, z0 as u16);

    let mut mesh = ChunkMesh {
        terrain: b.terrain,
        water: b.water,
        min: [u16::MAX; 3],
        max: [0; 3],
    };
    for v in mesh.terrain.iter().chain(mesh.water.iter()) {
        for (i, c) in [v.x, v.y, v.z].into_iter().enumerate() {
            mesh.min[i] = mesh.min[i].min(c);
            mesh.max[i] = mesh.max[i].max(c);
        }
    }
    mesh
}

/// Whether neighbour `nb` (offset by `dir`) is at least as tall as our corner `c`
/// (with height `h`) along the shared edge.
fn covers(nb: Block, dir: (i32, i32), c: (u16, u16), h: u16) -> bool {
    let ncx = (c.0 as i32 - dir.0) as u16;
    let ncz = (c.1 as i32 - dir.1) as u16;
    nb.shape.corner(ncx, ncz) >= h
}

/// The sloped top of a shaped block, split along the diagonal that keeps the
/// fold in the right place for corner and inner-corner shapes.
fn top_quad(out: &mut Vec<Vertex>, x: u16, y: u16, z: u16, shape: Shape, data: u16) {
    let p = Shape::CORNERS.map(|(cx, cz)| (x + cx, y + shape.corner(cx, cz), z + cz, false));
    // Anticlockwise from above is P0, P3, P2, P1; indices (0,2) form the diagonal.
    let corners = if shape.splits_13() {
        [p[3], p[2], p[1], p[0]]
    } else {
        [p[0], p[3], p[2], p[1]]
    };
    Builder::quad(out, corners, data);
}

fn flat_quad(out: &mut Vec<Vertex>, xa: u16, za: u16, xb: u16, zb: u16, y: u16, data: u16) {
    Builder::quad(
        out,
        [
            (xa, y, za, false),
            (xa, y, zb, false),
            (xb, y, zb, false),
            (xb, y, za, false),
        ],
        data,
    );
}

/// Greedily merges same-height, same-material flat tops into rectangles.
fn greedy_flat(out: &mut Vec<Vertex>, cells: &mut [Option<(u16, u16)>], x0: u16, z0: u16) {
    let n = CHUNK_SIZE;
    for z in 0..n {
        let mut x = 0;
        while x < n {
            let Some(key) = cells[z * n + x] else {
                x += 1;
                continue;
            };
            let mut w = 1;
            while x + w < n && cells[z * n + x + w] == Some(key) {
                w += 1;
            }
            let mut h = 1;
            'grow: while z + h < n {
                for i in 0..w {
                    if cells[(z + h) * n + x + i] != Some(key) {
                        break 'grow;
                    }
                }
                h += 1;
            }
            for dz in 0..h {
                for i in 0..w {
                    cells[(z + dz) * n + x + i] = None;
                }
            }
            let (xa, za) = (x0 + x as u16, z0 + z as u16);
            flat_quad(out, xa, za, xa + w as u16, za + h as u16, key.0, key.1);
            x += w;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terrain::Material;

    #[test]
    fn single_block_has_six_faces() {
        let mut w = World::new(4, 4, 4, 0);
        w.set(1, 1, 1, Block::full(Material::Rock));
        let m = mesh_chunk(&w, 0, 0);
        assert_eq!(m.terrain.len(), 6 * 4);
    }

    #[test]
    fn buried_block_has_no_faces() {
        let mut w = World::new(3, 3, 3, 0);
        for x in 0..3 {
            for y in 0..3 {
                for z in 0..3 {
                    w.set(x, y, z, Block::full(Material::Rock));
                }
            }
        }
        let m = mesh_chunk(&w, 0, 0);
        // 3x3 top merges to one quad; 4 sides of 3x3 faces; bottom at y=0 is skipped.
        assert_eq!(m.terrain.len() / 4, 1 + 4 * 9);
    }
}

#[cfg(test)]
mod bench {
    use super::*;
    use crate::terrain::generation::{GenParams, generate};

    /// `cargo test --release mesh_timing -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn mesh_timing() {
        let w = generate(&GenParams::default());
        let t = std::time::Instant::now();
        let mut quads = 0;
        for cz in 0..w.chunks_z() {
            for cx in 0..w.chunks_x() {
                let m = mesh_chunk(&w, cx, cz);
                quads += (m.terrain.len() + m.water.len()) / 4;
            }
        }
        println!("meshed {} quads in {:?}", quads, t.elapsed());
    }
}
