//! Trains: movement along track, path signals and crashes.
//!
//! Track pieces join edge midpoints, so a train moves along a chain of
//! *segments* (a piece traversed in one direction). At the end of a segment it
//! can carry on onto any piece touching that point, in either tile, that turns
//! by at most 45°.
//!
//! Pieces are owned by at most one train. A train owns the pieces under it,
//! and before passing a path signal it reserves a whole path through the
//! block ahead (up to the next signal); if no free path exists it waits. On
//! unsignalled track a train only claims a piece as its head enters it, so
//! two trains can run into each other there, and they crash.

use crate::network::Layer;
use crate::rails::{Signals, piece_ends};
use crate::stations::{StationId, Stations};
use glam::Vec2;
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, HashSet, VecDeque};

pub const CAR_LENGTH: f32 = 0.8;
pub const CAR_GAP: f32 = 0.08;
const MAX_SPEED: f32 = 3.0;
const ACCELERATION: f32 = 1.5;
const BRAKING: f32 = 3.0;
/// How long a crashed train lies there before it's cleared away.
const WRECK_SECONDS: f32 = 3.0;
/// Longest path a signal reservation will search.
const MAX_BLOCK_SEGMENTS: usize = 64;
/// Time spent at each station.
pub const LOADING_SECONDS: f32 = 3.0;

type PieceKey = ((i32, i32), u8);

/// A track piece traversed in one direction.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Seg {
    pub tile: (i32, i32),
    pub piece: u8,
    /// Along the piece from its `piece_ends` start to its end.
    pub forward: bool,
}

impl Seg {
    /// Start and end points in world x/z.
    pub fn ends(self) -> (Vec2, Vec2) {
        let ((fx, fz), (tx, tz)) = piece_ends(self.piece);
        let o = Vec2::new(self.tile.0 as f32, self.tile.1 as f32);
        let (a, b) = (o + Vec2::new(fx, fz), o + Vec2::new(tx, tz));
        if self.forward { (a, b) } else { (b, a) }
    }

    pub fn len(self) -> f32 {
        let (a, b) = self.ends();
        a.distance(b)
    }

    pub fn dir(self) -> Vec2 {
        let (a, b) = self.ends();
        (b - a).normalize()
    }

    /// The point `t` along the segment from its start.
    pub fn point(self, t: f32) -> Vec2 {
        let (a, _) = self.ends();
        a + self.dir() * t
    }

    pub fn reversed(self) -> Seg {
        Seg {
            forward: !self.forward,
            ..self
        }
    }

    fn key(self) -> PieceKey {
        (self.tile, self.piece)
    }
}

/// Segments a train can move onto from the end of `seg`.
pub fn next_segments(rails: &Layer, seg: Seg) -> Vec<Seg> {
    let (_, end) = seg.ends();
    let dir = seg.dir();
    // The end point is an edge midpoint shared with one neighbouring tile.
    let local = end - Vec2::new(seg.tile.0 as f32, seg.tile.1 as f32);
    let across = if local.x > 0.99 {
        (1, 0)
    } else if local.x < 0.01 {
        (-1, 0)
    } else if local.y > 0.99 {
        (0, 1)
    } else {
        (0, -1)
    };
    let mut out = Vec::new();
    for tile in [seg.tile, (seg.tile.0 + across.0, seg.tile.1 + across.1)] {
        let bits = rails.get(tile.0, tile.1);
        for i in 0..6 {
            let piece = 1u8 << i;
            if bits & piece == 0 {
                continue;
            }
            for forward in [true, false] {
                let s = Seg { tile, piece, forward };
                if s.key() == seg.key() {
                    continue;
                }
                if s.ends().0.distance(end) < 1e-3 && s.dir().dot(dir) > 0.5 {
                    out.push(s);
                }
            }
        }
    }
    out
}

/// Whether passing onto `seg` means passing a signal: `Some(true)` the right
/// way (a block entry needing a reservation), `Some(false)` from behind
/// (not allowed), `None` for no signal.
fn signal_on(signals: &Signals, seg: Seg) -> Option<bool> {
    signals
        .by_tile
        .get(&seg.tile)
        .filter(|s| s.piece == seg.piece)
        .map(|s| s.forward == seg.forward)
}

#[derive(Clone, Copy, Debug)]
struct Entry {
    seg: Seg,
    /// Owned by this train (under it, or reserved through a signal). Planned
    /// segments on unsignalled track are only claimed when the head enters.
    claimed: bool,
}

pub struct Train {
    pub id: u32,
    pub cars: u32,
    /// Oldest (under the tail) first. `route[head]` holds the head; anything
    /// after it is the path ahead.
    route: VecDeque<Entry>,
    head: usize,
    head_pos: f32,
    pub speed: f32,
    /// Seconds left before a wreck is cleared away.
    pub crashed: Option<f32>,
    /// Waiting for a path through the block ahead.
    pub waiting: bool,
    /// Stations to visit in turn. With none, the train wanders.
    pub orders: Vec<StationId>,
    /// Index into `orders` of the next stop.
    pub order: usize,
    /// Seconds left loading at a station.
    pub loading: Option<f32>,
    /// Told to stop by the player.
    pub stopped: bool,
    pub passengers: u32,
    /// Distance to the target station from each segment, and the target and
    /// track version it was computed for.
    routing: Option<Routing>,
}

struct Routing {
    target: StationId,
    version: u64,
    dist: HashMap<Seg, f32>,
}

impl Train {
    /// Passenger seats: every car but the locomotive.
    pub fn capacity(&self) -> u32 {
        self.cars.saturating_sub(1) * 30
    }

    pub fn length(&self) -> f32 {
        self.cars as f32 * CAR_LENGTH + (self.cars.saturating_sub(1)) as f32 * CAR_GAP
    }

    /// The point `d` behind the head, following the track.
    pub fn point_behind(&self, mut d: f32) -> Vec2 {
        let mut i = self.head;
        let mut pos = self.head_pos;
        loop {
            let seg = self.route[i].seg;
            if d <= pos || i == 0 {
                return seg.point((pos - d).max(0.0));
            }
            d -= pos;
            i -= 1;
            pos = self.route[i].seg.len();
        }
    }

    /// Each car's (front, back) points.
    pub fn car_ends(&self) -> Vec<(Vec2, Vec2)> {
        (0..self.cars)
            .map(|c| {
                let front = c as f32 * (CAR_LENGTH + CAR_GAP);
                (self.point_behind(front), self.point_behind(front + CAR_LENGTH))
            })
            .collect()
    }

    pub fn tiles(&self) -> impl Iterator<Item = (i32, i32)> + '_ {
        self.route.iter().map(|e| e.seg.tile)
    }

    /// The station the train is heading for, if it has orders.
    pub fn target(&self) -> Option<StationId> {
        (!self.orders.is_empty()).then(|| self.orders[self.order % self.orders.len()])
    }

    /// What the train is doing, for the UI.
    pub fn status(&self, stations: &Stations) -> String {
        let name = |id: StationId| stations.get(id).map_or("?".to_string(), |s| s.name.clone());
        if self.crashed.is_some() {
            "Crashed!".into()
        } else if let (Some(_), Some(id)) = (self.loading, self.target()) {
            format!("Loading at {}", name(id))
        } else if self.stopped {
            "Stopped".into()
        } else if self.waiting {
            "Waiting at signal".into()
        } else if let Some(id) = self.target() {
            format!("Heading for {}", name(id))
        } else {
            "No orders: wandering".into()
        }
    }

    /// How promising a segment is for reaching the target: 0 inside the
    /// target station, otherwise the distance from its end, or infinity.
    fn rank(&self, stations: &Stations, seg: Seg) -> f32 {
        let (Some(target), Some(routing)) = (self.target(), &self.routing) else {
            return f32::INFINITY;
        };
        if stations.at(seg.tile) == Some(target) {
            return 0.0;
        }
        routing.dist.get(&seg).map_or(f32::INFINITY, |d| d + seg.len())
    }
}

/// Whether a train could get from station `from` to station `to`.
pub fn reachable(rails: &Layer, stations: &Stations, from: StationId, to: StationId) -> bool {
    let dist = distances(rails, stations, to);
    let Some(from) = stations.get(from) else { return false };
    dist.keys().any(|s| from.tiles.contains(&s.tile))
}

/// Shortest distance from the end of each segment to the start of a segment
/// in `station`, by Dijkstra backwards from the station's track.
fn distances(rails: &Layer, stations: &Stations, station: StationId) -> HashMap<Seg, f32> {
    // Integer costs (millitiles) so the heap can order them.
    let cost = |s: Seg| (s.len() * 1000.0) as u32;
    let previous = |t: Seg| -> Vec<Seg> {
        next_segments(rails, t.reversed())
            .into_iter()
            .map(Seg::reversed)
            .collect()
    };
    let mut best: HashMap<Seg, u32> = HashMap::new();
    let mut heap = BinaryHeap::new();
    let Some(station) = stations.get(station) else {
        return HashMap::new();
    };
    for &tile in &station.tiles {
        let piece = rails.get(tile.0, tile.1);
        for forward in [true, false] {
            let t = Seg { tile, piece, forward };
            for p in previous(t) {
                if best.get(&p).is_none_or(|&d| d > 0) {
                    best.insert(p, 0);
                    heap.push(Reverse((0, p.tile, p.piece, p.forward)));
                }
            }
        }
    }
    while let Some(Reverse((d, tile, piece, forward))) = heap.pop() {
        let s = Seg { tile, piece, forward };
        if best.get(&s).is_some_and(|&b| b < d) {
            continue;
        }
        for p in previous(s) {
            let nd = d + cost(s);
            if best.get(&p).is_none_or(|&b| nd < b) {
                best.insert(p, nd);
                heap.push(Reverse((nd, p.tile, p.piece, p.forward)));
            }
        }
    }
    best.into_iter().map(|(s, d)| (s, d as f32 / 1000.0)).collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpawnError {
    NoTrack,
    Junction,
    TooShort,
    Occupied,
}

#[derive(Default)]
pub struct Trains {
    pub list: Vec<Train>,
    /// Trains that have just stopped at a station, for the caller to load
    /// and unload: (train, station).
    pub arrivals: Vec<(u32, StationId)>,
    owner: HashMap<PieceKey, u32>,
    next_id: u32,
    rng: u64,
}

impl Trains {
    pub fn new(seed: u64) -> Trains {
        Trains {
            rng: seed | 1,
            ..Default::default()
        }
    }

    /// Whether any train owns (occupies or has reserved) this piece.
    pub fn is_owned(&self, tile: (i32, i32), piece: u8) -> bool {
        self.owner.contains_key(&(tile, piece))
    }

    /// Places a train with its head at the end of the piece on `tile`, facing
    /// `forward` along it, and its cars on the track behind.
    pub fn spawn(&mut self, rails: &Layer, tile: (i32, i32), forward: bool, cars: u32) -> Result<u32, SpawnError> {
        let bits = rails.get(tile.0, tile.1);
        if bits == 0 {
            return Err(SpawnError::NoTrack);
        }
        if bits.count_ones() > 1 {
            return Err(SpawnError::Junction);
        }
        let first = Seg {
            tile,
            piece: bits,
            forward,
        };
        let mut route = VecDeque::from([first]);
        let length = cars as f32 * CAR_LENGTH + (cars - 1) as f32 * CAR_GAP;
        let mut need = length - first.len();
        // Lay the rest of the train on the straightest track behind.
        while need > 0.0 {
            let behind = route[0].reversed();
            let Some(prev) = next_segments(rails, behind)
                .into_iter()
                .max_by(|a, b| a.dir().dot(behind.dir()).total_cmp(&b.dir().dot(behind.dir())))
            else {
                return Err(SpawnError::TooShort);
            };
            if route.iter().any(|s| s.key() == prev.key()) {
                return Err(SpawnError::TooShort);
            }
            need -= prev.len();
            route.push_front(prev.reversed());
        }
        if route.iter().any(|s| self.owner.contains_key(&s.key())) {
            return Err(SpawnError::Occupied);
        }
        let id = self.next_id;
        self.next_id += 1;
        for s in &route {
            self.owner.insert(s.key(), id);
        }
        let head = route.len() - 1;
        self.list.push(Train {
            id,
            cars,
            route: route.into_iter().map(|seg| Entry { seg, claimed: true }).collect(),
            head,
            head_pos: first.len(),
            speed: 0.0,
            crashed: None,
            waiting: false,
            orders: Vec::new(),
            order: 0,
            loading: None,
            stopped: false,
            passengers: 0,
            routing: None,
        });
        Ok(id)
    }

    /// Removes trains on any of these tiles. Returns whether any were removed.
    pub fn remove_on_tiles(&mut self, tiles: &HashSet<(i32, i32)>) -> bool {
        let doomed: Vec<u32> = self
            .list
            .iter()
            .filter(|t| t.tiles().any(|tile| tiles.contains(&tile)))
            .map(|t| t.id)
            .collect();
        for id in &doomed {
            self.remove(*id);
        }
        !doomed.is_empty()
    }

    /// Removes a train (e.g. sold by the player).
    pub fn remove(&mut self, id: u32) {
        self.list.retain(|t| t.id != id);
        self.owner.retain(|_, owner| *owner != id);
    }

    fn next_random(&mut self) -> u64 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        self.rng
    }

    /// Advances every train by `dt` seconds. Returns whether anything moved
    /// or changed (so the scene needs redrawing).
    pub fn update(&mut self, dt: f32, rails: &Layer, signals: &Signals, stations: &Stations, version: u64) -> bool {
        let mut changed = false;
        let ids: Vec<u32> = self.list.iter().map(|t| t.id).collect();
        for id in ids {
            self.refresh_routing(id, rails, stations, version);
            changed |= self.update_train(id, dt, rails, signals, stations);
        }
        // Clear away wrecks whose time is up.
        let done: Vec<u32> = self
            .list
            .iter()
            .filter(|t| t.crashed.is_some_and(|s| s <= 0.0))
            .map(|t| t.id)
            .collect();
        for id in done {
            self.remove(id);
            changed = true;
        }
        changed
    }

    pub fn train(&mut self, id: u32) -> &mut Train {
        self.list.iter_mut().find(|t| t.id == id).unwrap()
    }

    pub fn get(&self, id: u32) -> Option<&Train> {
        self.list.iter().find(|t| t.id == id)
    }

    /// Replaces a train's orders and which one it's heading for next.
    pub fn set_orders(&mut self, id: u32, orders: Vec<StationId>, current: usize) {
        let t = self.train(id);
        t.order = if orders.is_empty() { 0 } else { current % orders.len() };
        t.orders = orders;
        t.routing = None;
    }

    /// Drops a removed station from every train's orders.
    pub fn forget_station(&mut self, station: StationId) {
        for t in &mut self.list {
            t.orders.retain(|&s| s != station);
            t.routing = None;
        }
    }

    /// Recomputes a train's route distances if its target or the track changed.
    fn refresh_routing(&mut self, id: u32, rails: &Layer, stations: &Stations, version: u64) {
        let t = self.train(id);
        let Some(target) = t.target() else {
            t.routing = None;
            return;
        };
        if t.routing
            .as_ref()
            .is_some_and(|r| r.target == target && r.version == version)
        {
            return;
        }
        t.routing = Some(Routing {
            target,
            version,
            dist: distances(rails, stations, target),
        });
    }

    /// Sorts options best first for reaching the train's target (ties keep
    /// their shuffled order).
    fn by_rank(&self, id: u32, stations: &Stations, options: &mut [Seg]) {
        let t = self.list.iter().find(|t| t.id == id).unwrap();
        options.sort_by(|&a, &b| t.rank(stations, a).total_cmp(&t.rank(stations, b)));
    }

    fn update_train(&mut self, id: u32, dt: f32, rails: &Layer, signals: &Signals, stations: &Stations) -> bool {
        if let Some(left) = &mut self.train(id).crashed {
            *left -= dt;
            return true;
        }
        if let Some(left) = &mut self.train(id).loading {
            *left -= dt;
            if *left > 0.0 {
                return false;
            }
            let t = self.train(id);
            t.loading = None;
            t.order += 1;
            t.routing = None;
            return true;
        }

        // Make sure we know the track far enough ahead to stop in time.
        let speed = self.train(id).speed;
        let lookahead = speed * speed / (2.0 * BRAKING) + 1.5;
        let mut blocked = false;
        let mut dead_end = false;
        loop {
            let t = self.train(id);
            let ahead: f32 = t.route.iter().skip(t.head + 1).map(|e| e.seg.len()).sum::<f32>()
                + t.route[t.head].seg.len()
                - t.head_pos;
            if ahead >= lookahead {
                break;
            }
            match self.extend(id, rails, signals, stations) {
                Extend::Ok => {}
                Extend::Blocked => {
                    blocked = true;
                    break;
                }
                Extend::DeadEnd => {
                    dead_end = true;
                    break;
                }
            }
        }

        let station_stop = self.station_stop(id, stations);
        let t = self.train(id);
        let to_end =
            t.route.iter().skip(t.head + 1).map(|e| e.seg.len()).sum::<f32>() + t.route[t.head].seg.len() - t.head_pos;
        let mut stop_at = (blocked || dead_end).then_some(to_end);
        if let Some(d) = station_stop {
            stop_at = Some(stop_at.map_or(d, |s: f32| s.min(d)));
        }
        if t.stopped {
            stop_at = Some(0.0);
        }
        t.waiting = blocked;
        let arriving = station_stop.is_some_and(|d| d < 1e-3) && t.speed < 0.05;

        // Speed: accelerate to cruising speed, or brake to stop at the stop point.
        let target = stop_at.map_or(MAX_SPEED, |d| (2.0 * BRAKING * d.max(0.0)).sqrt().min(MAX_SPEED));
        t.speed = if t.speed < target {
            (t.speed + ACCELERATION * dt).min(target)
        } else {
            target
        };
        let mut step = t.speed * dt;
        if let Some(d) = stop_at {
            step = step.min(d.max(0.0));
        }
        if step <= 0.0 {
            if arriving {
                self.arrive(id, rails, stations);
                return true;
            }
            if dead_end && !self.train(id).stopped && stop_at.is_some_and(|d| d < 1e-3) {
                self.reverse(id);
                return true;
            }
            return false;
        }

        // Move the head, claiming segments as it enters them.
        let t = self.train(id);
        t.head_pos += step;
        let mut entered = Vec::new();
        while t.head_pos > t.route[t.head].seg.len() && t.head + 1 < t.route.len() {
            t.head_pos -= t.route[t.head].seg.len();
            t.head += 1;
            if !t.route[t.head].claimed {
                t.route[t.head].claimed = true;
                entered.push(t.route[t.head].seg.key());
            }
        }
        for key in entered {
            match self.owner.get(&key) {
                Some(&other) if other != id => self.crash(id, other),
                _ => {
                    self.owner.insert(key, id);
                }
            }
        }
        self.release_tail(id);
        true
    }

    /// Tries to add the next segment (or, at a signal, a whole reserved path)
    /// to the route ahead.
    fn extend(&mut self, id: u32, rails: &Layer, signals: &Signals, stations: &Stations) -> Extend {
        let last = self.train(id).route.back().unwrap().seg;
        let mut options: Vec<Seg> = next_segments(rails, last)
            .into_iter()
            .filter(|&s| signal_on(signals, s) != Some(false))
            .collect();
        if options.is_empty() {
            return Extend::DeadEnd;
        }
        self.shuffle(&mut options);
        self.by_rank(id, stations, &mut options);
        let needs_reservation = options.iter().any(|&s| signal_on(signals, s) == Some(true));
        if !needs_reservation {
            self.train(id).route.push_back(Entry {
                seg: options[0],
                claimed: false,
            });
            return Extend::Ok;
        }
        // Path signal: find a free path through the block ahead and reserve it all.
        for start in options {
            if let Some(path) = self.find_path(id, rails, signals, stations, start) {
                for s in &path {
                    self.owner.insert(s.key(), id);
                }
                let t = self.train(id);
                t.route.extend(path.into_iter().map(|seg| Entry { seg, claimed: true }));
                return Extend::Ok;
            }
        }
        Extend::Blocked
    }

    /// A free path from `start` to the end of its block: up to (not including)
    /// the next signal facing the same way, or a dead end.
    fn find_path(
        &mut self,
        id: u32,
        rails: &Layer,
        signals: &Signals,
        stations: &Stations,
        start: Seg,
    ) -> Option<Vec<Seg>> {
        let target = self.list.iter().find(|t| t.id == id).unwrap().target();
        let in_target = |s: Seg| target.is_some() && stations.at(s.tile) == target;
        let free = |owner: &HashMap<PieceKey, u32>, s: Seg| owner.get(&s.key()).is_none_or(|&o| o == id);
        if !free(&self.owner, start) {
            return None;
        }
        let mut path = vec![start];
        let mut stack: Vec<Vec<Seg>> = vec![];
        let mut visited = HashSet::from([start.key()]);
        loop {
            let last = *path.last().unwrap();
            let mut options: Vec<Seg> = next_segments(rails, last)
                .into_iter()
                .filter(|&s| signal_on(signals, s) != Some(false) && !visited.contains(&s.key()))
                .collect();
            // The block ends where the next signal begins, or at a dead end. A
            // train bound for a station only reserves as far as its platform end.
            if options.is_empty() || options.iter().any(|&s| signal_on(signals, s) == Some(true)) {
                return Some(path);
            }
            if in_target(last) && options.iter().all(|&s| !in_target(s)) {
                return Some(path);
            }
            if path.len() >= MAX_BLOCK_SEGMENTS {
                return Some(path);
            }
            options.retain(|&s| free(&self.owner, s));
            self.shuffle(&mut options);
            // Options are popped from the back, so put the best last.
            self.by_rank(id, stations, &mut options);
            options.reverse();
            stack.push(options);
            // Take the next untried option, backtracking when a branch runs out.
            loop {
                match stack.last_mut().and_then(|o| o.pop()) {
                    Some(s) => {
                        visited.insert(s.key());
                        path.push(s);
                        break;
                    }
                    None => {
                        stack.pop();
                        path.pop();
                        if path.is_empty() || stack.is_empty() {
                            return None;
                        }
                    }
                }
            }
        }
    }

    fn shuffle(&mut self, v: &mut [Seg]) {
        for i in (1..v.len()).rev() {
            let j = (self.next_random() % (i as u64 + 1)) as usize;
            v.swap(i, j);
        }
    }

    /// Distance from the head to where it should stop for its target station:
    /// the far end of the platform, if that's within the known route ahead.
    fn station_stop(&mut self, id: u32, stations: &Stations) -> Option<f32> {
        let t = self.list.iter().find(|t| t.id == id).unwrap();
        let target = t.target()?;
        let in_target = |i: usize| stations.at(t.route[i].seg.tile) == Some(target);
        let mut d = t.route[t.head].seg.len() - t.head_pos;
        let mut seen = false;
        for i in t.head..t.route.len() {
            if i > t.head {
                d += t.route[i].seg.len();
            }
            if in_target(i) {
                seen = true;
                if i + 1 == t.route.len() || !in_target(i + 1) {
                    return Some(d);
                }
            } else if seen {
                break;
            }
        }
        None
    }

    /// Stopped at the target platform: start loading. Releases anything
    /// reserved beyond the station, and turns round now if the next stop is
    /// only reachable that way.
    fn arrive(&mut self, id: u32, rails: &Layer, stations: &Stations) {
        let t = self.train(id);
        t.loading = Some(LOADING_SECONDS);
        t.speed = 0.0;
        if let Some(station) = t.target() {
            self.arrivals.push((id, station));
        }
        let t = self.train(id);
        let ahead: Vec<Entry> = t.route.drain(t.head + 1..).collect();
        for e in ahead.into_iter().filter(|e| e.claimed) {
            if self.owner.get(&e.seg.key()) == Some(&id) {
                self.owner.remove(&e.seg.key());
            }
        }
        // Peek at the next order: can we get there going forward, or only behind us?
        let t = self.train(id);
        let next_target = (t.orders.len() > 1).then(|| t.orders[(t.order + 1) % t.orders.len()]);
        if let Some(next) = next_target {
            let dist = distances(rails, stations, next);
            let t = self.list.iter().find(|t| t.id == id).unwrap();
            let best = |s: Seg| {
                next_segments(rails, s)
                    .into_iter()
                    .map(|o| {
                        if stations.at(o.tile) == Some(next) {
                            0.0
                        } else {
                            dist.get(&o).map_or(f32::INFINITY, |d| d + o.len())
                        }
                    })
                    .fold(f32::INFINITY, f32::min)
            };
            let ahead = best(t.route[t.head].seg);
            let behind = best(t.route[0].seg.reversed());
            if ahead.is_infinite() && behind.is_finite() {
                self.reverse(id);
                self.train(id).loading = Some(LOADING_SECONDS);
            }
        }
    }

    /// Drops segments the tail has fully left, releasing them.
    fn release_tail(&mut self, id: u32) {
        let t = self.train(id);
        let mut behind_head: f32 = t.route.iter().take(t.head).map(|e| e.seg.len()).sum::<f32>() + t.head_pos;
        let mut released = Vec::new();
        while t.head > 0 && behind_head - t.route[0].seg.len() >= t.length() {
            behind_head -= t.route[0].seg.len();
            let e = t.route.pop_front().unwrap();
            t.head -= 1;
            if e.claimed {
                released.push(e.seg.key());
            }
        }
        for key in released {
            if self.owner.get(&key) == Some(&id) {
                self.owner.remove(&key);
            }
        }
    }

    /// Turns a stopped train round: the tail becomes the head.
    fn reverse(&mut self, id: u32) {
        let t = self.train(id);
        // Forget the path ahead (it's a dead end), keeping only what's under the train.
        let ahead: Vec<Entry> = t.route.drain(t.head + 1..).collect();
        let behind_head: f32 = t.route.iter().take(t.head).map(|e| e.seg.len()).sum::<f32>() + t.head_pos;
        let tail_into_first = behind_head - t.length();
        let first_len = t.route[0].seg.len();
        t.route = t
            .route
            .iter()
            .rev()
            .map(|e| Entry {
                seg: e.seg.reversed(),
                ..*e
            })
            .collect();
        t.head = t.route.len() - 1;
        t.head_pos = first_len - tail_into_first.max(0.0);
        t.speed = 0.0;
        for e in ahead.into_iter().filter(|e| e.claimed) {
            if self.owner.get(&e.seg.key()) == Some(&id) {
                self.owner.remove(&e.seg.key());
            }
        }
    }

    fn crash(&mut self, a: u32, b: u32) {
        for id in [a, b] {
            let t = self.train(id);
            t.crashed = Some(WRECK_SECONDS);
            t.speed = 0.0;
        }
    }
}

enum Extend {
    Ok,
    Blocked,
    DeadEnd,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rails::{Signal, X};

    fn line(len: i32) -> Layer {
        let mut r = Layer::new(32, 8);
        for x in 1..=len {
            r.set(x, 2, X);
        }
        r
    }

    fn run(trains: &mut Trains, rails: &Layer, signals: &Signals, seconds: f32) {
        for _ in 0..(seconds * 60.0) as usize {
            trains.update(1.0 / 60.0, rails, signals, &Stations::default(), 0);
        }
    }

    #[test]
    fn straight_track_continues_and_curves_at_most_45_degrees() {
        let mut r = line(4);
        r.set(5, 2, X | crate::rails::SW);
        let east = Seg {
            tile: (4, 2),
            piece: X,
            forward: true,
        };
        let next = next_segments(&r, east);
        assert_eq!(next.len(), 2, "straight on, or the diagonal branch: {next:?}");
        // Going west from (1, 2) there's nothing.
        assert!(
            next_segments(
                &r,
                Seg {
                    tile: (1, 2),
                    piece: X,
                    forward: false
                }
            )
            .is_empty()
        );
    }

    #[test]
    fn train_runs_to_the_end_and_turns_round() {
        let r = line(20);
        let mut trains = Trains::new(1);
        let id = trains.spawn(&r, (5, 2), true, 3).unwrap();
        run(&mut trains, &r, &Signals::default(), 12.0);
        let t = &trains.list[0];
        assert_eq!(t.id, id);
        assert!(t.crashed.is_none());
        // It went east, hit the end of the line, reversed and is heading back west.
        let head = t.point_behind(0.0);
        assert!(head.x < 20.0, "head at {head}");
        assert!(t.route.back().unwrap().seg.forward == false);
    }

    #[test]
    fn head_on_on_unsignalled_track_crashes() {
        let r = line(24);
        let mut trains = Trains::new(2);
        trains.spawn(&r, (6, 2), true, 2).unwrap();
        trains.spawn(&r, (18, 2), false, 2).unwrap();
        run(&mut trains, &r, &Signals::default(), 4.0);
        assert!(trains.list.iter().all(|t| t.crashed.is_some()), "both trains crashed");
        run(&mut trains, &r, &Signals::default(), WRECK_SECONDS + 1.0);
        assert!(trains.list.is_empty(), "wrecks are cleared");
        assert!(trains.owner.is_empty());
    }

    #[test]
    fn signals_keep_trains_apart() {
        let r = line(24);
        let mut signals = Signals::default();
        // Signals facing each other, splitting the line into three blocks.
        signals.by_tile.insert(
            (10, 2),
            Signal {
                piece: X,
                forward: true,
            },
        );
        signals.by_tile.insert(
            (14, 2),
            Signal {
                piece: X,
                forward: false,
            },
        );
        let mut trains = Trains::new(3);
        trains.spawn(&r, (6, 2), true, 2).unwrap();
        trains.spawn(&r, (18, 2), false, 2).unwrap();
        let mut someone_waited = false;
        for _ in 0..20 * 60 {
            trains.update(1.0 / 60.0, &r, &signals, &Stations::default(), 0);
            someone_waited |= trains.list.iter().any(|t| t.waiting);
        }
        assert!(trains.list.iter().all(|t| t.crashed.is_none()), "no crash");
        assert!(someone_waited, "one train had to wait for the other's block");
    }

    #[test]
    fn spawning_needs_room() {
        let r = line(2);
        let mut trains = Trains::new(4);
        assert_eq!(trains.spawn(&r, (2, 2), true, 3).unwrap_err(), SpawnError::TooShort);
        assert_eq!(trains.spawn(&r, (5, 5), true, 3).unwrap_err(), SpawnError::NoTrack);
        let r = line(10);
        trains.spawn(&r, (5, 2), true, 2).unwrap();
        assert_eq!(trains.spawn(&r, (5, 2), true, 2).unwrap_err(), SpawnError::Occupied);
    }

    #[test]
    fn train_shuttles_between_two_stations() {
        use crate::terrain::{Block, Material, World};
        let mut w = World::new(32, 8, 4, 0);
        for x in 0..32 {
            for z in 0..8 {
                w.set(x, 0, z, Block::full(Material::Grass));
            }
        }
        let mut r = line(24);
        let mut stations = Stations::default();
        let none = Layer::new(32, 8);
        let sigs = Signals::default();
        let a = stations.plan(&w, &r, &none, &none, &sigs, (2, 2), (5, 2));
        stations.build(&a, &mut r);
        let b = stations.plan(&w, &r, &none, &none, &sigs, (18, 2), (21, 2));
        stations.build(&b, &mut r);
        let (sa, sb) = (stations.at((3, 2)).unwrap(), stations.at((19, 2)).unwrap());

        let mut trains = Trains::new(5);
        let id = trains.spawn(&r, (10, 2), true, 2).unwrap();
        trains.train(id).orders = vec![sb, sa];
        let mut loaded_at = Vec::new();
        for _ in 0..60 * 60 {
            trains.update(1.0 / 60.0, &r, &sigs, &stations, 1);
            let t = trains.get(id).unwrap();
            if t.loading.is_some() {
                let station = stations.at(t.route[t.head].seg.tile);
                if loaded_at.last() != Some(&station) {
                    loaded_at.push(station);
                }
            }
        }
        assert!(loaded_at.len() >= 3, "visited stations in turn: {loaded_at:?}");
        assert_eq!(&loaded_at[..3], &[Some(sb), Some(sa), Some(sb)]);
        assert!(trains.get(id).unwrap().crashed.is_none());
    }
}
