//! A ready-made test scenario: railway lines between neighbouring towns, for
//! trying out trains, stations and traffic without building everything by
//! hand. Started with `--demo` natively or `?demo` on the web.
//!
//! For each pair of nearby towns it puts a 4-tile station on the side of each
//! town facing the other, then joins the stations' facing ends with track:
//! straight, then 45°, then straight again. A line is kept only if a train
//! could actually run between the stations; otherwise it's rolled back.

use crate::network::Layer;
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

    // Straight, 45°, straight between the facing platform ends.
    let du = (b_end.0 - a_end.0) * s;
    let dv = b_end.1 - a_end.1;
    if du < dv.abs() + 2 {
        return None;
    }
    let k = (du - dv.abs()) / 2;
    let p1 = (a_end.0 + s * k, a_end.1);
    let p2 = (p1.0 + s * dv.abs(), b_end.1);
    for (from, to) in [(a_end, p1), (p1, p2), (p2, b_end)] {
        if from == to {
            continue;
        }
        let plan = rails::plan(
            rw.world,
            rw.rails,
            rw.roads,
            rw.buildings,
            rw.signals,
            to_xz(from.0, from.1),
            to_xz(to.0, to.1),
        );
        if plan.blocked.is_some() {
            return None;
        }
        plan.build(rw.rails);
    }

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
