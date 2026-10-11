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
use glam::Vec2;
use std::collections::{HashMap, HashSet, VecDeque};

pub const CAR_LENGTH: f32 = 0.8;
pub const CAR_GAP: f32 = 0.08;
const MAX_SPEED: f32 = 3.0;
const ACCELERATION: f32 = 1.5;
const BRAKING: f32 = 3.0;
/// How long a crashed train lies there before it's cleared away.
const WRECK_SECONDS: f32 = 3.0;
/// Longest path a signal reservation will search.
const MAX_BLOCK_SEGMENTS: usize = 64;

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
}

impl Train {
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

    fn remove(&mut self, id: u32) {
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
    pub fn update(&mut self, dt: f32, rails: &Layer, signals: &Signals) -> bool {
        let mut changed = false;
        let ids: Vec<u32> = self.list.iter().map(|t| t.id).collect();
        for id in ids {
            changed |= self.update_train(id, dt, rails, signals);
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

    fn train(&mut self, id: u32) -> &mut Train {
        self.list.iter_mut().find(|t| t.id == id).unwrap()
    }

    fn update_train(&mut self, id: u32, dt: f32, rails: &Layer, signals: &Signals) -> bool {
        if let Some(left) = &mut self.train(id).crashed {
            *left -= dt;
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
            match self.extend(id, rails, signals) {
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

        let t = self.train(id);
        let stop_at = (blocked || dead_end).then(|| {
            t.route.iter().skip(t.head + 1).map(|e| e.seg.len()).sum::<f32>() + t.route[t.head].seg.len() - t.head_pos
        });
        t.waiting = blocked;

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
            if dead_end && stop_at.is_some_and(|d| d < 1e-3) {
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
    fn extend(&mut self, id: u32, rails: &Layer, signals: &Signals) -> Extend {
        let last = self.train(id).route.back().unwrap().seg;
        let mut options: Vec<Seg> = next_segments(rails, last)
            .into_iter()
            .filter(|&s| signal_on(signals, s) != Some(false))
            .collect();
        if options.is_empty() {
            return Extend::DeadEnd;
        }
        self.shuffle(&mut options);
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
            if let Some(path) = self.find_path(id, rails, signals, start) {
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
    fn find_path(&mut self, id: u32, rails: &Layer, signals: &Signals, start: Seg) -> Option<Vec<Seg>> {
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
            // The block ends where the next signal begins, or at a dead end.
            if options.is_empty() || options.iter().any(|&s| signal_on(signals, s) == Some(true)) {
                return Some(path);
            }
            if path.len() >= MAX_BLOCK_SEGMENTS {
                return Some(path);
            }
            options.retain(|&s| free(&self.owner, s));
            self.shuffle(&mut options);
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
            trains.update(1.0 / 60.0, rails, signals);
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
            trains.update(1.0 / 60.0, &r, &signals);
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
}
