//! Railways: single track, TTD-style. Every piece joins two edge midpoints of
//! a tile: two straights (X runs west-east, Y north-south) and four diagonal
//! pieces across the corners. There are no 90° turns and nothing stops at a
//! tile's centre; junctions are pieces sharing an edge midpoint.

use crate::network::{self, Blocked, E, Layer, N, Piece, Plan, S, W, edge_towards, midpoint};
use crate::terrain::World;
use std::collections::BTreeMap;

pub const X: u8 = 1 << 0;
pub const Y: u8 = 1 << 1;
pub const NE: u8 = 1 << 2;
pub const ES: u8 = 1 << 3;
pub const SW: u8 = 1 << 4;
pub const WN: u8 = 1 << 5;
pub const DIAGONALS: u8 = NE | ES | SW | WN;

/// The diagonal piece joining two adjacent edges.
pub fn diagonal_piece(a: u8, b: u8) -> u8 {
    match a | b {
        x if x == N | E => NE,
        x if x == E | S => ES,
        x if x == S | W => SW,
        _ => WN,
    }
}

/// Pieces that touch each edge's midpoint.
fn touching(edge: u8) -> u8 {
    match edge {
        N => Y | NE | WN,
        E => X | NE | ES,
        S => Y | ES | SW,
        _ => X | SW | WN,
    }
}

/// Where a piece starts and ends within its tile, as (from, to) offsets from
/// the tile's corner. "Forward" along a piece means from `from` to `to`.
pub fn piece_ends(piece: u8) -> ((f32, f32), (f32, f32)) {
    let (n, e, s, w) = ((0.5, 0.0), (1.0, 0.5), (0.5, 1.0), (0.0, 0.5));
    match piece {
        X => (w, e),
        Y => (n, s),
        NE => (n, e),
        ES => (e, s),
        SW => (s, w),
        _ => (w, n),
    }
}

/// A path signal. It faces one way along its piece: trains may only pass it
/// travelling that way, and only once they've reserved a path through the
/// block ahead. Nothing may enter a signalled block from behind its signals.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Signal {
    /// The track piece it stands beside.
    pub piece: u8,
    /// Faces trains travelling forward along the piece (see `piece_ends`):
    /// its lamp is on the side they approach from.
    pub forward: bool,
}

/// Signals by tile. At most one per tile, and only on tiles with a single
/// track piece (not junctions).
#[derive(Default)]
pub struct Signals {
    pub by_tile: BTreeMap<(i32, i32), Signal>,
}

/// What left-clicking a tile with the signal tool will do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SignalClick {
    Place,
    Flip,
    NoTrack,
    Junction,
}

impl Signals {
    pub fn click_effect(&self, rails: &Layer, tile: (i32, i32)) -> SignalClick {
        let bits = rails.get(tile.0, tile.1);
        if self.by_tile.contains_key(&tile) {
            SignalClick::Flip
        } else if bits == 0 {
            SignalClick::NoTrack
        } else if bits.count_ones() > 1 {
            SignalClick::Junction
        } else {
            SignalClick::Place
        }
    }

    /// Left-click: places a signal (facing `forward` along its piece) or flips
    /// an existing one round. Returns whether anything changed.
    pub fn click(&mut self, rails: &Layer, tile: (i32, i32), forward: bool) -> bool {
        match self.click_effect(rails, tile) {
            SignalClick::Place => {
                self.by_tile.insert(
                    tile,
                    Signal {
                        piece: rails.get(tile.0, tile.1),
                        forward,
                    },
                );
                true
            }
            SignalClick::Flip => {
                let sig = self.by_tile.get_mut(&tile).unwrap();
                sig.forward = !sig.forward;
                true
            }
            SignalClick::NoTrack | SignalClick::Junction => false,
        }
    }

    /// Right-click: removes the signal on a tile. Returns whether there was one.
    pub fn remove(&mut self, tile: (i32, i32)) -> bool {
        self.by_tile.remove(&tile).is_some()
    }

    /// Drops signals whose track has gone or changed. Returns whether any were removed.
    pub fn prune(&mut self, rails: &Layer) -> bool {
        let before = self.by_tile.len();
        self.by_tile.retain(|&(x, z), sig| rails.get(x, z) == sig.piece);
        self.by_tile.len() != before
    }
}

/// Whether a tile could hold these track pieces.
pub fn check(world: &World, roads: &Layer, buildings: &Layer, x: i32, z: i32, bits: u8) -> Result<(), Blocked> {
    if roads.get(x, z) != 0 || buildings.get(x, z) != 0 {
        return Err(Blocked::Occupied);
    }
    network::slope_check(world, x, z, bits & X != 0, bits & Y != 0, bits & DIAGONALS != 0)
}

/// Plans track dragged from tile `a` to tile `b`: straight along an axis, or
/// diagonal when the drag is nearer 45°.
pub fn plan(
    world: &World,
    rails: &Layer,
    roads: &Layer,
    buildings: &Layer,
    signals: &Signals,
    a: (i32, i32),
    b: (i32, i32),
) -> Plan {
    let (dx, dz) = (b.0 - a.0, b.1 - a.1);
    let pieces = if dx == 0 && dz == 0 {
        Vec::new()
    } else if dx.abs() >= 2 * dz.abs() {
        straight(rails, a, (dx.signum(), 0), dx.abs())
    } else if dz.abs() >= 2 * dx.abs() {
        straight(rails, a, (0, dz.signum()), dz.abs())
    } else {
        let n = (dx.abs() + dz.abs() + 1) / 2;
        diagonal(rails, a, (dx.signum(), dz.signum()), n)
    };
    let mut plan = Plan { pieces, blocked: None };
    let tiles: Vec<_> = plan.tiles().collect();
    for t in tiles {
        let bits = plan.result(rails, t).unwrap();
        if signals.by_tile.contains_key(&t) && bits != rails.get(t.0, t.1) {
            plan.blocked = Some(Blocked::Signal);
            break;
        }
        if let Err(why) = check(world, roads, buildings, t.0, t.1, bits) {
            plan.blocked = Some(why);
            break;
        }
    }
    plan
}

/// Straight track across `len + 1` tiles. An end tile whose existing diagonal
/// already reaches the edge facing along the track is left alone: the new
/// straight joins it at that edge rather than crossing it.
fn straight(rails: &Layer, a: (i32, i32), (sx, sz): (i32, i32), len: i32) -> Vec<Piece> {
    let piece = if sx != 0 { X } else { Y };
    let forward = edge_towards(sx, sz);
    let back = network::opposite(forward);
    (0..=len)
        .filter_map(|i| {
            let t = (a.0 + sx * i, a.1 + sz * i);
            let existing = rails.get(t.0, t.1) & DIAGONALS;
            let joins_at = |edge| existing != 0 && existing & touching(edge) != 0 && len > 0;
            if (i == 0 && joins_at(forward)) || (i == len && joins_at(back)) {
                None
            } else {
                Some(Piece::add(t, piece))
            }
        })
        .collect()
}

/// Diagonal track from tile `a` towards `a + n * (sx, sz)`.
///
/// Off the end of a straight it starts at the edge the straight leads to (a
/// 45° turn at that edge); from an existing diagonal it continues on the same
/// line. At the far end, if track already reaches the edge the diagonal
/// arrives at, the last piece is dropped and the two join there.
fn diagonal(rails: &Layer, a: (i32, i32), (sx, sz): (i32, i32), n: i32) -> Vec<Piece> {
    let b = (a.0 + n * sx, a.1 + n * sz);
    let (back_x, back_z) = (edge_towards(-sx, 0), edge_towards(0, -sz));
    let (fwd_x, fwd_z) = (edge_towards(sx, 0), edge_towards(0, sz));
    let at_a = rails.get(a.0, a.1);

    // A straight or a diagonal at `a` fixes which line to follow. From an empty
    // tile either of two lines works, so pick the one that joins up at the end.
    let starts = if at_a & X != 0 {
        vec![midpoint(a, fwd_x)]
    } else if at_a & Y != 0 {
        vec![midpoint(a, fwd_z)]
    } else if at_a & diagonal_piece(back_x, fwd_z) != 0 {
        vec![midpoint(a, back_x)]
    } else if at_a & diagonal_piece(back_z, fwd_x) != 0 {
        vec![midpoint(a, back_z)]
    } else {
        vec![midpoint(a, back_z), midpoint(a, back_x)]
    };
    let walks: Vec<_> = starts
        .into_iter()
        .map(|p| network::diagonal_walk(p, (sx, sz), b))
        .collect();
    let joins = |walk: &[network::Step]| {
        walk.last().is_some_and(|step| {
            let at = rails.get(b.0, b.1);
            at & diagonal_piece(step.enter, step.leave) == 0 && at & touching(step.enter) != 0
        })
    };
    let walk = walks.iter().find(|w| joins(w)).unwrap_or(&walks[0]);

    let mut pieces: Vec<_> = walk
        .iter()
        .map(|step| Piece::add(step.tile, diagonal_piece(step.enter, step.leave)))
        .collect();
    if joins(walk) {
        // Track already reaches the edge we arrive at: end there.
        pieces.pop();
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
        let p = plan(
            w,
            r,
            &Layer::new(r.size_x, r.size_z),
            &Layer::new(64, 64),
            &Signals::default(),
            a,
            b,
        );
        assert!(p.blocked.is_none(), "{a:?} -> {b:?} blocked: {:?}", p.blocked);
        p.build(r);
    }

    #[test]
    fn straight_track_fills_every_tile() {
        let w = flat_world(8);
        let mut r = Layer::new(8, 8);
        build(&w, &mut r, (1, 2), (4, 2));
        assert_eq!((1..=4).map(|x| r.get(x, 2)).collect::<Vec<_>>(), vec![X; 4]);
        build(&w, &mut r, (6, 1), (6, 5));
        assert_eq!(r.get(6, 3), Y);
    }

    #[test]
    fn diagonal_leaves_a_straight_from_its_end_edge() {
        let w = flat_world(12);
        let mut r = Layer::new(12, 12);
        build(&w, &mut r, (1, 2), (4, 2));
        build(&w, &mut r, (4, 2), (7, 5));
        assert_eq!(r.get(4, 2), X, "the straight is untouched");
        assert_eq!(r.get(5, 2), SW, "the diagonal starts at its east edge");
        assert_eq!(r.get(5, 3), NE);
    }

    #[test]
    fn diagonal_arriving_at_a_straight_joins_it() {
        let w = flat_world(12);
        let mut r = Layer::new(12, 12);
        build(&w, &mut r, (5, 4), (9, 4));
        // Coming in from the north-west, the walk enters tile (5, 4) from its west edge.
        build(&w, &mut r, (2, 1), (5, 4));
        assert_eq!(r.get(5, 4), X, "no piece added on the straight's tile");
        assert_eq!(r.get(4, 3) & DIAGONALS, r.get(4, 3));
    }

    #[test]
    fn straight_continues_from_a_diagonal_edge() {
        let w = flat_world(12);
        let mut r = Layer::new(12, 12);
        build(&w, &mut r, (1, 1), (3, 3));
        assert_eq!(r.get(3, 3), NE);
        build(&w, &mut r, (3, 3), (6, 3));
        assert_eq!(r.get(3, 3), NE, "joins at the east edge instead of crossing");
        assert_eq!(r.get(4, 3), X);
    }

    #[test]
    fn one_bad_tile_blocks_everything() {
        let mut w = flat_world(8);
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
        let p = plan(
            &w,
            &r,
            &Layer::new(8, 8),
            &Layer::new(64, 64),
            &Signals::default(),
            (1, 2),
            (7, 2),
        );
        assert_eq!(p.blocked, Some(Blocked::Slope));
        assert!(p.build(&mut r).is_empty());
        let mut roads = Layer::new(8, 8);
        roads.set(2, 5, 1);
        let p = plan(&w, &r, &roads, &Layer::new(64, 64), &Signals::default(), (0, 5), (5, 5));
        assert_eq!(p.blocked, Some(Blocked::Occupied));
    }

    #[test]
    fn signals_flip_and_stay_off_junctions() {
        let w = flat_world(12);
        let mut r = Layer::new(12, 12);
        build(&w, &mut r, (1, 2), (8, 2));
        let mut sigs = Signals::default();
        assert_eq!(sigs.click_effect(&r, (3, 3)), SignalClick::NoTrack);
        assert!(sigs.click(&r, (3, 2), true));
        assert_eq!(
            sigs.by_tile[&(3, 2)],
            Signal {
                piece: X,
                forward: true
            }
        );
        sigs.click(&r, (3, 2), true);
        assert!(!sigs.by_tile[&(3, 2)].forward, "clicking again flips it");
        assert!(sigs.remove((3, 2)));
        assert!(sigs.by_tile.is_empty());

        // A branch dragged from (5, 2) leaves from its east edge, so the junction is (6, 2).
        build(&w, &mut r, (5, 2), (8, 5));
        assert!(r.get(6, 2).count_ones() > 1);
        assert_eq!(sigs.click_effect(&r, (6, 2)), SignalClick::Junction);
    }

    #[test]
    fn track_through_a_signal_is_blocked_and_removal_prunes() {
        let w = flat_world(12);
        let mut r = Layer::new(12, 12);
        build(&w, &mut r, (1, 2), (8, 2));
        let mut sigs = Signals::default();
        sigs.click(&r, (4, 2), true);
        // A branch dragged from (3, 2) starts on (4, 2), which would make the
        // signal's tile a junction; one from (5, 2) is clear of it.
        let p = plan(&w, &r, &Layer::new(12, 12), &Layer::new(64, 64), &sigs, (3, 2), (6, 5));
        assert_eq!(p.blocked, Some(Blocked::Signal));
        let p = plan(&w, &r, &Layer::new(12, 12), &Layer::new(64, 64), &sigs, (5, 2), (8, 5));
        assert_eq!(p.blocked, None);
        // A north-south line crossing the signal's tile would change it.
        let p = plan(&w, &r, &Layer::new(12, 12), &Layer::new(64, 64), &sigs, (4, 0), (4, 5));
        assert_eq!(p.blocked, Some(Blocked::Signal));
        r.set(4, 2, 0);
        assert!(sigs.prune(&r));
        assert!(sigs.by_tile.is_empty());
    }
}
