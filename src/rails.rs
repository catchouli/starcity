//! Railways: single track, TTD-style. Every piece joins two edge midpoints of
//! a tile: two straights (X runs west-east, Y north-south) and four diagonal
//! pieces across the corners. There are no 90° turns and nothing stops at a
//! tile's centre; junctions are pieces sharing an edge midpoint.

use crate::network::{self, Blocked, E, Layer, N, Piece, Plan, S, W, edge_towards, midpoint};
use crate::terrain::World;
use crate::terrain::edit::Corners;
use std::collections::{BTreeMap, HashMap};

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

/// A straight road and a straight track at right angles cross on the level.
/// (Anything else on the same tile isn't allowed.)
pub fn crossing_ok(road: u8, rail: u8) -> bool {
    (road == N | S && rail == X) || (road == E | W && rail == Y)
}

/// Whether a tile could hold these track pieces.
pub fn check(world: &World, roads: &Layer, buildings: &Layer, x: i32, z: i32, bits: u8) -> Result<(), Blocked> {
    let road = roads.get(x, z);
    if (road != 0 && !crossing_ok(road, bits)) || buildings.get(x, z) != 0 {
        return Err(Blocked::Occupied);
    }
    network::slope_check(world, x, z, bits & X != 0, bits & Y != 0, bits & DIAGONALS != 0)
}

/// Plans track dragged from tile `a` to tile `b`: straight along an axis, or
/// diagonal when the drag is nearer 45°. Slopes that don't suit the track
/// block it.
#[allow(dead_code)] // The game levels slopes; this strict version serves tests and dev demos.
pub fn plan(
    world: &World,
    rails: &Layer,
    roads: &Layer,
    buildings: &Layer,
    signals: &Signals,
    a: (i32, i32),
    b: (i32, i32),
) -> Plan {
    plan_with(world, rails, roads, buildings, signals, a, b, false)
}

/// Like [`plan`], but where the ground has the wrong slope the plan levels it
/// (see `Plan::terraform`), which costs extra. Ground that's too steep to
/// level without moving corners too far still blocks it.
#[allow(dead_code)] // The game routes with `plan_route`; kept for the demo and tests.
pub fn plan_terraforming(
    world: &World,
    rails: &Layer,
    roads: &Layer,
    buildings: &Layer,
    signals: &Signals,
    a: (i32, i32),
    b: (i32, i32),
) -> Plan {
    plan_with(world, rails, roads, buildings, signals, a, b, true)
}

/// Plans track from tile `a` to any other tile `b`: finds a route round
/// water, buildings and roads, then lays it on a gentle grade, moving the
/// ground (at extra cost) so every tile suits its track. Where the two
/// points are lined up, or the ground is easy, this is simply a straight
/// line. If there's no way through the plan is blocked with `NoRoute`.
pub fn plan_route(
    world: &World,
    rails: &Layer,
    roads: &Layer,
    buildings: &Layer,
    signals: &Signals,
    a: (i32, i32),
    b: (i32, i32),
) -> Plan {
    if a == b {
        return Plan::default();
    }
    // Where the ground work fails, search again round the trouble spot.
    let mut avoid = std::collections::HashSet::new();
    let mut last: Option<Plan> = None;
    for _ in 0..6 {
        let Some(pieces) = find_route(world, rails, roads, buildings, &avoid, a, b) else {
            break;
        };
        let grade = Grade {
            max_shift: 6,
            max_corners: 400 + 14 * pieces.len(),
        };
        let plan = finish(
            world,
            rails,
            roads,
            buildings,
            signals,
            Plan::from_pieces(pieces),
            true,
            Some(grade),
        );
        let Some(trouble) = plan.trouble.filter(|_| plan.blocked.is_some()) else {
            return plan;
        };
        // Give up on a spot at the very ends: going round won't help there.
        let near_end = |e: (i32, i32)| (trouble.0 - e.0).abs().max((trouble.1 - e.1).abs()) <= 2;
        if near_end(a) || near_end(b) {
            return plan;
        }
        for dz in -3..=3 {
            for dx in -3..=3 {
                avoid.insert((trouble.0 + dx, trouble.1 + dz));
            }
        }
        last = Some(plan);
    }
    if let Some(plan) = last {
        return plan;
    }
    // Nothing found: show the straight attempt, in red.
    let mut plan = plan_with(world, rails, roads, buildings, signals, a, b, true);
    plan.blocked = Some(Blocked::NoRoute);
    plan
}

const DIRS: [(i32, i32); 4] = [(1, 0), (0, 1), (-1, 0), (0, -1)];

/// The piece a tile holds when track enters travelling `DIRS[d_in]` and leaves
/// travelling `DIRS[d_out]`: a straight, or a curve across the corner.
fn step_piece(d_in: usize, d_out: usize) -> u8 {
    if d_in == d_out {
        if d_in.is_multiple_of(2) { X } else { Y }
    } else {
        let (i, o) = (DIRS[d_in], DIRS[d_out]);
        diagonal_piece(network::opposite(edge_towards(i.0, i.1)), edge_towards(o.0, o.1))
    }
}

/// Ground height of a tile in quarter-blocks (the sum of its four corners
/// relative to the base) and how uneven it is.
fn ground(world: &World, t: (i32, i32)) -> Option<(i32, i32)> {
    let (base, shape) = world.surface(t.0, t.1)?;
    let raised = (0..4).filter(|&i| shape.0 & (1 << i) != 0).count() as i32;
    Some((base as i32 * 4 + raised, if shape.0 == 0 { 0 } else { 1 }))
}

/// A* over (tile, heading): straights and curves only, avoiding water,
/// buildings, roads and other track, preferring flat, straight, direct
/// routes and keeping a little clear of buildings (levelling beside a house
/// would disturb it). Returns the pieces from `a` to `b`.
fn find_route(
    world: &World,
    rails: &Layer,
    roads: &Layer,
    buildings: &Layer,
    avoid: &std::collections::HashSet<(i32, i32)>,
    a: (i32, i32),
    b: (i32, i32),
) -> Option<Vec<Piece>> {
    use std::cmp::Reverse;
    use std::collections::BinaryHeap;

    let (sx, sz) = (world.size_x as i32, world.size_z as i32);
    // Weighted a little above the true minimum: slightly longer routes, far less searching.
    let h = |t: (i32, i32)| 16 * ((t.0 - b.0).abs() + (t.1 - b.1).abs()) as u32;
    // Tiles the track may use. The ends may already carry track (a join).
    let free = |t: (i32, i32)| {
        t.0 >= 0
            && t.1 >= 0
            && t.0 < sx
            && t.1 < sz
            && buildings.get(t.0, t.1) == 0
            && !avoid.contains(&t)
            && (t == a || t == b || rails.get(t.0, t.1) == 0)
            && world.surface(t.0, t.1).is_some_and(|(base, _)| base >= world.sea_level)
    };
    // Near buildings, roads or track costs extra: ground work next to them is refused.
    let crowded = |t: (i32, i32)| {
        (-1..=1).any(|dz| {
            (-1..=1).any(|dx| {
                let (x, z) = (t.0 + dx, t.1 + dz);
                (dx != 0 || dz != 0) && (roads.get(x, z) != 0 || buildings.get(x, z) != 0 || rails.get(x, z) != 0)
            })
        })
    };

    type State = ((i32, i32), usize);
    let mut best: HashMap<State, u32> = HashMap::new();
    let mut from: HashMap<State, (State, u8)> = HashMap::new();
    let mut open = BinaryHeap::new();
    for d in 0..4 {
        best.insert((a, d), 0);
        open.push(Reverse((h(a), 0u32, (a, d))));
    }
    let mut expanded = 0;
    while let Some(Reverse((_, cost, state))) = open.pop() {
        if best.get(&state).is_some_and(|&c| c < cost) {
            continue;
        }
        expanded += 1;
        if expanded > 80_000 {
            return None;
        }
        let (tile, d_in) = state;
        if !free(tile) {
            continue;
        }
        let here = ground(world, tile)?;
        // The first tile has no track behind it, so it can only run straight on.
        let outs: &[usize] = if tile == a {
            &[d_in][..]
        } else {
            &[d_in, (d_in + 1) % 4, (d_in + 3) % 4][..]
        };
        // Arriving at the goal: the last tile runs straight on.
        if tile == b {
            let mut pieces = vec![Piece::add(tile, step_piece(d_in, d_in))];
            let mut at = state;
            while let Some(&(prev, bits)) = from.get(&at) {
                pieces.push(Piece::add(prev.0, bits));
                at = prev;
            }
            pieces.reverse();
            return Some(pieces);
        }
        let road_here = roads.get(tile.0, tile.1);
        for &d_out in outs {
            let bits = step_piece(d_in, d_out);
            // A road in the way can only be crossed, square on, by a straight.
            if road_here != 0 && !crossing_ok(road_here, bits) {
                continue;
            }
            let next_tile = (tile.0 + DIRS[d_out].0, tile.1 + DIRS[d_out].1);
            let Some(there) = ground(world, next_tile).filter(|_| free(next_tile)) else {
                continue;
            };
            let turn = if d_out == d_in { 0 } else { 20 };
            // Climbing costs per quarter-block; uneven ground costs because it will be levelled.
            let climb = (there.0 - here.0).unsigned_abs() * 3;
            let uneven = there.1 as u32 * 6 + if turn > 0 { here.1 as u32 * 12 } else { 0 };
            let near = if crowded(next_tile) && next_tile != b { 25 } else { 0 }
                + if roads.get(next_tile.0, next_tile.1) != 0 { 8 } else { 0 };
            let c = cost + 10 + turn + climb + uneven + near;
            let next = (next_tile, d_out);
            if best.get(&next).is_none_or(|&old| c < old) {
                best.insert(next, c);
                from.insert(next, (state, bits));
                open.push(Reverse((c + h(next_tile), c, next)));
            }
        }
    }
    None
}

/// Chooses a height for each tile along the line: close to the ground (less
/// to move, so cheaper), but changing by at most one block at a time, only on
/// straight tiles (which become ramps), and with at least two tiles between
/// changes: a gentle grade rather than a cliff. Dynamic programming over
/// (tile, height, tiles since the last change).
fn grade_profile(
    corners: &Corners,
    sea_level: i32,
    tiles: &[(i32, i32)],
    straight: &[bool],
    weights: &[u32],
    frozen: &HashMap<(i32, i32), i32>,
    min_run: usize,
) -> Result<Vec<i32>, usize> {
    let min_run_const = min_run;
    const REACH: i32 = 5;
    const REACH_FROZEN: i32 = 12;
    let n = tiles.len();
    let ground: Vec<[i32; 4]> = tiles
        .iter()
        .map(|&t| corners.tile(t.0, t.1))
        .collect::<Option<_>>()
        .ok_or(0usize)?;
    let lo = ground.iter().flatten().min().ok_or(0usize)? - REACH;
    let hi = ground.iter().flatten().max().ok_or(0usize)? + REACH;
    let (lo, hi) = (lo.max(sea_level).max(1), hi);
    if lo > hi {
        return Err(0);
    }
    let heights = (hi - lo + 1) as usize;
    let states = heights * (min_run_const + 1);
    let idx = |h: usize, s: usize| h * (min_run_const + 1) + s;
    const INF: u32 = u32::MAX / 2;
    // Each corner ends up at the height of the last tile along the line that
    // touches it. Ground heights change by at most one per corner, so such a
    // corner can't be further from a fixed corner's height than the corners
    // between them: that bounds the heights each tile may take.
    let mut last: HashMap<(i32, i32), usize> = HashMap::new();
    for (i, &(x, z)) in tiles.iter().enumerate() {
        for (cx, cz) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
            last.insert((x + cx, z + cz), i);
        }
    }
    let bounds: Vec<(i32, i32)> = tiles
        .iter()
        .enumerate()
        .map(|(i, &(x, z))| {
            let (mut min, mut max) = (i32::MIN, i32::MAX);
            for (cx, cz) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                let c = (x + cx, z + cz);
                if last[&c] != i {
                    continue;
                }
                for dz in -REACH_FROZEN..=REACH_FROZEN {
                    for dx in -REACH_FROZEN..=REACH_FROZEN {
                        if let Some(&f) = frozen.get(&(c.0 + dx, c.1 + dz)) {
                            let d = dx.abs().max(dz.abs());
                            min = min.max(f - d);
                            max = max.min(f + d);
                        }
                    }
                }
            }
            (min, max)
        })
        .collect();
    let tile_cost = |i: usize, h: usize| {
        if (bounds[i].0..=bounds[i].1).contains(&(lo + h as i32)) {
            weights[i]
                * ground[i]
                    .iter()
                    .map(|&c| (c - (lo + h as i32)).unsigned_abs())
                    .sum::<u32>()
        } else {
            INF / 4
        }
    };
    let mut dp = vec![INF; states];
    let mut back: Vec<Vec<usize>> = Vec::with_capacity(n);
    for h in 0..heights {
        dp[idx(h, min_run_const)] = tile_cost(0, h);
    }
    back.push(vec![usize::MAX; states]);
    for i in 1..n {
        let mut next = vec![INF; states];
        let mut from = vec![usize::MAX; states];
        for h in 0..heights {
            for s in 0..=min_run_const {
                let cur = dp[idx(h, s)];
                if cur >= INF {
                    continue;
                }
                // Stay level.
                let mut moves = vec![(h, (s + 1).min(min_run_const))];
                // Or step up or down onto the next tile, if this one can be a ramp.
                if straight[i - 1] && s >= min_run_const {
                    if h > 0 {
                        moves.push((h - 1, 1.min(min_run_const)));
                    }
                    if h + 1 < heights {
                        moves.push((h + 1, 1.min(min_run_const)));
                    }
                }
                for (nh, ns) in moves {
                    let c = cur + tile_cost(i, nh);
                    if c < next[idx(nh, ns)] {
                        next[idx(nh, ns)] = c;
                        from[idx(nh, ns)] = idx(h, s);
                    }
                }
            }
        }
        // If nothing is possible any more, say where it went wrong.
        if next.iter().all(|&c| c >= INF / 4) {
            return Err(i);
        }
        dp = next;
        back.push(from);
    }
    // A profile that breaks a fixed corner's limit isn't one.
    let mut at = (0..states)
        .min_by_key(|&k| dp[k])
        .filter(|&k| dp[k] < INF / 4)
        .ok_or(n - 1)?;
    let mut out = vec![0; n];
    for i in (0..n).rev() {
        out[i] = lo + (at / (min_run_const + 1)) as i32;
        at = back[i][at];
    }
    Ok(out)
}

#[allow(clippy::too_many_arguments)]
fn plan_with(
    world: &World,
    rails: &Layer,
    roads: &Layer,
    buildings: &Layer,
    signals: &Signals,
    a: (i32, i32),
    b: (i32, i32),
    terraform: bool,
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
    finish(
        world,
        rails,
        roads,
        buildings,
        signals,
        Plan::from_pieces(pieces),
        terraform,
        None,
    )
}

/// A planned path's heights are chosen along its length (see `grade_profile`)
/// so the line climbs gently instead of cutting through hills.
struct Grade {
    /// The most the ground may move at any corner, and corners moved in all.
    max_shift: i32,
    max_corners: usize,
}

#[allow(clippy::too_many_arguments)]
fn finish(
    world: &World,
    rails: &Layer,
    roads: &Layer,
    buildings: &Layer,
    signals: &Signals,
    mut plan: Plan,
    terraform: bool,
    grade: Option<Grade>,
) -> Plan {
    let tiles: Vec<_> = plan.tiles().collect();
    let bits_at: Vec<u8> = tiles.iter().map(|&t| plan.result(rails, t).unwrap()).collect();
    for (&t, &bits) in tiles.iter().zip(&bits_at) {
        if signals.by_tile.contains_key(&t) && bits != rails.get(t.0, t.1) {
            plan.blocked = Some(Blocked::Signal);
            return plan;
        }
        let road = roads.get(t.0, t.1);
        if (road != 0 && !crossing_ok(road, bits)) || buildings.get(t.0, t.1) != 0 {
            plan.blocked = Some(Blocked::Occupied);
            return plan;
        }
    }

    // Each pass finds the tiles whose ground doesn't suit their track and
    // levels them; levelling can disturb neighbours, so check again until
    // everything fits (or it clearly won't).
    let mut corners = match &grade {
        Some(g) => Corners::with_limits(world, g.max_shift, g.max_corners),
        None => Corners::new(world),
    };
    let mut levelled: HashMap<(i32, i32), i32> = HashMap::new();
    // Corners of anything already built (roads, houses, track, even under the
    // new line, as at a level crossing) keep their height; the grade has to
    // meet them. Only needed near the line.
    let mut frozen: HashMap<(i32, i32), i32> = HashMap::new();
    if grade.is_some() {
        let (x0, x1) = (
            tiles.iter().map(|t| t.0).min().unwrap() - 12,
            tiles.iter().map(|t| t.0).max().unwrap() + 12,
        );
        let (z0, z1) = (
            tiles.iter().map(|t| t.1).min().unwrap() - 12,
            tiles.iter().map(|t| t.1).max().unwrap() + 12,
        );
        for z in z0.max(0)..=z1.min(world.size_z as i32 - 1) {
            for x in x0.max(0)..=x1.min(world.size_x as i32 - 1) {
                if roads.get(x, z) != 0 || buildings.get(x, z) != 0 || rails.get(x, z) != 0 {
                    for (cx, cz) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                        let c = (x + cx, z + cz);
                        if let Some(h) = corners.world_height(c) {
                            frozen.insert(c, h);
                            corners.freeze(c);
                        }
                    }
                }
            }
        }
    }
    if grade.is_some() {
        // Lay the line on a gentle grade first; the misfit pass below only
        // tidies what that leaves.
        // Only straight tiles can be ramps, and a tile with a road across it has to stay level.
        let straight: Vec<bool> = tiles
            .iter()
            .zip(&bits_at)
            .map(|(&t, &b)| b & DIAGONALS == 0 && roads.get(t.0, t.1) == 0)
            .collect();
        // Moving ground that's built on (or beside something built) is made very unattractive.
        let weights: Vec<u32> = tiles
            .iter()
            .map(|&(x, z)| {
                let built = |x, z| roads.get(x, z) != 0 || buildings.get(x, z) != 0 || rails.get(x, z) != 0;
                // The closer something built is, the dearer it is to move this ground.
                match (0..=3i32).find(|&r| {
                    (-r..=r).any(|dz| (-r..=r).any(|dx| dx.abs().max(dz.abs()) == r && built(x + dx, z + dz)))
                }) {
                    Some(0) => 60,
                    Some(1) => 24,
                    Some(2) => 8,
                    Some(_) => 3,
                    None => 1,
                }
            })
            .collect();
        // Prefer a gentle grade (two tiles between steps); steeper only if that's what it takes.
        let mut profile = Err(0);
        for run in [2, 1, 0] {
            profile = grade_profile(
                &corners,
                world.sea_level as i32,
                &tiles,
                &straight,
                &weights,
                &frozen,
                run,
            );
            if profile.is_ok() {
                break;
            }
        }
        match profile {
            Ok(profile) => {
                for (&t, &h) in tiles.iter().zip(&profile) {
                    if !corners.level(t.0, t.1, h) {
                        plan.blocked = Some(Blocked::Slope);
                        plan.trouble = Some(t);
                        return plan;
                    }
                    levelled.insert(t, h);
                }
            }
            Err(at) => {
                plan.blocked = Some(Blocked::Slope);
                plan.trouble = tiles.get(at).copied();
                return plan;
            }
        }
    }
    for pass in 0..8 {
        let mut misfits = Vec::new();
        for (&t, &bits) in tiles.iter().zip(&bits_at) {
            let Some((base, shape)) = corners.surface(t.0, t.1) else {
                plan.blocked = Some(Blocked::Water);
                return plan;
            };
            match network::shape_check(
                world.sea_level,
                base,
                shape,
                bits & X != 0,
                bits & Y != 0,
                bits & DIAGONALS != 0,
            ) {
                Ok(()) => {}
                Err(Blocked::Slope) if terraform && pass < 7 => misfits.push(t),
                Err(why) => {
                    plan.blocked = Some(why);
                    return plan;
                }
            }
        }
        if misfits.is_empty() {
            break;
        }
        for t in misfits {
            // Track wants to stay level from one levelled tile to the next, so
            // reuse a neighbour's height; otherwise move the ground least.
            let h = levelled.get(&t).copied().or_else(|| {
                [(1, 0), (-1, 0), (0, 1), (0, -1)]
                    .iter()
                    .find_map(|&(dx, dz)| levelled.get(&(t.0 + dx, t.1 + dz)).copied())
            });
            let h = h.unwrap_or_else(|| {
                let mut c = corners.tile(t.0, t.1).unwrap();
                c.sort();
                c[1]
            });
            levelled.insert(t, h);
            if !corners.level(t.0, t.1, h) {
                plan.blocked = Some(Blocked::Slope);
                return plan;
            }
        }
    }

    // Moving the ground mustn't disturb anything already built beside it.
    for (x, z) in corners.affected_tiles() {
        if tiles.contains(&(x, z)) {
            continue;
        }
        let built = roads.get(x, z) != 0 || buildings.get(x, z) != 0 || rails.get(x, z) != 0;
        if built && corners.surface(x, z) != world.surface(x, z) {
            plan.blocked = Some(Blocked::Disturbs);
            plan.trouble = tiles
                .iter()
                .min_by_key(|t| (t.0 - x).abs().max((t.1 - z).abs()))
                .copied();
            return plan;
        }
    }
    plan.terraform = corners.changes();
    plan.terraform_steps = corners.steps();
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

    /// A free tile near a town's centre.
    fn free_tile_near(world: &World, roads: &Layer, buildings: &Layer, centre: (i32, i32)) -> Option<(i32, i32)> {
        for r in 0..20i32 {
            for dz in -r..=r {
                for dx in -r..=r {
                    let t = (centre.0 + dx, centre.1 + dz);
                    // Open ground: nothing built within a tile of it.
                    let open = (-1..=1).all(|oz| {
                        (-1..=1).all(|ox| {
                            roads.get(t.0 + ox, t.1 + oz) == 0
                                && buildings.get(t.0 + ox, t.1 + oz) == 0
                                && world
                                    .surface(t.0 + ox, t.1 + oz)
                                    .is_some_and(|(b, _)| b > world.sea_level)
                        })
                    });
                    if dx.abs().max(dz.abs()) == r && open {
                        return Some(t);
                    }
                }
            }
        }
        None
    }

    /// Whether `b` can be reached from `a` over land (ignoring slopes and buildings).
    fn land_connected(world: &World, a: (i32, i32), b: (i32, i32)) -> bool {
        let mut seen = std::collections::HashSet::from([a]);
        let mut queue = std::collections::VecDeque::from([a]);
        while let Some(t) = queue.pop_front() {
            if t == b {
                return true;
            }
            for (dx, dz) in DIRS {
                let n = (t.0 + dx, t.1 + dz);
                if world.in_bounds(n.0, n.1)
                    && world.surface(n.0, n.1).is_some_and(|(base, _)| base >= world.sea_level)
                    && seen.insert(n)
                {
                    queue.push_back(n);
                }
            }
        }
        false
    }

    /// Any two towns that are a sensible distance apart can be joined, on any seed,
    /// whatever the ground is like, and the result is valid track once the ground moves.
    #[test]
    fn towns_can_be_joined_across_any_terrain() {
        use crate::terrain::edit;
        use crate::terrain::generation::{self, GenParams};
        use crate::towns::Towns;
        let (mut tried, mut joined, mut total_steps, mut total_len) = (0, 0, 0u32, 0usize);
        let mut slowest = 0u128;
        for seed in [1u32, 7, 42, 2024, 99] {
            let params = GenParams {
                seed,
                ..GenParams::default()
            };
            let mut world = generation::generate(&params);
            let mut roads = Layer::new(world.size_x, world.size_z);
            let mut rails = Layer::new(world.size_x, world.size_z);
            let towns = Towns::generate(&world, &mut roads, &rails, seed, 2000);
            let signals = Signals::default();
            for t in &towns.list {
                // The nearest other town that's 25 to 120 tiles away.
                let dist = |o: &crate::towns::Town| {
                    (((o.centre.0 - t.centre.0).pow(2) + (o.centre.1 - t.centre.1).pow(2)) as f32).sqrt()
                };
                let Some(other) = towns
                    .list
                    .iter()
                    .filter(|o| o.id != t.id && (25.0..=120.0).contains(&dist(o)))
                    .min_by(|a, b| dist(a).total_cmp(&dist(b)))
                else {
                    continue;
                };
                let (Some(a), Some(b)) = (
                    free_tile_near(&world, &roads, &towns.occupied, t.centre),
                    free_tile_near(&world, &roads, &towns.occupied, other.centre),
                ) else {
                    continue;
                };
                if !land_connected(&world, a, b) {
                    continue;
                }
                tried += 1;
                let started = std::time::Instant::now();
                let plan = plan_route(&world, &rails, &roads, &towns.occupied, &signals, a, b);
                slowest = slowest.max(started.elapsed().as_millis());
                if plan.blocked.is_some() {
                    eprintln!("seed {seed}: {a:?} -> {b:?} blocked: {:?}", plan.blocked);
                    continue;
                }
                joined += 1;
                total_steps += plan.terraform_steps;
                total_len += plan.pieces.len();
                edit::apply(&mut world, &plan.terraform);
                plan.build(&mut rails);
                for p in &plan.pieces {
                    let (x, z) = p.tile;
                    let bits = rails.get(x, z);
                    assert!(
                        check(&world, &roads, &towns.occupied, x, z, bits).is_ok(),
                        "seed {seed}: track at {:?} doesn't fit the ground after levelling",
                        p.tile
                    );
                }
                // Contiguous: the path starts at a and ends at b.
                assert_eq!(plan.pieces.first().unwrap().tile, a);
                assert_eq!(plan.pieces.last().unwrap().tile, b);
            }
        }
        eprintln!(
            "slowest plan {slowest} ms; joined {joined}/{tried}, average {} tiles, {} corner-steps",
            total_len / joined.max(1),
            total_steps / joined.max(1) as u32
        );
        assert!(tried >= 10);
        assert!(
            joined * 100 >= tried * 85,
            "only {joined} of {tried} town pairs could be joined"
        );
    }
}
