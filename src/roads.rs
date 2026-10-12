//! Roads: a per-tile bitmask of road pieces, and planning roads from a drag.
//!
//! Every piece runs between two "ports" of a tile: its centre and its four edge
//! midpoints. Half pieces join the centre to an edge, so straights, bends,
//! T-junctions and crossroads are combinations of halves. Diagonal pieces join
//! two adjacent edge midpoints across a corner; a 45° road is a staircase of
//! them. Because everything meets at edge midpoints, neighbouring tiles join up
//! wherever both have a piece touching their shared edge.

use crate::network::{self, Blocked, Layer, Piece, Plan, edge_towards, midpoint, opposite};
use crate::terrain::World;

pub use crate::network::{E, N, S, W};
pub const NE: u8 = 1 << 4;
pub const ES: u8 = 1 << 5;
pub const SW: u8 = 1 << 6;
pub const WN: u8 = 1 << 7;
pub const HALVES: u8 = N | E | S | W;
pub const DIAGONALS: u8 = NE | ES | SW | WN;

/// The diagonal piece joining two adjacent edges.
fn diagonal_piece(a: u8, b: u8) -> u8 {
    match a | b {
        x if x == N | E => NE,
        x if x == E | S => ES,
        x if x == S | W => SW,
        _ => WN,
    }
}

/// Pieces that touch each edge's midpoint.
fn touching(edge: u8) -> u8 {
    edge | match edge {
        N => NE | WN,
        E => NE | ES,
        S => ES | SW,
        _ => SW | WN,
    }
}

/// A dead end: exactly one half and nothing else.
fn dead_end(bits: u8) -> Option<u8> {
    (bits.count_ones() == 1 && bits & HALVES != 0).then_some(bits)
}

/// Whether a tile could hold these road pieces.
pub fn check(world: &World, rails: &Layer, buildings: &Layer, x: i32, z: i32, bits: u8) -> Result<(), Blocked> {
    if rails.get(x, z) != 0 || buildings.get(x, z) != 0 {
        return Err(Blocked::Occupied);
    }
    network::slope_check(
        world,
        x,
        z,
        bits & (E | W) != 0,
        bits & (N | S) != 0,
        bits & DIAGONALS != 0,
    )
}

/// Plans a road dragged from tile `a` to tile `b`. A drag mostly along one axis
/// gives a straight road along that axis; a drag closer to 45° gives a diagonal.
pub fn plan(world: &World, roads: &Layer, rails: &Layer, buildings: &Layer, a: (i32, i32), b: (i32, i32)) -> Plan {
    let (dx, dz) = (b.0 - a.0, b.1 - a.1);
    let pieces = if dx == 0 && dz == 0 {
        Vec::new()
    } else if dx.abs() >= 2 * dz.abs() {
        straight(roads, a, (dx.signum(), 0), dx.abs())
    } else if dz.abs() >= 2 * dx.abs() {
        straight(roads, a, (0, dz.signum()), dz.abs())
    } else {
        let n = (dx.abs() + dz.abs() + 1) / 2;
        diagonal(roads, a, (dx.signum(), dz.signum()), n)
    };
    let mut plan = Plan::from_pieces(pieces);
    let tiles: Vec<_> = plan.tiles().collect();
    for t in tiles {
        let bits = plan.result(roads, t).unwrap();
        if let Err(why) = check(world, rails, buildings, t.0, t.1, bits) {
            plan.blocked = Some(why);
            break;
        }
    }
    plan
}

/// A straight road of `len + 1` tiles. Inner tiles get a full straight; the
/// end tiles get the half pointing inwards, plus the outward half if the road
/// beyond already reaches their shared edge.
fn straight(roads: &Layer, a: (i32, i32), (sx, sz): (i32, i32), len: i32) -> Vec<Piece> {
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
            // An end tile holding only diagonal pieces that already reach the edge
            // the straight leaves by needs no half: the road joins at that edge.
            let existing = roads.get(t.0, t.1);
            if existing & HALVES == 0 {
                if i == 0 && existing & touching(forward) != 0 {
                    bits &= !forward;
                }
                if i == len && existing & touching(back) != 0 {
                    bits &= !back;
                }
            }
            Piece::add(t, bits)
        })
        .filter(|p| p.add != 0)
        .collect()
}

/// A 45° road of diagonal pieces from tile `a` to `a + n * (sx, sz)`.
///
/// Where it starts depends on what's already at `a`:
/// - A dead end whose road arrives from behind the drag turns into a diagonal
///   piece itself: the road bends 45° at the edge where it enters the tile.
/// - Other straight road carries on to the tile's far edge along the axis it
///   was already running, then turns 45° there.
/// - Otherwise the first diagonal piece crosses the corner of `a`.
///
/// At the far end, a diagonal that leaves through a dead end's open edge
/// replaces that stub; other straight road gets a half from the edge the
/// diagonal arrives at.
fn diagonal(roads: &Layer, a: (i32, i32), (sx, sz): (i32, i32), n: i32) -> Vec<Piece> {
    let b = (a.0 + n * sx, a.1 + n * sz);
    let (back_x, back_z) = (edge_towards(-sx, 0), edge_towards(0, -sz));
    let at_a = roads.get(a.0, a.1);
    let mut pieces = Vec::new();

    let mut replace_at_a = 0;
    let starts = match dead_end(at_a) {
        Some(h) if h == back_x || h == back_z => {
            replace_at_a = h;
            vec![midpoint(a, h)]
        }
        _ if at_a & HALVES != 0 => {
            let exit = if at_a & back_x != 0 {
                edge_towards(sx, 0)
            } else if at_a & back_z != 0 {
                edge_towards(0, sz)
            } else {
                edge_towards(sx, 0)
            };
            pieces.push(Piece::add(a, exit));
            vec![midpoint(a, exit)]
        }
        // From an empty tile either of two lines works: prefer one that leaves
        // through a dead end's open edge at the far end.
        _ => vec![midpoint(a, back_z), midpoint(a, back_x)],
    };
    let at_b = roads.get(b.0, b.1);
    let walks: Vec<_> = starts
        .into_iter()
        .map(|p| network::diagonal_walk(p, (sx, sz), b))
        .collect();
    let joins = |walk: &[network::Step]| walk.last().is_some_and(|step| dead_end(at_b) == Some(step.leave));
    let walk = walks.iter().find(|w| joins(w)).unwrap_or(&walks[0]);

    for step in walk {
        let bit = diagonal_piece(step.enter, step.leave);
        let piece = if step.tile == a {
            Piece {
                tile: a,
                add: bit,
                remove: replace_at_a,
            }
        } else if step.tile == b {
            match dead_end(at_b) {
                Some(h) if h == step.leave => Piece {
                    tile: b,
                    add: bit,
                    remove: h,
                },
                _ if at_b & HALVES != 0 => Piece::add(b, step.enter),
                _ => Piece::add(b, bit),
            }
        } else {
            Piece::add(step.tile, bit)
        };
        pieces.push(piece);
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

    fn build(w: &World, r: &mut Layer, a: (i32, i32), b: (i32, i32)) {
        let p = plan(w, r, &Layer::new(r.size_x, r.size_z), &Layer::new(64, 64), a, b);
        assert!(p.blocked.is_none(), "{a:?} -> {b:?} blocked: {:?}", p.blocked);
        p.build(r);
    }

    fn adds(p: &Plan) -> Vec<((i32, i32), u8)> {
        p.pieces.iter().map(|p| (p.tile, p.add)).collect()
    }

    #[test]
    fn straight_road_has_half_pieces_at_the_ends() {
        let w = flat_world(8);
        let r = Layer::new(8, 8);
        let p = plan(&w, &r, &Layer::new(8, 8), &Layer::new(64, 64), (1, 2), (4, 2));
        assert_eq!(
            adds(&p),
            vec![((1, 2), E), ((2, 2), E | W), ((3, 2), E | W), ((4, 2), W)]
        );
    }

    #[test]
    fn straight_road_joins_existing_road_into_a_t() {
        let w = flat_world(8);
        let mut r = Layer::new(8, 8);
        build(&w, &mut r, (0, 4), (7, 4));
        build(&w, &mut r, (3, 0), (3, 4));
        assert_eq!(r.get(3, 4), E | W | N);
        // Continuing from a dead end turns it into a bend.
        build(&w, &mut r, (5, 0), (5, 2));
        build(&w, &mut r, (5, 2), (7, 2));
        assert_eq!(r.get(5, 2), N | E);
    }

    #[test]
    fn diagonal_is_a_connected_staircase() {
        let w = flat_world(8);
        let r = Layer::new(8, 8);
        let p = plan(&w, &r, &Layer::new(8, 8), &Layer::new(64, 64), (1, 1), (3, 3));
        assert_eq!(
            adds(&p),
            vec![((1, 1), NE), ((2, 1), SW), ((2, 2), NE), ((3, 2), SW), ((3, 3), NE)]
        );
    }

    #[test]
    fn dead_end_turns_into_a_diagonal() {
        let w = flat_world(12);
        let mut r = Layer::new(12, 12);
        build(&w, &mut r, (1, 2), (4, 2));
        assert_eq!(r.get(4, 2), W);
        // Drag diagonally (south-east) from the road's last tile.
        build(&w, &mut r, (4, 2), (7, 5));
        assert_eq!(r.get(4, 2), SW, "the dead end becomes a 45° bend");
        assert_eq!(r.get(4, 3), NE, "and carries straight on diagonally");
        assert_eq!(r.get(7, 5) & DIAGONALS, r.get(7, 5));
    }

    #[test]
    fn through_road_carries_on_to_the_edge_then_turns() {
        let w = flat_world(12);
        let mut r = Layer::new(12, 12);
        build(&w, &mut r, (1, 2), (6, 2));
        build(&w, &mut r, (3, 2), (6, 5));
        assert_eq!(r.get(3, 2), E | W, "no 90° turn: it leaves along the road's own axis");
        assert_eq!(r.get(4, 2), E | W | SW, "and branches off diagonally from that edge");
    }

    #[test]
    fn diagonal_arriving_at_a_dead_end_replaces_the_stub() {
        let w = flat_world(12);
        let mut r = Layer::new(12, 12);
        build(&w, &mut r, (4, 4), (8, 4));
        assert_eq!(r.get(4, 4), E);
        build(&w, &mut r, (1, 1), (4, 4));
        assert_eq!(r.get(4, 4), NE, "the diagonal leaves through the dead end's open edge");
    }

    #[test]
    fn straight_leaves_a_diagonal_from_its_edge() {
        let w = flat_world(12);
        let mut r = Layer::new(12, 12);
        build(&w, &mut r, (1, 1), (3, 3));
        build(&w, &mut r, (3, 3), (6, 3));
        assert_eq!(r.get(3, 3), NE, "no stub back to the centre");
        assert_eq!(r.get(4, 3), W | E);
    }

    #[test]
    fn continuing_a_diagonal_stays_on_its_line() {
        let w = flat_world(12);
        let mut r = Layer::new(12, 12);
        build(&w, &mut r, (1, 1), (3, 3));
        build(&w, &mut r, (3, 3), (5, 5));
        assert_eq!(r.get(3, 3), NE);
        assert_eq!(r.get(4, 3), SW);
        assert_eq!(r.get(5, 5), NE);
    }

    #[test]
    fn one_bad_tile_blocks_the_whole_road() {
        let mut w = flat_world(8);
        // Tile (4, 2) rises towards the south, so an east-west road can't cross it.
        w.set(
            4,
            1,
            2,
            Block {
                material: Material::Grass,
                shape: Shape(0b1100),
            },
        );
        let mut r = Layer::new(8, 8);
        let p = plan(&w, &r, &Layer::new(8, 8), &Layer::new(64, 64), (1, 2), (7, 2));
        assert_eq!(p.blocked, Some(Blocked::Slope));
        assert!(p.build(&mut r).is_empty());
        assert_eq!(r.get(2, 2), 0, "nothing is built");
        // Running up the slope is fine.
        let p = plan(&w, &r, &Layer::new(8, 8), &Layer::new(64, 64), (4, 0), (4, 5));
        assert_eq!(p.blocked, None);
    }

    #[test]
    fn water_and_rails_block() {
        let mut w = flat_world(4);
        let mut rails = Layer::new(4, 4);
        rails.set(2, 1, 1);
        let p = plan(&w, &Layer::new(4, 4), &rails, &Layer::new(64, 64), (0, 1), (3, 1));
        assert_eq!(p.blocked, Some(Blocked::Occupied));
        w.sea_level = 5;
        let p = plan(
            &w,
            &Layer::new(4, 4),
            &Layer::new(4, 4),
            &Layer::new(64, 64),
            (0, 0),
            (2, 0),
        );
        assert_eq!(p.blocked, Some(Blocked::Water));
    }
}
