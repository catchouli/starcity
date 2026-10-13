//! The company's buses (GAME-DESIGN.md §7.2): they drive the road network between bus stops in
//! their orders, carrying passengers. A bus finds its way with a breadth-first search over road
//! pieces (the same pieces town traffic drives), pulls in at each stop for a moment (the app loads
//! and unloads when it reports an arrival), and goes on to the next.

use crate::network::Layer;
use crate::terminals::{Orders, Site, TerminalId, Terminals};
use crate::vehicles::{LANE_OFFSET, RoadSeg, next_road_segments};
use glam::Vec2;
use std::collections::{HashMap, VecDeque};

const LOAD_SECONDS: f32 = 3.0;
/// Road pieces a route search may look at before giving up.
const SEARCH_LIMIT: usize = 40_000;
const RETRY_SECONDS: f32 = 3.0;

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
enum State {
    Idle,
    Driving { path: VecDeque<RoadSeg>, to: TerminalId },
    Loading { at: TerminalId, timer: f32 },
    NoRoute { to: TerminalId, retry: f32 },
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct Bus {
    pub id: u32,
    pub born: i32,
    pub seg: RoadSeg,
    /// Distance along `seg`.
    pub pos: f32,
    pub orders: Orders,
    pub stopped: bool,
    /// Passengers on a bus; units of `cargo` on a truck.
    pub passengers: u32,
    /// Where and when (game seconds) the passengers on board got on, for the fare.
    pub boarded: Option<(f32, f32, f64)>,
    /// What it carries: passengers for a bus, a freight cargo for a truck.
    pub cargo: crate::cargo::Cargo,
    /// From the catalogue.
    pub speed: f32,
    pub capacity: u32,
    pub price: u32,
    pub running: u32,
    state: State,
}

impl Bus {
    pub fn status(&self, terminals: &Terminals) -> String {
        let name = |id: TerminalId| terminals.get(id).map_or("?".to_string(), |t| t.name.clone());
        if self.stopped {
            return "Stopped".to_string();
        }
        match &self.state {
            State::Idle => "No orders: add bus stops below".to_string(),
            State::Driving { to, .. } => format!("Driving to {}", name(*to)),
            State::Loading { at, .. } => format!("Loading at {}", name(*at)),
            State::NoRoute { to, .. } => format!("No road to {}", name(*to)),
        }
    }

    /// Position in the right-hand lane, and the direction of travel.
    pub fn placement(&self) -> (Vec2, Vec2) {
        let (a, _) = self.seg.ends();
        let dir = self.seg.dir();
        let right = Vec2::new(-dir.y, dir.x);
        (a + dir * self.pos + right * LANE_OFFSET, dir)
    }
}

#[derive(serde::Serialize, serde::Deserialize, Default)]
pub struct Buses {
    pub list: Vec<Bus>,
    /// The current year; new buses are built in it.
    pub year: i32,
    /// Buses that have just pulled in: (bus, stop). The app loads and unloads.
    pub arrivals: Vec<(u32, TerminalId)>,
    next_id: u32,
}

/// The road tile a bus stop stands on.
fn stop_tile(terminals: &Terminals, id: TerminalId) -> Option<(i32, i32)> {
    match terminals.get(id)?.site {
        Site::BusStop { tile } => Some(tile),
        _ => None,
    }
}

/// The road pieces from the end of `from` to a piece on `goal`, breadth first.
pub fn route(roads: &Layer, from: RoadSeg, goal: (i32, i32)) -> Option<VecDeque<RoadSeg>> {
    let mut parent: HashMap<RoadSeg, RoadSeg> = HashMap::new();
    let mut queue = VecDeque::from([from]);
    parent.insert(from, from);
    while let Some(seg) = queue.pop_front() {
        if seg != from && seg.tile == goal {
            let mut path = VecDeque::new();
            let mut at = seg;
            while at != from {
                path.push_front(at);
                at = parent[&at];
            }
            return Some(path);
        }
        if parent.len() > SEARCH_LIMIT {
            return None;
        }
        for next in next_road_segments(roads, seg) {
            if let std::collections::hash_map::Entry::Vacant(e) = parent.entry(next) {
                e.insert(seg);
                queue.push_back(next);
            }
        }
    }
    None
}

impl Buses {
    /// Puts a bus on the road at a tile (a bus stop's), facing along one of its pieces.
    pub fn spawn(&mut self, roads: &Layer, tile: (i32, i32)) -> Option<u32> {
        let bits = roads.get(tile.0, tile.1);
        let piece = (0..8).map(|i| 1u8 << i).find(|p| bits & p != 0)?;
        let id = self.next_id;
        self.next_id += 1;
        self.list.push(Bus {
            id,
            born: self.year,
            seg: RoadSeg {
                tile,
                piece,
                forward: true,
            },
            pos: 0.0,
            orders: Orders::default(),
            stopped: false,
            passengers: 0,
            boarded: None,
            cargo: crate::cargo::Cargo::Passengers,
            speed: 1.5,
            capacity: 40,
            price: 5_500,
            running: 500,
            state: State::Idle,
        });
        Some(id)
    }

    pub fn get(&self, id: u32) -> Option<&Bus> {
        self.list.iter().find(|b| b.id == id)
    }

    pub fn get_mut(&mut self, id: u32) -> Option<&mut Bus> {
        self.list.iter_mut().find(|b| b.id == id)
    }

    pub fn remove(&mut self, id: u32) {
        self.list.retain(|b| b.id != id);
    }

    pub fn set_orders(&mut self, id: u32, orders: Orders) {
        if let Some(b) = self.get_mut(id) {
            b.orders = orders;
            if !matches!(b.state, State::Loading { .. }) {
                b.state = State::Idle;
            }
        }
    }

    /// A stop has gone: drop it from every bus's orders.
    pub fn forget_terminal(&mut self, gone: TerminalId) {
        for b in &mut self.list {
            b.orders.forget(gone);
            b.state = State::Idle;
        }
    }

    /// Drives every bus. Returns whether any moved.
    pub fn update(&mut self, dt: f32, roads: &Layer, terminals: &Terminals) -> bool {
        let mut moved = false;
        // Buses whose road has gone are scrapped.
        self.list
            .retain(|b| roads.get(b.seg.tile.0, b.seg.tile.1) & b.seg.piece != 0);
        for bus in &mut self.list {
            if bus.stopped {
                continue;
            }
            let state = std::mem::replace(&mut bus.state, State::Idle);
            bus.state = match state {
                State::Loading { at, timer } => {
                    if timer > dt {
                        State::Loading { at, timer: timer - dt }
                    } else {
                        bus.orders.advance();
                        State::Idle
                    }
                }
                State::NoRoute { to, retry } => {
                    if retry > dt {
                        State::NoRoute { to, retry: retry - dt }
                    } else {
                        State::Idle
                    }
                }
                State::Idle => match bus.orders.target() {
                    None => State::Idle,
                    Some(to) => match stop_tile(terminals, to).and_then(|goal| route(roads, bus.seg, goal)) {
                        Some(path) => State::Driving { path, to },
                        None => State::NoRoute {
                            to,
                            retry: RETRY_SECONDS,
                        },
                    },
                },
                State::Driving { mut path, to } => {
                    moved = true;
                    bus.pos += bus.speed * dt;
                    let mut arrived = false;
                    while bus.pos >= bus.seg.len() {
                        let Some(next) = path.pop_front() else {
                            bus.pos = bus.seg.len();
                            break;
                        };
                        bus.pos -= bus.seg.len();
                        bus.seg = next;
                        if path.is_empty() {
                            arrived = true;
                            bus.pos = 0.0;
                            break;
                        }
                    }
                    if arrived {
                        self.arrivals.push((bus.id, to));
                        State::Loading {
                            at: to,
                            timer: LOAD_SECONDS,
                        }
                    } else if path.is_empty() {
                        // The road changed under it: plan again.
                        State::Idle
                    } else {
                        State::Driving { path, to }
                    }
                }
            };
        }
        moved
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::roads::{E, W};

    /// A straight east-west road along row 5, with a bus stop at each end.
    fn street(len: i32) -> (Layer, Terminals, TerminalId, TerminalId) {
        let mut roads = Layer::new(64, 16);
        for x in 2..2 + len {
            roads.set(x, 5, E | W);
        }
        let mut t = Terminals::default();
        let a = t.build(Site::BusStop { tile: (3, 5) }, "West".into());
        let b = t.build(Site::BusStop { tile: (len, 5) }, "East".into());
        (roads, t, a, b)
    }

    #[test]
    fn a_bus_shuttles_between_its_stops() {
        let (roads, terminals, a, b) = street(30);
        let mut buses = Buses::default();
        let id = buses.spawn(&roads, (3, 5)).unwrap();
        buses.set_orders(id, Orders::new(vec![b, a]));
        let mut calls = Vec::new();
        for _ in 0..1200 {
            buses.update(0.1, &roads, &terminals);
            calls.extend(buses.arrivals.drain(..).map(|(_, t)| t));
        }
        assert!(
            calls.len() >= 4,
            "{calls:?} {}",
            buses.get(id).unwrap().status(&terminals)
        );
        assert_eq!(&calls[..2], &[b, a]);
    }

    #[test]
    fn no_road_means_no_route_and_a_retry() {
        let (mut roads, terminals, a, b) = street(30);
        roads.set(15, 5, 0);
        let mut buses = Buses::default();
        let id = buses.spawn(&roads, (3, 5)).unwrap();
        buses.set_orders(id, Orders::new(vec![b, a]));
        buses.update(0.1, &roads, &terminals);
        assert!(buses.get(id).unwrap().status(&terminals).starts_with("No road"));
    }
}
