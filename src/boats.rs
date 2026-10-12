//! Boats: simple ferries that follow a list of ports.
//!
//! A boat sails a route found by [`Pathfinder`] over the water grid, ties up
//! at the port's berth for a few seconds (the app loads and unloads
//! passengers when it reports an arrival), then heads for the next port in
//! its orders. Routes are cached per pair of berths, so a ferry line searches
//! once, and searches are spread over frames, so a long route never stalls
//! one. Boats don't collide with each other.

use crate::sea::{Pathfinder, Route, RouteKey, Sea, centre};
use crate::terminals::{Orders, TerminalId, Terminals};
use glam::Vec2;
use std::collections::HashMap;
use std::f32::consts::{PI, TAU};
use std::rc::Rc;

/// Passengers a boat carries.
pub const CAPACITY: u32 = 40;
/// Tiles per second.
const SPEED: f32 = 2.5;
/// Radians per second the hull can swing round (the position follows the route exactly).
const TURN_RATE: f32 = 3.0;
const LOAD_SECONDS: f32 = 4.0;
/// A* node expansions per update, across all searches.
const SEARCH_BUDGET: usize = 3000;
const MAX_CACHED_ROUTES: usize = 256;

#[derive(Clone, Debug)]
enum State {
    /// Nowhere to go.
    Idle,
    /// Needs a route from this tile to a port.
    Route {
        from: (i32, i32),
        to: TerminalId,
    },
    /// The route is being searched for.
    Waiting {
        key: RouteKey,
        to: TerminalId,
    },
    Sailing {
        path: Vec<Vec2>,
        next: usize,
        to: TerminalId,
    },
    Docked {
        at: TerminalId,
        timer: f32,
    },
    /// The sea has no way there; try again if it changes.
    NoRoute {
        version: u32,
        to: TerminalId,
    },
}

pub struct Boat {
    pub id: u32,
    /// The year it was built, which decides what it looks like.
    pub born: i32,
    pub pos: Vec2,
    /// Rotation for rendering, like the other props: `atan2(-dz, dx)`.
    pub heading: f32,
    pub orders: Orders,
    pub stopped: bool,
    pub passengers: u32,
    /// Where and when (game seconds) the passengers on board got on, for the fare.
    pub boarded: Option<(f32, f32, f64)>,
    state: State,
}

impl Boat {
    pub fn status(&self, terminals: &Terminals) -> String {
        let name = |id: TerminalId| terminals.get(id).map_or("?".to_string(), |t| t.name.clone());
        if self.stopped {
            return "Stopped".to_string();
        }
        match &self.state {
            State::Idle => "No orders: add ports below".to_string(),
            State::Route { to, .. } | State::Waiting { to, .. } => format!("Planning a route to {}", name(*to)),
            State::Sailing { to, .. } => format!("Sailing to {}", name(*to)),
            State::Docked { at, .. } => format!("Loading at {}", name(*at)),
            State::NoRoute { to, .. } => format!("No way to reach {} by sea", name(*to)),
        }
    }

    fn tile(&self) -> (i32, i32) {
        (self.pos.x.floor() as i32, self.pos.y.floor() as i32)
    }

    /// Whether time passing changes anything about this boat.
    fn active(&self) -> bool {
        !self.stopped && !matches!(self.state, State::Idle | State::NoRoute { .. })
    }
}

#[derive(Default)]
pub struct Boats {
    pub list: Vec<Boat>,
    /// The current year; new boats are built in it.
    pub year: i32,
    /// Boats that have just tied up: (boat, port). The app loads and unloads.
    pub arrivals: Vec<(u32, TerminalId)>,
    routes: HashMap<RouteKey, Option<Rc<Route>>>,
    finder: Pathfinder,
    /// The sea version the cached routes are for.
    version: u32,
    next_id: u32,
}

impl Boats {
    /// Puts a boat on the water at a tile.
    pub fn spawn(&mut self, sea: &Sea, tile: (i32, i32)) -> Option<u32> {
        if !sea.is_water(tile.0, tile.1) {
            return None;
        }
        let id = self.next_id;
        self.next_id += 1;
        self.list.push(Boat {
            id,
            born: self.year,
            pos: centre(tile),
            heading: 0.0,
            orders: Orders::default(),
            stopped: false,
            passengers: 0,
            boarded: None,
            state: State::Idle,
        });
        Some(id)
    }

    pub fn get(&self, id: u32) -> Option<&Boat> {
        self.list.iter().find(|b| b.id == id)
    }

    pub fn get_mut(&mut self, id: u32) -> Option<&mut Boat> {
        self.list.iter_mut().find(|b| b.id == id)
    }

    pub fn remove(&mut self, id: u32) {
        self.list.retain(|b| b.id != id);
    }

    /// Replaces a boat's orders; it replans its route.
    pub fn set_orders(&mut self, id: u32, orders: Orders) {
        if let Some(b) = self.get_mut(id) {
            b.orders = orders;
            if !matches!(b.state, State::Docked { .. }) {
                b.state = State::Idle;
            }
        }
    }

    /// A port has gone: drop it from every boat's orders.
    pub fn forget_terminal(&mut self, gone: TerminalId) {
        for b in &mut self.list {
            b.orders.forget(gone);
            let targets = match &b.state {
                State::Route { to, .. }
                | State::Waiting { to, .. }
                | State::Sailing { to, .. }
                | State::NoRoute { to, .. } => *to == gone,
                State::Docked { at, .. } => *at == gone,
                State::Idle => false,
            };
            if targets {
                b.state = State::Idle;
            }
        }
    }

    /// Searches in progress and routes waiting to be found (for the debug window).
    pub fn searches_pending(&self) -> usize {
        self.finder.pending()
    }

    pub fn cached_routes(&self) -> usize {
        self.routes.len()
    }

    pub fn last_expansions(&self) -> usize {
        self.finder.last_expansions
    }

    /// Moves every boat on by `dt` seconds. Returns whether anything is
    /// happening that needs the next frame (sailing, loading, planning).
    pub fn update(&mut self, dt: f32, sea: &Sea, terminals: &Terminals) -> bool {
        if self.version != sea.version {
            self.sea_changed(sea);
        }
        let mut done = Vec::new();
        self.finder.work(sea, SEARCH_BUDGET, &mut done);
        for (key, route) in done {
            let route = route.map(Rc::new);
            if let Some(r) = &route {
                // The way back is the same way, reversed.
                let back: Route = r.iter().rev().copied().collect();
                self.routes.entry((key.1, key.0)).or_insert_with(|| Some(Rc::new(back)));
            }
            self.routes.insert(key, route);
        }
        if self.routes.len() > MAX_CACHED_ROUTES {
            self.routes.clear();
        }
        let Boats {
            list,
            arrivals,
            routes,
            finder,
            ..
        } = self;
        let mut active = false;
        for boat in list.iter_mut() {
            step(boat, dt, sea, terminals, routes, finder, arrivals);
            active |= boat.active();
        }
        active
    }

    /// Tiles changed between land and water: routes are stale, and a boat
    /// might be sitting on what's now land.
    fn sea_changed(&mut self, sea: &Sea) {
        self.version = sea.version;
        self.routes.clear();
        self.finder.reset();
        for b in &mut self.list {
            if !matches!(b.state, State::Docked { .. }) {
                b.state = State::Idle;
            }
            if !sea.is_water(b.tile().0, b.tile().1) {
                // Washed ashore: move to the nearest water, or give up.
                match sea.nearest_water(b.tile(), 8) {
                    Some(t) => b.pos = centre(t),
                    None => b.passengers = u32::MAX,
                }
            }
        }
        self.list.retain(|b| b.passengers != u32::MAX);
    }
}

fn turn_towards(current: f32, target: f32, max: f32) -> f32 {
    let diff = (target - current + PI).rem_euclid(TAU) - PI;
    current + diff.clamp(-max, max)
}

fn step(
    boat: &mut Boat,
    dt: f32,
    sea: &Sea,
    terminals: &Terminals,
    routes: &mut HashMap<RouteKey, Option<Rc<Route>>>,
    finder: &mut Pathfinder,
    arrivals: &mut Vec<(u32, TerminalId)>,
) {
    if boat.stopped {
        return;
    }
    // A few state changes can happen in one step (idle → route → sailing).
    for _ in 0..4 {
        match boat.state.clone() {
            State::Idle => {
                let Some(to) = boat.orders.target() else { return };
                boat.state = State::Route { from: boat.tile(), to };
            }
            State::Route { from, to } => {
                let Some(berth) = terminals.get(to).and_then(|t| t.berth()) else {
                    // A port that's gone, or not a port: skip it.
                    boat.orders.forget(to);
                    boat.state = State::Idle;
                    continue;
                };
                if from == berth {
                    boat.state = State::Docked {
                        at: to,
                        timer: LOAD_SECONDS,
                    };
                    arrivals.push((boat.id, to));
                    return;
                }
                let key = (from, berth);
                match routes.get(&key) {
                    Some(Some(route)) => {
                        let path: Vec<Vec2> = route.iter().map(|&t| centre(t)).collect();
                        let next = if path[0].distance(boat.pos) < 0.05 { 1 } else { 0 };
                        boat.state = State::Sailing { path, next, to };
                    }
                    Some(None) => {
                        boat.state = State::NoRoute {
                            version: sea.version,
                            to,
                        }
                    }
                    None => {
                        finder.request(key);
                        boat.state = State::Waiting { key, to };
                    }
                }
            }
            State::Waiting { key, to } => {
                if routes.contains_key(&key) {
                    boat.state = State::Route { from: key.0, to };
                    continue;
                }
                return;
            }
            State::Sailing { path, mut next, to } => {
                let mut budget = SPEED * dt;
                let mut heading_to = None;
                while budget > 0.0 && next < path.len() {
                    let d = path[next] - boat.pos;
                    let dist = d.length();
                    if dist <= budget {
                        boat.pos = path[next];
                        budget -= dist;
                        next += 1;
                    } else {
                        boat.pos += d / dist * budget;
                        budget = 0.0;
                    }
                    if dist > 1e-4 {
                        heading_to = Some((-d.y).atan2(d.x));
                    }
                }
                if let Some(target) = heading_to {
                    boat.heading = turn_towards(boat.heading, target, TURN_RATE * dt);
                }
                boat.state = if next >= path.len() {
                    arrivals.push((boat.id, to));
                    State::Docked {
                        at: to,
                        timer: LOAD_SECONDS,
                    }
                } else {
                    State::Sailing { path, next, to }
                };
                return;
            }
            State::Docked { at, timer } => {
                let timer = timer - dt;
                if timer > 0.0 {
                    boat.state = State::Docked { at, timer };
                    return;
                }
                boat.orders.advance();
                let berth = terminals.get(at).and_then(|t| t.berth());
                boat.state = match boat.orders.target() {
                    // Only one port to call at: stay and load again.
                    Some(to) if to == at => State::Docked {
                        at,
                        timer: LOAD_SECONDS,
                    },
                    Some(to) => State::Route {
                        from: berth.unwrap_or_else(|| boat.tile()),
                        to,
                    },
                    None => State::Idle,
                };
                return;
            }
            State::NoRoute { version, to } => {
                if version != sea.version {
                    boat.state = State::Route { from: boat.tile(), to };
                    continue;
                }
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminals::Site;
    use crate::terrain::{Block, Material, World};

    /// Open sea with a land wall across the middle and one gap at z = 10.
    fn walled_world() -> World {
        let n = 64;
        let mut w = World::new(n, n, 8, 4);
        for z in 0..n {
            for x in 0..n {
                if x == 32 && z != 10 {
                    for y in 0..5 {
                        w.set(x, y, z, Block::full(Material::Grass));
                    }
                } else {
                    w.set(x, 0, z, Block::full(Material::Sand));
                }
            }
        }
        w
    }

    fn two_ports() -> (Terminals, TerminalId, TerminalId) {
        let mut t = Terminals::default();
        let a = t.build(
            Site::Port {
                tile: (6, 41),
                berth: (6, 40),
            },
            "West".into(),
        );
        let b = t.build(
            Site::Port {
                tile: (58, 41),
                berth: (58, 40),
            },
            "East".into(),
        );
        (t, a, b)
    }

    #[test]
    fn a_ferry_shuttles_through_the_gap_and_reuses_its_routes() {
        let world = walled_world();
        let sea = Sea::build(&world);
        let (terminals, a, b) = two_ports();
        let mut boats = Boats::default();
        let id = boats.spawn(&sea, (6, 40)).unwrap();
        boats.set_orders(id, Orders::new(vec![b, a]));
        let mut visited = Vec::new();
        let mut max_search = 0;
        for _ in 0..20_000 {
            boats.update(0.05, &sea, &terminals);
            max_search = max_search.max(boats.last_expansions());
            let boat = boats.get(id).unwrap();
            assert!(
                sea.is_water(boat.pos.x as i32, boat.pos.y as i32),
                "{:?} is land",
                boat.pos
            );
            if boat.pos.x > 31.0 && boat.pos.x < 33.0 {
                assert!(
                    boat.pos.y < 11.0 && boat.pos.y > 9.0,
                    "crossed the wall at {:?}",
                    boat.pos
                );
            }
            for (_, port) in boats.arrivals.drain(..) {
                visited.push(port);
            }
            if visited.len() >= 4 {
                break;
            }
        }
        assert_eq!(visited, vec![b, a, b, a], "ferry goes back and forth");
        assert!(max_search <= SEARCH_BUDGET);
        // Two routes (each way) are cached after the first trip; later trips searched nothing.
        assert!(boats.cached_routes() <= 3, "{} routes", boats.cached_routes());
        assert_eq!(boats.searches_pending(), 0);
    }

    #[test]
    fn unreachable_ports_and_removed_ports_are_handled() {
        let mut world = walled_world();
        // Close the gap: two seas.
        for y in 0..5 {
            world.set(32, y, 10, Block::full(Material::Grass));
        }
        let mut sea = Sea::build(&world);
        let (mut terminals, a, b) = two_ports();
        let mut boats = Boats::default();
        let id = boats.spawn(&sea, (6, 40)).unwrap();
        boats.set_orders(id, Orders::new(vec![b]));
        for _ in 0..50 {
            boats.update(0.1, &sea, &terminals);
        }
        let status = boats.get(id).unwrap().status(&terminals);
        assert!(status.starts_with("No way"), "{status}");
        // Open the gap again: the boat tries again by itself.
        world.set(32, 0, 10, Block::full(Material::Sand));
        for y in 1..5 {
            world.set(32, y, 10, Block::AIR);
        }
        assert!(sea.refresh(&world, &[(32, 10)]));
        for _ in 0..2000 {
            boats.update(0.1, &sea, &terminals);
            if !boats.arrivals.is_empty() {
                break;
            }
        }
        assert_eq!(boats.arrivals.first().map(|a| a.1), Some(b));
        // A removed port leaves the orders.
        let gone = terminals.remove_in((58, 41), (58, 41));
        assert_eq!(gone.len(), 1);
        boats.forget_terminal(b);
        assert!(boats.get(id).unwrap().orders.list.is_empty());
        assert!(!boats.update(0.1, &sea, &terminals), "nothing left to do");
        let _ = a;
    }

    #[test]
    fn searches_are_spread_over_updates() {
        let world = crate::terrain::generation::generate(&crate::terrain::generation::GenParams::default());
        let sea = Sea::build(&world);
        let ring = |x: i32, z: i32| sea.nearest_water((x, z), 30).unwrap();
        let (from, to) = (ring(3, 3), ring(508, 508));
        let mut terminals = Terminals::default();
        let port = terminals.build(Site::Port { tile: to, berth: to }, "Far".into());
        let mut boats = Boats::default();
        let id = boats.spawn(&sea, from).unwrap();
        boats.set_orders(id, Orders::new(vec![port]));
        let mut updates = 0;
        while boats.get(id).unwrap().status(&terminals).starts_with("Planning") || updates == 0 {
            boats.update(1.0 / 60.0, &sea, &terminals);
            assert!(boats.last_expansions() <= SEARCH_BUDGET);
            updates += 1;
            assert!(updates < 200, "never found a route");
        }
        assert!(
            updates > 1,
            "a cross-map route should take several updates, took {updates}"
        );
        assert!(boats.get(id).unwrap().status(&terminals).starts_with("Sailing"));
    }

    /// 200 boats on a generated map, all sailing between eight ports for a
    /// simulated minute at 60 fps: the cost per frame has to stay small enough
    /// for the web build (which runs two to three times slower than native).
    #[test]
    fn two_hundred_boats_are_cheap() {
        let world = crate::terrain::generation::generate(&crate::terrain::generation::GenParams::default());
        let sea = Sea::build(&world);
        let ring = |x: i32, z: i32| sea.nearest_water((x, z), 40).unwrap();
        let spots = [
            (5, 5),
            (250, 4),
            (506, 5),
            (506, 250),
            (506, 506),
            (250, 507),
            (5, 506),
            (4, 250),
        ];
        let mut terminals = Terminals::default();
        let ports: Vec<_> = spots
            .iter()
            .map(|&(x, z)| {
                let berth = ring(x, z);
                terminals.build(Site::Port { tile: berth, berth }, "Port".into())
            })
            .collect();
        let mut boats = Boats::default();
        for i in 0..200 {
            let at = terminals.get(ports[i % 8]).unwrap().berth().unwrap();
            let id = boats.spawn(&sea, at).unwrap();
            let orders = vec![ports[(i + 1 + i / 8) % 8], ports[i % 8]];
            boats.set_orders(id, Orders::new(orders));
        }
        let (mut total, mut worst) = (std::time::Duration::ZERO, std::time::Duration::ZERO);
        let frames = 3600;
        for _ in 0..frames {
            let t = std::time::Instant::now();
            boats.update(1.0 / 60.0, &sea, &terminals);
            let e = t.elapsed();
            total += e;
            worst = worst.max(e);
            boats.arrivals.clear();
        }
        let avg = total / frames;
        eprintln!(
            "200 boats: {avg:?} per frame on average, worst {worst:?}, {} routes cached",
            boats.cached_routes()
        );
        assert!(avg.as_micros() < 400, "average {avg:?}");
        assert!(worst.as_millis() < 6, "worst frame {worst:?}");
    }
}
