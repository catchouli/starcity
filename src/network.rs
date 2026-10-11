//! Shared machinery for tile networks (roads and rails): per-tile bit layers,
//! tile edges, diagonal walks, and all-or-nothing build plans.

use crate::terrain::World;

/// Tile edges. These double as the road "half" bits (centre to that edge).
pub const N: u8 = 1 << 0;
pub const E: u8 = 1 << 1;
pub const S: u8 = 1 << 2;
pub const W: u8 = 1 << 3;

/// The edge of a tile facing a neighbour offset (only the sign matters).
pub fn edge_towards(dx: i32, dz: i32) -> u8 {
    match (dx.signum(), dz.signum()) {
        (1, 0) => E,
        (-1, 0) => W,
        (0, 1) => S,
        _ => N,
    }
}

pub fn opposite(edge: u8) -> u8 {
    match edge {
        N => S,
        S => N,
        E => W,
        _ => E,
    }
}

/// An edge's midpoint in half-tile units.
pub fn midpoint(tile: (i32, i32), edge: u8) -> (i32, i32) {
    let (x, z) = (2 * tile.0, 2 * tile.1);
    match edge {
        N => (x + 1, z),
        E => (x + 2, z + 1),
        S => (x + 1, z + 2),
        _ => (x, z + 1),
    }
}

/// One segment of a diagonal walk: the tile it crosses and the edges it
/// enters and leaves by.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Step {
    pub tile: (i32, i32),
    pub enter: u8,
    pub leave: u8,
}

/// Walks a 45° line through edge midpoints, starting at `start` (half-tile
/// units) and stepping half a tile in `(sx, sz)` each time, until it has
/// crossed tile `end`. Consecutive midpoints always lie on adjacent edges of
/// the tile between them, so each step is one corner-crossing piece.
pub fn diagonal_walk(start: (i32, i32), (sx, sz): (i32, i32), end: (i32, i32)) -> Vec<Step> {
    let mut p = start;
    let mut steps = Vec::new();
    let limit = 4 * ((end.0 - start.0 / 2).abs() + (end.1 - start.1 / 2).abs()) + 8;
    for _ in 0..limit {
        let q = (p.0 + sx, p.1 + sz);
        let tile = ((p.0 + q.0).div_euclid(4), (p.1 + q.1).div_euclid(4));
        let edge = |pt: (i32, i32)| match (pt.0 - 2 * tile.0, pt.1 - 2 * tile.1) {
            (1, 0) => N,
            (2, 1) => E,
            (1, 2) => S,
            _ => W,
        };
        steps.push(Step {
            tile,
            enter: edge(p),
            leave: edge(q),
        });
        if tile == end {
            break;
        }
        p = q;
    }
    steps
}

/// A per-tile bitmask layer covering the map.
pub struct Layer {
    pub size_x: usize,
    pub size_z: usize,
    bits: Vec<u8>,
}

impl Layer {
    pub fn new(size_x: usize, size_z: usize) -> Layer {
        Layer {
            size_x,
            size_z,
            bits: vec![0; size_x * size_z],
        }
    }

    pub fn get(&self, x: i32, z: i32) -> u8 {
        if x < 0 || z < 0 || x as usize >= self.size_x || z as usize >= self.size_z {
            return 0;
        }
        self.bits[z as usize * self.size_x + x as usize]
    }

    pub fn set(&mut self, x: i32, z: i32, bits: u8) {
        self.bits[z as usize * self.size_x + x as usize] = bits;
    }
}

/// A change to one tile: bits to remove, then bits to add.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Piece {
    pub tile: (i32, i32),
    pub add: u8,
    pub remove: u8,
}

impl Piece {
    pub fn add(tile: (i32, i32), add: u8) -> Piece {
        Piece { tile, add, remove: 0 }
    }
}

/// Why a plan can't be built.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Blocked {
    Water,
    Slope,
    Occupied,
}

/// A dragged-out piece of network. All or nothing: if any piece doesn't fit,
/// none of it is built.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Plan {
    pub pieces: Vec<Piece>,
    pub blocked: Option<Blocked>,
}

impl Plan {
    pub fn is_empty(&self) -> bool {
        self.pieces.is_empty()
    }

    pub fn tiles(&self) -> impl Iterator<Item = (i32, i32)> + '_ {
        self.pieces.iter().map(|p| p.tile)
    }

    /// The tile's bits after this plan, if it touches the tile.
    pub fn result(&self, layer: &Layer, tile: (i32, i32)) -> Option<u8> {
        let mut bits = None;
        for p in self.pieces.iter().filter(|p| p.tile == tile) {
            let b = bits.unwrap_or(layer.get(tile.0, tile.1));
            bits = Some((b & !p.remove) | p.add);
        }
        bits
    }

    /// Applies the plan if nothing is blocked. Returns the tiles changed.
    pub fn build(&self, layer: &mut Layer) -> Vec<(i32, i32)> {
        if self.blocked.is_some() {
            return Vec::new();
        }
        let tiles: Vec<_> = self.tiles().collect();
        for &t in &tiles {
            let bits = self.result(layer, t).unwrap();
            layer.set(t.0, t.1, bits);
        }
        tiles
    }
}

/// Whether a tile's slope allows the given kinds of piece: anything on flat
/// ground; on a slope only straight pieces running up it; nothing on water.
pub fn slope_check(world: &World, x: i32, z: i32, along_x: bool, along_z: bool, diagonal: bool) -> Result<(), Blocked> {
    let Some((base, shape)) = world.surface(x, z) else {
        return Err(Blocked::Water);
    };
    if base < world.sea_level {
        return Err(Blocked::Water);
    }
    if shape.0 == 0 {
        return Ok(());
    }
    // Corners: (0,0) = NW, (1,0) = NE, (1,1) = SE, (0,1) = SW.
    let c = |cx, cz| shape.corner(cx, cz);
    let rises_along_z = c(0, 0) == c(1, 0) && c(0, 1) == c(1, 1);
    let rises_along_x = c(0, 0) == c(0, 1) && c(1, 0) == c(1, 1);
    let ok = !diagonal && ((!along_x && rises_along_z) || (!along_z && rises_along_x));
    if ok { Ok(()) } else { Err(Blocked::Slope) }
}
