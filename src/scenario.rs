//! A ready-made test scenario: railway lines between neighbouring towns, for
//! trying out trains, stations and traffic without building everything by
//! hand. Started with `--demo` natively or `?demo` on the web.
//!
//! For each pair of nearby towns it puts a 4-tile station on the side of each
//! town facing the other, then joins the stations' facing ends with track:
//! straight, then 45°, then straight again. A line is kept only if a train
//! could actually run between the stations; otherwise it's rolled back.

use crate::network::{self, Layer, Piece, Plan};
use crate::rails::{self, Signals};
use crate::stations::{StationId, Stations};
use crate::terrain::World;
use crate::towns::Towns;
use crate::trains;

/// Everything a line is built into.
pub struct Railway<'a> {
    pub world: &'a World,
    pub roads: &'a Layer,
    pub buildings: &'a Layer,
    pub signals: &'a Signals,
    pub rails: &'a mut Layer,
    pub stations: &'a mut Stations,
}

/// A built line: its two stations, and the tile to start a train on at the
/// first (with the train facing the second).
pub struct Line {
    pub from: StationId,
    pub to: StationId,
    pub start: (i32, i32),
    pub forward: bool,
}

const PLATFORM: i32 = 4;

/// Builds up to `max` lines between the nearest pairs of towns.
pub fn build_lines(rw: &mut Railway, towns: &Towns, max: usize) -> Vec<Line> {
    let mut pairs = Vec::new();
    for a in &towns.list {
        for b in &towns.list {
            let (dx, dz) = (b.centre.0 - a.centre.0, b.centre.1 - a.centre.1);
            let d2 = dx * dx + dz * dz;
            if a.id < b.id && d2 < 160 * 160 {
                pairs.push((d2, a.centre, b.centre));
            }
        }
    }
    pairs.sort();
    let mut lines = Vec::new();
    let mut used: Vec<(i32, i32)> = Vec::new();
    for (_, a, b) in pairs {
        if lines.len() >= max {
            break;
        }
        // Keep it simple: each town gets at most one line.
        if used.contains(&a) || used.contains(&b) {
            continue;
        }
        let saved = (rw.rails.clone(), rw.stations.clone());
        match try_line(rw, a, b) {
            Some(line) => {
                used.extend([a, b]);
                lines.push(line);
            }
            None => (*rw.rails, *rw.stations) = saved,
        }
    }
    lines
}

/// Works in "line space": `u` runs along the main axis between the towns
/// (towards town `b`), `v` across it.
fn try_line(rw: &mut Railway, a: (i32, i32), b: (i32, i32)) -> Option<Line> {
    let along_x = (b.0 - a.0).abs() >= (b.1 - a.1).abs();
    let s = if along_x {
        (b.0 - a.0).signum()
    } else {
        (b.1 - a.1).signum()
    };
    let to_xz = |u: i32, v: i32| if along_x { (u, v) } else { (v, u) };
    let to_uv = |t: (i32, i32)| if along_x { t } else { (t.1, t.0) };

    let station_a = place_station(rw, to_uv(a), s, &to_xz)?;
    let station_b = place_station(rw, to_uv(b), -s, &to_xz)?;
    let (a_end, b_end) = (*station_a.last().unwrap(), *station_b.last().unwrap());

    // Join the facing platform ends: out of one, round the hills, into the other.
    let dir = to_xz(s, 0);
    let start = (to_xz(a_end.0 + s, a_end.1), dir);
    let goal = to_xz(b_end.0 - s, b_end.1);
    let pieces = route(rw, start.0, dir, goal)?;
    let plan = Plan { pieces, blocked: None };
    plan.build(rw.rails);

    let (from, to) = (
        rw.stations.at(to_xz(a_end.0, a_end.1))?,
        rw.stations.at(to_xz(b_end.0, b_end.1))?,
    );
    if !trains::reachable(rw.rails, rw.stations, from, to) || !trains::reachable(rw.rails, rw.stations, to, from) {
        return None;
    }
    // Start the train on the third platform tile, facing along the line.
    let start = station_a[2];
    Some(Line {
        from,
        to,
        start: to_xz(start.0, start.1),
        forward: s > 0,
    })
}

const DIRS: [(i32, i32); 4] = [(1, 0), (0, 1), (-1, 0), (0, -1)];

/// Finds track from `start` (entered travelling `dir`) to `goal`, leaving
/// `goal` in direction `dir` too. Every tile is a straight piece or a curve
/// (a diagonal piece joining two adjacent edges); curves need flat ground and
/// straights on a slope must run up it, so the route winds round hills.
fn route(rw: &Railway, start: (i32, i32), dir: (i32, i32), goal: (i32, i32)) -> Option<Vec<Piece>> {
    use std::cmp::Reverse;
    use std::collections::{BinaryHeap, HashMap};

    let size = rw.world.size_x as i32;
    let heading = |d: (i32, i32)| DIRS.iter().position(|&x| x == d).unwrap();
    let h = |t: (i32, i32)| (t.0 - goal.0).abs() + (t.1 - goal.1).abs();
    // What a tile would hold if the track went in by `d_in` and out by `d_out`.
    let piece = |d_in: usize, d_out: usize| {
        if d_in == d_out {
            if d_in % 2 == 0 { rails::X } else { rails::Y }
        } else {
            let (i, o) = (DIRS[d_in], DIRS[d_out]);
            rails::diagonal_piece(
                network::opposite(network::edge_towards(i.0, i.1)),
                network::edge_towards(o.0, o.1),
            )
        }
    };

    type State = ((i32, i32), usize);
    let first: State = (start, heading(dir));
    let mut best: HashMap<State, u32> = HashMap::from([(first, 0)]);
    let mut from: HashMap<State, (State, u8)> = HashMap::new();
    let mut open = BinaryHeap::from([Reverse((h(start) as u32, 0u32, first))]);
    let mut expanded = 0;
    while let Some(Reverse((_, cost, state))) = open.pop() {
        if best.get(&state).is_some_and(|&c| c < cost) {
            continue;
        }
        expanded += 1;
        if expanded > 300_000 {
            return None;
        }
        let (tile, d_in) = state;
        if !(0..size).contains(&tile.0) || !(0..size).contains(&tile.1) || rw.rails.get(tile.0, tile.1) != 0 {
            continue;
        }
        for d_out in [d_in, (d_in + 1) % 4, (d_in + 3) % 4] {
            let bits = piece(d_in, d_out);
            if rails::check(rw.world, rw.roads, rw.buildings, tile.0, tile.1, bits).is_err() {
                continue;
            }
            let step = 1 + if d_out == d_in { 0 } else { 2 };
            if tile == goal && DIRS[d_out] == dir {
                // Done: walk the path back.
                let mut pieces = vec![Piece::add(tile, bits)];
                let mut at = state;
                while let Some(&(prev, bits)) = from.get(&at) {
                    pieces.push(Piece::add(prev.0, bits));
                    at = prev;
                }
                return Some(pieces);
            }
            let next = ((tile.0 + DIRS[d_out].0, tile.1 + DIRS[d_out].1), d_out);
            let c = cost + step;
            if best.get(&next).is_none_or(|&old| c < old) {
                best.insert(next, c);
                from.insert(next, (state, bits));
                open.push(Reverse((c + h(next.0) as u32, c, next)));
            }
        }
    }
    None
}

/// Finds and builds a 4-tile station a little way from `centre` in direction
/// `s` along the line. Returns its tiles in line space, ordered in direction `s`.
fn place_station(
    rw: &mut Railway,
    centre: (i32, i32),
    s: i32,
    to_xz: &dyn Fn(i32, i32) -> (i32, i32),
) -> Option<Vec<(i32, i32)>> {
    for d in 6..18 {
        for v in [0, 2, -2, 4, -4, 6, -6] {
            let tiles: Vec<(i32, i32)> = (0..PLATFORM).map(|i| (centre.0 + s * (d + i), centre.1 + v)).collect();
            let (first, last) = (tiles[0], tiles[PLATFORM as usize - 1]);
            let plan = rw.stations.plan(
                rw.world,
                rw.rails,
                rw.roads,
                rw.buildings,
                rw.signals,
                to_xz(first.0, first.1),
                to_xz(last.0, last.1),
            );
            // The track has to be able to leave the far end, too.
            let beyond = to_xz(last.0 + s, last.1);
            let exit_clear = rw.roads.get(beyond.0, beyond.1) == 0 && rw.buildings.get(beyond.0, beyond.1) == 0;
            if plan.blocked.is_none() && exit_clear {
                rw.stations.build(&plan, rw.rails);
                return Some(tiles);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terrain::generation::{self, GenParams};

    #[test]
    fn builds_lines_on_a_generated_map() {
        let p = GenParams::default();
        let world = generation::generate(&p);
        let (sx, sz) = (world.size_x, world.size_z);
        let mut roads = Layer::new(sx, sz);
        let mut rails = Layer::new(sx, sz);
        let towns = Towns::generate(&world, &mut roads, &rails, p.seed);
        let signals = Signals::default();
        let mut stations = Stations::default();
        let mut rw = Railway {
            world: &world,
            roads: &roads,
            buildings: &towns.occupied,
            signals: &signals,
            rails: &mut rails,
            stations: &mut stations,
        };
        let lines = build_lines(&mut rw, &towns, 12);
        assert!(!lines.is_empty(), "no lines among {} towns", towns.list.len());
        for line in &lines {
            assert!(trains::reachable(&rails, &stations, line.from, line.to));
            assert!(stations.at(line.start).is_some(), "trains start on a platform");
        }
        let mut tr = trains::Trains::new(1);
        for line in &lines {
            let id = tr.spawn(&rails, line.start, line.forward, 3).unwrap();
            tr.set_orders(id, vec![line.to, line.from], 0);
        }
        // Two minutes of running shouldn't crash anything.
        for _ in 0..1200 {
            tr.update(0.1, &rails, &signals, &stations, 1);
        }
        assert!(tr.list.iter().all(|t| t.crashed.is_none()));
    }
}
