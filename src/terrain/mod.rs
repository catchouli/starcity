//! Voxel terrain: block storage, generation and meshing.
//!
//! The world is a grid of blocks. Horizontally one block is one tile (1.0 world
//! units); vertically one block is [`BLOCK_HEIGHT`] world units. Every block is
//! either air or a solid block with a [`Shape`] describing which of its four top
//! corners are raised. Underground blocks are full; surface blocks (and tunnel
//! ramps) can be sloped.

pub mod edit;
pub mod generation;
pub mod mesh;

/// Height of one block in world units (a tile is 1.0 wide).
pub const BLOCK_HEIGHT: f32 = 0.5;

/// Chunks are `CHUNK_SIZE` x `CHUNK_SIZE` columns, the full height of the world.
pub const CHUNK_SIZE: usize = 32;

#[repr(u8)]
#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Material {
    #[default]
    Air = 0,
    Grass = 1,
    Dirt = 2,
    Rock = 3,
    #[allow(dead_code)] // Not generated any more (beaches are sand); kept for quarries.
    Gravel = 4,
    // Clouds come in four tones, from light to rain-cloud grey.
    Cloud0 = 5,
    Cloud1 = 6,
    Cloud2 = 7,
    Cloud3 = 8,
    Sand = 9,
}

impl Material {
    pub fn cloud(tone: u8) -> Material {
        match tone {
            0 => Material::Cloud0,
            1 => Material::Cloud1,
            2 => Material::Cloud2,
            _ => Material::Cloud3,
        }
    }
}

/// Which top corners of a block are raised to the full block height, as a bitmask.
///
/// Corners are numbered anticlockwise when seen from above (x east, z south):
/// bit 0 = (x0, z0), bit 1 = (x1, z0), bit 2 = (x1, z1), bit 3 = (x0, z1).
/// `FULL` is an ordinary cube. A mask of 0 would have no volume and is never stored.
#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Shape(pub u8);

impl Shape {
    pub const FULL: Shape = Shape(0b1111);

    /// Local (x, z) offset of each corner bit.
    pub const CORNERS: [(u16, u16); 4] = [(0, 0), (1, 0), (1, 1), (0, 1)];

    pub fn is_full(self) -> bool {
        self.0 == 0b1111
    }

    /// Whether the top surface is split along the P1-P3 diagonal rather than P0-P2.
    /// For corner and inner-corner shapes the fold must avoid the odd corner out.
    pub fn splits_13(self) -> bool {
        let odd = match self.0.count_ones() {
            1 => self.0.trailing_zeros(),
            3 => (!self.0 & 0b1111).trailing_zeros(),
            _ => return false,
        };
        odd == 0 || odd == 2
    }

    /// Height (0 or 1 blocks above the block's floor) of the corner at local (cx, cz).
    pub fn corner(self, cx: u16, cz: u16) -> u16 {
        let bit = match (cx, cz) {
            (0, 0) => 0,
            (1, 0) => 1,
            (1, 1) => 2,
            _ => 3,
        };
        ((self.0 >> bit) & 1) as u16
    }
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Block {
    pub material: Material,
    pub shape: Shape,
}

impl Block {
    pub const AIR: Block = Block {
        material: Material::Air,
        shape: Shape(0),
    };

    pub fn full(material: Material) -> Block {
        Block {
            material,
            shape: Shape::FULL,
        }
    }

    pub fn is_air(self) -> bool {
        self.material == Material::Air
    }
}

/// Dense block storage for the whole map. Columns are contiguous (indexed
/// `[z][x][y]`) since almost everything walks up and down columns.
pub struct World {
    pub size_x: usize,
    pub size_z: usize,
    pub height: usize,
    /// Water surface level in blocks. Anything below it is underwater.
    pub sea_level: u16,
    /// Tiles below the sea level that stay dry: behind sea walls, or not flooded yet as the sea
    /// rises (`flood.rs`).
    pub dry: std::collections::HashSet<(i32, i32)>,
    blocks: Vec<Block>,
    /// Which world this is, and how many edits each chunk has had: together they change whenever
    /// a chunk's blocks do, so a save can reuse a chunk's encoding until then.
    stamp: u64,
    chunk_edits: Vec<u32>,
}

/// What a save keeps of the world besides its blocks.
#[derive(serde::Serialize, serde::Deserialize)]
pub struct Header {
    pub size_x: usize,
    pub size_z: usize,
    pub height: usize,
    pub sea_level: u16,
    pub dry: std::collections::HashSet<(i32, i32)>,
}

fn fresh_stamp() -> u64 {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

impl World {
    pub fn new(size_x: usize, size_z: usize, height: usize, sea_level: u16) -> World {
        World {
            size_x,
            size_z,
            height,
            sea_level,
            dry: Default::default(),
            blocks: vec![Block::AIR; size_x * size_z * height],
            stamp: fresh_stamp(),
            chunk_edits: vec![0; size_x.div_ceil(CHUNK_SIZE) * size_z.div_ceil(CHUNK_SIZE)],
        }
    }

    /// Whether a tile is under the sea: below its level and not kept dry.
    pub fn underwater(&self, x: i32, z: i32) -> bool {
        self.surface(x, z).is_none_or(|(base, _)| base < self.sea_level) && !self.dry.contains(&(x, z))
    }

    pub fn chunks_x(&self) -> usize {
        self.size_x.div_ceil(CHUNK_SIZE)
    }

    pub fn chunks_z(&self) -> usize {
        self.size_z.div_ceil(CHUNK_SIZE)
    }

    fn index(&self, x: usize, y: usize, z: usize) -> usize {
        (z * self.size_x + x) * self.height + y
    }

    /// Returns the block at a position; anything outside the map is air.
    #[inline]
    pub fn get(&self, x: i32, y: i32, z: i32) -> Block {
        if x < 0 || y < 0 || z < 0 {
            return Block::AIR;
        }
        let (x, y, z) = (x as usize, y as usize, z as usize);
        if x >= self.size_x || y >= self.height || z >= self.size_z {
            return Block::AIR;
        }
        self.blocks[self.index(x, y, z)]
    }

    pub fn set(&mut self, x: usize, y: usize, z: usize, block: Block) {
        let i = self.index(x, y, z);
        self.blocks[i] = block;
        let chunk = (z / CHUNK_SIZE) * self.chunks_x() + x / CHUNK_SIZE;
        self.chunk_edits[chunk] += 1;
    }

    /// Changes whenever any block in the chunk (`cz * chunks_x + cx`) does (see `stamp`).
    pub fn chunk_version(&self, chunk: usize) -> (u64, u32) {
        (self.stamp, self.chunk_edits[chunk])
    }

    /// For saving: everything but the blocks.
    pub fn header(&self) -> Header {
        Header {
            size_x: self.size_x,
            size_z: self.size_z,
            height: self.height,
            sea_level: self.sea_level,
            dry: self.dry.clone(),
        }
    }

    /// For saving: one chunk's blocks as runs of the same block, column by column. Columns are mostly
    /// a few long runs, so this is small, and smaller still compressed.
    pub fn chunk_runs(&self, chunk: usize) -> Vec<(Block, u32)> {
        let (cx, cz) = (chunk % self.chunks_x(), chunk / self.chunks_x());
        let mut runs: Vec<(Block, u32)> = Vec::new();
        for z in cz * CHUNK_SIZE..((cz + 1) * CHUNK_SIZE).min(self.size_z) {
            for x in cx * CHUNK_SIZE..((cx + 1) * CHUNK_SIZE).min(self.size_x) {
                for &b in self.column(x, z) {
                    match runs.last_mut() {
                        Some((last, n)) if *last == b => *n += 1,
                        _ => runs.push((b, 1)),
                    }
                }
            }
        }
        runs
    }

    /// A saved world: its header and each chunk's runs, in order. `None` if they don't fit together.
    pub fn from_saved(header: Header, chunks: Vec<Vec<(Block, u32)>>) -> Option<World> {
        let mut w = World::new(header.size_x, header.size_z, header.height, header.sea_level);
        w.dry = header.dry;
        if chunks.len() != w.chunk_edits.len() {
            return None;
        }
        for (chunk, runs) in chunks.into_iter().enumerate() {
            let (cx, cz) = (chunk % w.chunks_x(), chunk / w.chunks_x());
            let mut blocks = runs.into_iter().flat_map(|(b, n)| std::iter::repeat_n(b, n as usize));
            for z in cz * CHUNK_SIZE..((cz + 1) * CHUNK_SIZE).min(w.size_z) {
                for x in cx * CHUNK_SIZE..((cx + 1) * CHUNK_SIZE).min(w.size_x) {
                    let start = w.index(x, 0, z);
                    for i in start..start + w.height {
                        w.blocks[i] = blocks.next()?;
                    }
                }
            }
            if blocks.next().is_some() {
                return None;
            }
        }
        Some(w)
    }

    /// The blocks of one column, bottom to top.
    #[inline]
    pub fn column(&self, x: usize, z: usize) -> &[Block] {
        let start = self.index(x, 0, z);
        &self.blocks[start..start + self.height]
    }

    pub fn in_bounds(&self, x: i32, z: i32) -> bool {
        x >= 0 && z >= 0 && (x as usize) < self.size_x && (z as usize) < self.size_z
    }

    /// The topmost solid block of a column and its y.
    pub fn top_block(&self, x: i32, z: i32) -> Option<(usize, Block)> {
        if !self.in_bounds(x, z) {
            return None;
        }
        let col = self.column(x as usize, z as usize);
        col.iter().rposition(|b| !b.is_air()).map(|y| (y, col[y]))
    }

    /// The surface of a column: its floor height in blocks and the shape on top
    /// of it. A flat top is reported as the full block's top with an empty shape.
    pub fn surface(&self, x: i32, z: i32) -> Option<(u16, Shape)> {
        self.top_block(x, z).map(|(y, b)| {
            if b.shape.is_full() {
                (y as u16 + 1, Shape(0))
            } else {
                (y as u16, b.shape)
            }
        })
    }

    /// World-space height of the terrain surface at any point, following the
    /// same triangles the mesher draws.
    pub fn height_at(&self, x: f32, z: f32) -> f32 {
        let (tx, tz) = (x.floor() as i32, z.floor() as i32);
        let Some((base, shape)) = self.surface(tx, tz) else {
            return 0.0;
        };
        let (fx, fz) = (x - tx as f32, z - tz as f32);
        let c = |cx, cz| (base + shape.corner(cx, cz)) as f32;
        let (c00, c10, c11, c01) = (c(0, 0), c(1, 0), c(1, 1), c(0, 1));
        let h = if !shape.splits_13() {
            // Split along (0,0)-(1,1).
            if fx >= fz {
                c00 + (c10 - c00) * fx + (c11 - c10) * fz
            } else {
                c00 + (c01 - c00) * fz + (c11 - c01) * fx
            }
        } else if fx + fz <= 1.0 {
            // Split along (1,0)-(0,1).
            c00 + (c10 - c00) * fx + (c01 - c00) * fz
        } else {
            c11 + (c01 - c11) * (1.0 - fx) + (c10 - c11) * (1.0 - fz)
        };
        h * BLOCK_HEIGHT
    }

    /// World-space height of the terrain surface at the centre of a tile.
    pub fn surface_height(&self, x: i32, z: i32) -> f32 {
        self.surface(x, z).map_or(0.0, |(base, shape)| {
            (base as f32 + shape.0.count_ones() as f32 / 4.0) * BLOCK_HEIGHT
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn height_follows_slopes() {
        let mut w = World::new(3, 3, 4, 0);
        w.set(0, 0, 0, Block::full(Material::Grass));
        // Tile (1, 0): raised along its east edge (corners NE and SE).
        w.set(
            1,
            0,
            0,
            Block {
                material: Material::Grass,
                shape: Shape(0b0110),
            },
        );
        assert_eq!(w.height_at(0.5, 0.5), BLOCK_HEIGHT);
        assert_eq!(w.height_at(1.0, 0.5), 0.0);
        assert!((w.height_at(1.5, 0.3) - 0.5 * BLOCK_HEIGHT).abs() < 1e-6);
        assert!((w.height_at(1.99, 0.9) - 0.99 * BLOCK_HEIGHT).abs() < 1e-5);
    }
}
