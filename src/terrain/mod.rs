//! Voxel terrain: block storage, generation and meshing.
//!
//! The world is a grid of blocks. Horizontally one block is one tile (1.0 world
//! units); vertically one block is [`BLOCK_HEIGHT`] world units. Every block is
//! either air or a solid block with a [`Shape`] describing which of its four top
//! corners are raised. Underground blocks are full; surface blocks (and tunnel
//! ramps) can be sloped.

pub mod generation;
pub mod mesh;

/// Height of one block in world units (a tile is 1.0 wide).
pub const BLOCK_HEIGHT: f32 = 0.5;

/// Chunks are `CHUNK_SIZE` x `CHUNK_SIZE` columns, the full height of the world.
pub const CHUNK_SIZE: usize = 32;

#[repr(u8)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Material {
    #[default]
    Air = 0,
    Grass = 1,
    Dirt = 2,
    Rock = 3,
    Gravel = 4,
}

/// Which top corners of a block are raised to the full block height, as a bitmask.
///
/// Corners are numbered anticlockwise when seen from above (x east, z south):
/// bit 0 = (x0, z0), bit 1 = (x1, z0), bit 2 = (x1, z1), bit 3 = (x0, z1).
/// `FULL` is an ordinary cube. A mask of 0 would have no volume and is never stored.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
    blocks: Vec<Block>,
}

impl World {
    pub fn new(size_x: usize, size_z: usize, height: usize, sea_level: u16) -> World {
        World {
            size_x,
            size_z,
            height,
            sea_level,
            blocks: vec![Block::AIR; size_x * size_z * height],
        }
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

    /// World-space height of the terrain surface at the centre of a tile.
    pub fn surface_height(&self, x: i32, z: i32) -> f32 {
        self.surface(x, z).map_or(0.0, |(base, shape)| {
            (base as f32 + shape.0.count_ones() as f32 / 4.0) * BLOCK_HEIGHT
        })
    }
}
