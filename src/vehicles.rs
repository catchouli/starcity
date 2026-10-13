//! Road vehicles: cars, buses and trucks that drive around the road network.
//!
//! Road pieces run between a tile's centre and its edge midpoints (halves) or
//! between two adjacent edge midpoints (diagonals), so a vehicle drives along
//! a chain of pieces just as a train does along track. Vehicles keep to the
//! right-hand lane, pick a random way at junctions (never doubling back unless
//! the road ends), and queue behind whatever is in front of them in their lane.
//! Each town keeps a fleet sized to its population.

use crate::network::Layer;
use crate::roads::{E, N, NE, S, W};
use crate::towns::{TownId, Towns};
use glam::Vec2;
use std::collections::HashMap;

/// How far right of the road's centre line vehicles drive.
pub const LANE_OFFSET: f32 = 0.16;
/// Gap a vehicle keeps to the one in front.
const FOLLOW_GAP: f32 = 0.5;
/// Never more than this many vehicles on the map.
const MAX_VEHICLES: usize = 900;

#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Car,
    Bus,
    Truck,
}

impl Kind {
    pub fn length(self) -> f32 {
        match self {
            Kind::Car => 0.28,
            Kind::Bus => 0.56,
            Kind::Truck => 0.46,
        }
    }

    fn speed(self) -> f32 {
        match self {
            Kind::Car => 1.6,
            Kind::Bus => 1.15,
            Kind::Truck => 1.25,
        }
    }
}

/// A road piece driven in one direction.
#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RoadSeg {
    pub tile: (i32, i32),
    pub piece: u8,
    pub forward: bool,
}

/// Where a road piece starts and ends within its tile.
fn piece_ends(piece: u8) -> (Vec2, Vec2) {
    let (c, n, e, s, w) = (
        Vec2::splat(0.5),
        Vec2::new(0.5, 0.0),
        Vec2::new(1.0, 0.5),
        Vec2::new(0.5, 1.0),
        Vec2::new(0.0, 0.5),
    );
    match piece {
        N => (c, n),
        E => (c, e),
        S => (c, s),
        W => (c, w),
        NE => (n, e),
        crate::roads::ES => (e, s),
        crate::roads::SW => (s, w),
        _ => (w, n),
    }
}

impl RoadSeg {
    pub fn ends(self) -> (Vec2, Vec2) {
        let (a, b) = piece_ends(self.piece);
        let o = Vec2::new(self.tile.0 as f32, self.tile.1 as f32);
        if self.forward { (o + a, o + b) } else { (o + b, o + a) }
    }

    pub fn len(self) -> f32 {
        let (a, b) = self.ends();
        a.distance(b)
    }

    pub fn dir(self) -> Vec2 {
        let (a, b) = self.ends();
        (b - a).normalize()
    }

    pub fn reversed(self) -> RoadSeg {
        RoadSeg {
            forward: !self.forward,
            ..self
        }
    }
}

/// Where a vehicle can go from the end of `seg`: any piece starting there in
/// either tile sharing the point, except straight back; at a dead end, back.
pub fn next_road_segments(roads: &Layer, seg: RoadSeg) -> Vec<RoadSeg> {
    let (_, end) = seg.ends();
    let dir = seg.dir();
    let local = end - Vec2::new(seg.tile.0 as f32, seg.tile.1 as f32);
    let across = if local.x > 0.99 {
        Some((1, 0))
    } else if local.x < 0.01 {
        Some((-1, 0))
    } else if local.y > 0.99 {
        Some((0, 1))
    } else if local.y < 0.01 {
        Some((0, -1))
    } else {
        None // the tile's centre
    };
    let mut tiles = vec![seg.tile];
    if let Some((dx, dz)) = across {
        tiles.push((seg.tile.0 + dx, seg.tile.1 + dz));
    }
    let mut out = Vec::new();
    for tile in tiles {
        let bits = roads.get(tile.0, tile.1);
        for i in 0..8 {
            let piece = 1u8 << i;
            if bits & piece == 0 || (tile == seg.tile && piece == seg.piece) {
                continue;
            }
            for forward in [true, false] {
                let s = RoadSeg { tile, piece, forward };
                if s.ends().0.distance(end) < 1e-3 && s.dir().dot(dir) > -0.5 {
                    out.push(s);
                }
            }
        }
    }
    if out.is_empty() && roads.get(seg.tile.0, seg.tile.1) & seg.piece != 0 {
        out.push(seg.reversed());
    }
    out
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct Vehicle {
    pub kind: Kind,
    pub town: TownId,
    pub seg: RoadSeg,
    /// Distance along `seg`.
    pub pos: f32,
    pub speed: f32,
    pub color: [u8; 3],
    /// The year it was bought, which decides what it looks like, and the
    /// year it's scrapped (replaced by a newer model).
    pub born: i32,
    pub retire: i32,
    /// Seconds spent stuck behind something, to break rare deadlocks.
    stuck: f32,
}

impl Vehicle {
    /// Position in the right-hand lane, and the direction of travel.
    pub fn placement(&self) -> (Vec2, Vec2) {
        let (a, _) = self.seg.ends();
        let dir = self.seg.dir();
        let right = Vec2::new(-dir.y, dir.x);
        (a + dir * self.pos + right * LANE_OFFSET, dir)
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct Vehicles {
    pub list: Vec<Vehicle>,
    /// The current year; new vehicles are bought in it.
    pub year: i32,
    rng: u64,
    spawn_clock: f32,
}

const CAR_COLOURS: [[u8; 3]; 6] = [
    [200, 40, 40],
    [40, 90, 190],
    [230, 230, 225],
    [40, 40, 45],
    [60, 150, 70],
    [210, 170, 40],
];

impl Vehicles {
    pub fn new(seed: u64) -> Vehicles {
        Vehicles {
            list: Vec::new(),
            year: 2000,
            rng: seed | 1,
            spawn_clock: 0.0,
        }
    }

    fn next(&mut self) -> u64 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        self.rng
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }

    /// Winds the fleet back to `year`: vehicles bought later are re-dated to
    /// just before it (for seeking back through time when testing).
    pub fn rewind(&mut self, year: i32) {
        for i in 0..self.list.len() {
            if self.list[i].born > year {
                let (a, b) = (self.below(4) as i32, self.below(11) as i32);
                self.list[i].born = year - a;
                self.list[i].retire = year + 8 + b;
            }
        }
        self.year = year;
    }

    /// How many of each kind a town of this size runs.
    fn fleet(population: u32) -> [(Kind, usize); 3] {
        [
            (Kind::Car, (population / 40).min(25) as usize),
            (Kind::Bus, (population / 500).min(3) as usize),
            (Kind::Truck, (population / 250).min(5) as usize),
        ]
    }

    /// Spawns one vehicle per town per second towards each town's fleet size.
    fn top_up(&mut self, roads: &Layer, towns: &Towns) {
        let mut counts: HashMap<(TownId, u8), usize> = HashMap::new();
        for v in &self.list {
            *counts.entry((v.town, v.kind as u8)).or_default() += 1;
        }
        for town in &towns.list {
            if self.list.len() >= MAX_VEHICLES {
                return;
            }
            let fleet = Self::fleet(towns.population(town.id));
            let Some(&(kind, _)) = fleet
                .iter()
                .find(|(k, n)| counts.get(&(town.id, *k as u8)).copied().unwrap_or(0) < *n)
            else {
                continue;
            };
            // Start on a random road tile near the town centre.
            let (cx, cz) = town.centre;
            let r = 12;
            let tiles: Vec<(i32, i32)> = (cz - r..=cz + r)
                .flat_map(|z| (cx - r..=cx + r).map(move |x| (x, z)))
                .filter(|&(x, z)| roads.get(x, z) != 0)
                .collect();
            if tiles.is_empty() {
                continue;
            }
            let tile = tiles[self.below(tiles.len())];
            let bits = roads.get(tile.0, tile.1);
            let pieces: Vec<u8> = (0..8).map(|i| 1u8 << i).filter(|p| bits & p != 0).collect();
            let piece = pieces[self.below(pieces.len())];
            let forward = self.below(2) == 0;
            let color = match kind {
                Kind::Car => CAR_COLOURS[self.below(CAR_COLOURS.len())],
                Kind::Bus => [235, 190, 30],
                Kind::Truck => [[170, 60, 40], [60, 100, 160], [90, 120, 70]][self.below(3)],
            };
            let born = self.year - self.below(4) as i32;
            let retire = self.year + 8 + self.below(11) as i32;
            self.list.push(Vehicle {
                kind,
                town: town.id,
                seg: RoadSeg { tile, piece, forward },
                pos: 0.0,
                speed: 0.0,
                color,
                born,
                retire,
                stuck: 0.0,
            });
        }
    }

    /// Advances all vehicles. Returns whether any exist (so the scene needs redrawing).
    pub fn update(&mut self, dt: f32, roads: &Layer, towns: &Towns) -> bool {
        self.spawn_clock += dt;
        if self.spawn_clock >= 1.0 {
            self.spawn_clock -= 1.0;
            // Old models are scrapped; the fleet tops up with new ones.
            let year = self.year;
            self.list.retain(|v| year < v.retire);
            self.top_up(roads, towns);
        }
        // Vehicles whose road has gone disappear.
        self.list
            .retain(|v| roads.get(v.seg.tile.0, v.seg.tile.1) & v.seg.piece != 0);

        // Who's where, to find the vehicle in front.
        let places: Vec<(Vec2, Vec2)> = self.list.iter().map(|v| v.placement()).collect();
        let mut by_tile: HashMap<(i32, i32), Vec<usize>> = HashMap::new();
        for (i, (p, _)) in places.iter().enumerate() {
            by_tile
                .entry((p.x.floor() as i32, p.y.floor() as i32))
                .or_default()
                .push(i);
        }
        let blocked: Vec<bool> = (0..self.list.len())
            .map(|i| {
                let (p, d) = places[i];
                let (tx, tz) = (p.x.floor() as i32, p.y.floor() as i32);
                let reach = FOLLOW_GAP + self.list[i].kind.length();
                (tz - 1..=tz + 1)
                    .flat_map(|z| (tx - 1..=tx + 1).map(move |x| (x, z)))
                    .any(|t| {
                        by_tile.get(&t).is_some_and(|others| {
                            others.iter().any(|&j| {
                                let (q, e) = places[j];
                                let rel = q - p;
                                let ahead = rel.dot(d);
                                j != i && ahead > 0.0 && ahead < reach && rel.perp_dot(d).abs() < 0.1 && e.dot(d) > 0.3
                            })
                        })
                    })
            })
            .collect();

        let mut rng = self.rng;
        for (i, v) in self.list.iter_mut().enumerate() {
            let wait = blocked[i] && v.stuck < 3.0;
            v.stuck = if blocked[i] { v.stuck + dt } else { 0.0 };
            if v.stuck > 4.0 {
                v.stuck = 0.0;
            }
            let target = if wait { 0.0 } else { v.kind.speed() };
            v.speed = if v.speed < target {
                (v.speed + 2.0 * dt).min(target)
            } else {
                target
            };
            v.pos += v.speed * dt;
            while v.pos >= v.seg.len() {
                let options = next_road_segments(roads, v.seg);
                if options.is_empty() {
                    v.pos = v.seg.len();
                    break;
                }
                v.pos -= v.seg.len();
                rng ^= rng << 13;
                rng ^= rng >> 7;
                rng ^= rng << 17;
                v.seg = options[(rng % options.len() as u64) as usize];
            }
        }
        self.rng = rng;
        !self.list.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn junctions_offer_every_way_but_back() {
        let mut roads = Layer::new(8, 8);
        // A crossroads at (3, 3) with arms in every direction.
        roads.set(3, 3, N | E | S | W);
        roads.set(4, 3, W | E);
        // Arriving at the centre from the west half, heading east.
        let arriving = RoadSeg {
            tile: (3, 3),
            piece: W,
            forward: false,
        };
        let next = next_road_segments(&roads, arriving);
        let pieces: Vec<u8> = next.iter().map(|s| s.piece).collect();
        assert_eq!(next.len(), 3, "{next:?}");
        assert!(pieces.contains(&N) && pieces.contains(&E) && pieces.contains(&S));
        // Leaving through the east edge carries on into the next tile.
        let east = RoadSeg {
            tile: (3, 3),
            piece: E,
            forward: true,
        };
        assert_eq!(
            next_road_segments(&roads, east),
            vec![RoadSeg {
                tile: (4, 3),
                piece: W,
                forward: false
            }]
        );
    }

    #[test]
    fn dead_ends_turn_round() {
        let mut roads = Layer::new(8, 8);
        roads.set(3, 3, E);
        roads.set(4, 3, W);
        let into_end = RoadSeg {
            tile: (4, 3),
            piece: W,
            forward: false,
        };
        // Reaching the centre of (4, 3), the only way is back.
        assert_eq!(next_road_segments(&roads, into_end), vec![into_end.reversed()]);
    }

    #[test]
    fn traffic_queues_without_overlapping() {
        let mut roads = Layer::new(32, 4);
        for x in 1..30 {
            roads.set(x, 1, E | W);
        }
        let mut v = Vehicles::new(3);
        for i in 0..3 {
            v.list.push(Vehicle {
                kind: Kind::Car,
                town: 0,
                seg: RoadSeg {
                    tile: (10 - i, 1),
                    piece: E,
                    forward: true,
                },
                pos: 0.0,
                speed: 0.0,
                color: [0; 3],
                born: 2000,
                retire: 2100,
                stuck: 0.0,
            });
        }
        // Stop the front car so the others have to queue behind it.
        let towns = Towns::empty(32, 4);
        for _ in 0..120 {
            v.list[0].speed = 0.0;
            v.list[0].pos = 0.0;
            v.update(1.0 / 60.0, &roads, &towns);
            let xs: Vec<f32> = v.list.iter().map(|c| c.placement().0.x).collect();
            assert!(xs[1] < xs[0] - 0.2 && xs[2] < xs[1] - 0.2, "cars overlapped: {xs:?}");
        }
    }
}
