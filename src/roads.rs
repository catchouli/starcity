//! Roads: a per-tile bitmask of road pieces, and planning roads from a drag.
//!
//! Every piece runs between two "ports" of a tile: its centre and its four edge
//! midpoints. Half pieces join the centre to an edge, so straights, bends,
//! T-junctions and crossroads are combinations of halves. Diagonal pieces join
//! two adjacent edge midpoints across a corner; a 45° road is a staircase of
//! them. Because everything meets at edge midpoints, neighbouring tiles join up
//! wherever both have a piece touching their shared edge.

use crate::terrain::World;

pub const N: u8 = 1 << 0;
pub const E: u8 = 1 << 1;
pub const S: u8 = 1 << 2;
pub const W: u8 = 1 << 3;
pub const NE: u8 = 1 << 4;
pub const ES: u8 = 1 << 5;
pub const SW: u8 = 1 << 6;
pub const WN: u8 = 1 << 7;
pub const DIAGONALS: u8 = NE | ES | SW | WN;

/// Pieces that touch each edge's midpoint.
fn touching(edge: u8) -> u8 {
    match edge {
        N => N | NE | WN,
        E => E | NE | ES,
        S => S | ES | SW,
        W => W | SW | WN,
        _ => 0,
    }
}

/// The edge of a tile facing a neighbour offset.
fn edge_towards(dx: i32, dz: i32) -> u8 {
    match (dx.signum(), dz.signum()) {
        (1, 0) => E,
        (-1, 0) => W,
        (0, 1) => S,
        _ => N,
    }
}

/// The opposite edge.
fn opposite(edge: u8) -> u8 {
    match edge {
        N => S,
        S => N,
        E => W,
        _ => E,
    }
}

pub struct Roads {
    pub size_x: usize,
    pub size_z: usize,
    bits: Vec<u8>,
}

impl Roads {
    pub fn new(size_x: usize, size_z: usize) -> Roads {
        Roads {
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

/// Whether a set of road pieces can sit on a tile. Anything goes on flat ground.
/// On a slope, only straight pieces running up the slope fit, and nothing can
/// be built on water.
pub fn fits(world: &World, x: i32, z: i32, bits: u8) -> bool {
    let Some((base, shape)) = world.surface(x, z) else {
        return false;
    };
    if base < world.sea_level {
        return false;
    }
    if shape.0 == 0 {
        return true;
    }
    if bits & DIAGONALS != 0 {
        return false;
    }
    // Corners: 0 = NW, 1 = NE, 2 = SE, 3 = SW.
    let c = |cx, cz| shape.corner(cx, cz);
    let rises_along_z = c(0, 0) == c(1, 0) && c(0, 1) == c(1, 1);
    let rises_along_x = c(0, 0) == c(0, 1) && c(1, 0) == c(1, 1);
    (bits & (E | W) == 0 && rises_along_z) || (bits & (N | S) == 0 && rises_along_x)
}

/// Road pieces to add, per tile, split into those that fit and those that don't.
#[derive(Default, Debug, PartialEq)]
pub struct Plan {
    pub valid: Vec<((i32, i32), u8)>,
    pub blocked: Vec<((i32, i32), u8)>,
}

impl Plan {
    pub fn is_empty(&self) -> bool {
        self.valid.is_empty() && self.blocked.is_empty()
    }

    /// Adds the valid pieces to the road network. Returns the tiles changed.
    pub fn build(&self, roads: &mut Roads) -> Vec<(i32, i32)> {
        for &((x, z), bits) in &self.valid {
            roads.set(x, z, roads.get(x, z) | bits);
        }
        self.valid.iter().map(|&(t, _)| t).collect()
    }
}

/// Plans a road dragged from tile `a` to tile `b`. A drag mostly along one axis
/// gives a straight road along that axis; a drag closer to 45° gives a diagonal.
pub fn plan(world: &World, roads: &Roads, a: (i32, i32), b: (i32, i32)) -> Plan {
    let (dx, dz) = (b.0 - a.0, b.1 - a.1);
    let pieces = if dx == 0 && dz == 0 {
        Vec::new()
    } else if dx.abs() >= 2 * dz.abs() {
        straight(roads, a, (dx.signum(), 0), dx.abs())
    } else if dz.abs() >= 2 * dx.abs() {
        straight(roads, a, (0, dz.signum()), dz.abs())
    } else {
        let n = (dx.abs() + dz.abs() + 1) / 2;
        diagonal(a, (dx.signum(), dz.signum()), n)
    };

    let mut plan = Plan::default();
    for ((x, z), bits) in pieces {
        let combined = roads.get(x, z) | bits;
        if fits(world, x, z, combined) {
            plan.valid.push(((x, z), bits));
        } else {
            plan.blocked.push(((x, z), bits));
        }
    }
    plan
}

/// A straight road of `len + 1` tiles. Inner tiles get a full straight; the
/// end tiles get the half pointing inwards, plus the outward half if the road
/// beyond already reaches their shared edge.
fn straight(roads: &Roads, a: (i32, i32), (sx, sz): (i32, i32), len: i32) -> Vec<((i32, i32), u8)> {
    let forward = edge_towards(sx, sz);
    let back = opposite(forward);
    (0..=len)
        .map(|i| {
            let t = (a.0 + sx * i, a.1 + sz * i);
            let mut bits = 0;
            if i < len {
                bits |= forward;
            }
            if i > 0 {
                bits |= back;
            }
            // Join up with whatever lies just beyond each end.
            if i == 0 && roads.get(t.0 - sx, t.1 - sz) & touching(forward) != 0 {
                bits |= back;
            }
            if i == len && roads.get(t.0 + sx, t.1 + sz) & touching(back) != 0 {
                bits |= forward;
            }
            (t, bits)
        })
        .collect()
}

/// A 45° road of diagonal pieces from tile `a` towards `a + n * (sx, sz)`.
///
/// Works in half-tile units, walking edge midpoints: it starts at the midpoint
/// of `a`'s edge facing away from the z direction and steps half a tile
/// diagonally each time. Each pair of consecutive midpoints is one diagonal
/// piece in the tile between them.
fn diagonal(a: (i32, i32), (sx, sz): (i32, i32), n: i32) -> Vec<((i32, i32), u8)> {
    let mut p = (2 * a.0 + 1, 2 * a.1 + 1 - sz);
    let mut pieces = Vec::new();
    for _ in 0..2 * n + 1 {
        let q = (p.0 + sx, p.1 + sz);
        let tile = ((p.0 + q.0).div_euclid(4), (p.1 + q.1).div_euclid(4));
        let edge = |pt: (i32, i32)| match (pt.0 - 2 * tile.0, pt.1 - 2 * tile.1) {
            (1, 0) => N,
            (2, 1) => E,
            (1, 2) => S,
            _ => W,
        };
        let bit = match edge(p) | edge(q) {
            x if x == N | E => NE,
            x if x == E | S => ES,
            x if x == S | W => SW,
            _ => WN,
        };
        pieces.push((tile, bit));
        p = q;
    }
    pieces
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terrain::{Block, Material, Shape};

    fn flat_world(n: usize) -> World {
        let mut w = World::new(n, n, 4, 0);
        for x in 0..n {
            for z in 0..n {
                w.set(x, 0, z, Block::full(Material::Grass));
            }
        }
        w
    }

    #[test]
    fn straight_road_has_half_pieces_at_the_ends() {
        let w = flat_world(8);
        let r = Roads::new(8, 8);
        let p = plan(&w, &r, (1, 2), (4, 2));
        assert_eq!(
            p.valid,
            vec![((1, 2), E), ((2, 2), E | W), ((3, 2), E | W), ((4, 2), W)]
        );
        assert!(p.blocked.is_empty());
    }

    #[test]
    fn straight_road_joins_existing_road_into_a_t() {
        let w = flat_world(8);
        let mut r = Roads::new(8, 8);
        plan(&w, &r, (0, 4), (7, 4)).build(&mut r);
        // Drag a road north-south that stops on the existing east-west road.
        plan(&w, &r, (3, 0), (3, 4)).build(&mut r);
        assert_eq!(r.get(3, 4), E | W | N);
        // Continuing from a dead end turns it into a bend.
        plan(&w, &r, (5, 0), (5, 2)).build(&mut r);
        assert_eq!(r.get(5, 2), N);
        plan(&w, &r, (5, 2), (7, 2)).build(&mut r);
        assert_eq!(r.get(5, 2), N | E);
        // Starting right beside a road's side doesn't join it.
        let p = plan(&w, &r, (6, 3), (6, 1));
        assert_eq!(p.valid[0], ((6, 3), N));
    }

    #[test]
    fn diagonal_is_a_connected_staircase() {
        let w = flat_world(8);
        let r = Roads::new(8, 8);
        let p = plan(&w, &r, (1, 1), (3, 3));
        assert_eq!(
            p.valid,
            vec![
                ((1, 1), NE),
                ((2, 1), SW),
                ((2, 2), NE),
                ((3, 2), SW),
                ((3, 3), NE)
            ]
        );
        // Same in the opposite direction: pieces stay connected corner to corner.
        let p = plan(&w, &r, (3, 3), (1, 5));
        assert_eq!(p.valid.len(), 5);
        assert_eq!(p.valid.last().unwrap().0, (1, 5));
    }

    #[test]
    fn slopes_only_take_roads_running_up_them() {
        let mut w = flat_world(4);
        // Tile (1, 1) rises towards the south: its two southern corners are raised.
        w.set(
            1,
            1,
            1,
            Block {
                material: Material::Grass,
                shape: Shape(0b1100),
            },
        );
        assert!(fits(&w, 1, 1, N | S));
        assert!(!fits(&w, 1, 1, E | W));
        assert!(!fits(&w, 1, 1, NE));
        assert!(fits(&w, 0, 0, NE | E | W));
    }

    #[test]
    fn water_is_blocked() {
        let mut w = flat_world(4);
        w.sea_level = 5;
        let p = plan(&w, &Roads::new(4, 4), (0, 0), (2, 0));
        assert!(p.valid.is_empty());
        assert_eq!(p.blocked.len(), 3);
    }
}
