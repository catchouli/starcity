//! Planes: simple aircraft that fly a list of airports back and forth.
//!
//! Each leg is a precomputed list of waypoints (position, height and speed):
//! taxi to the runway end that points at the destination, roll and lift off,
//! climb out, cruise in a straight line, descend along the approach to the
//! destination runway (landing from the side facing where it came from), roll
//! out and taxi to the apron. Between waypoints a plane just flies a straight
//! line, accelerating towards each waypoint's speed. There's no air traffic
//! control: planes don't see each other.

use crate::terminals::{AIRPORT_LEN, AirportLayout, Orders, TerminalId, Terminals};
use crate::terrain::{BLOCK_HEIGHT, World};
use glam::{Vec2, Vec3, Vec3Swizzles};
use std::f32::consts::{PI, TAU};

/// Passengers a plane carries.
pub const CAPACITY: u32 = 40;
const CRUISE_SPEED: f32 = 9.0;
const ACCEL: f32 = 3.5;
const LOAD_SECONDS: f32 = 5.0;
const GROUND_TURN: f32 = 5.0;
const AIR_TURN: f32 = 2.0;
/// Least height above the terrain while airborne, in world units.
const CLEARANCE: f32 = 2.0;
/// Spacing of terrain-following waypoints along a cruise, in tiles.
const SAMPLE: f32 = 8.0;

#[derive(Clone, Copy, Debug)]
struct Wp {
    /// x, height, z in world units.
    p: Vec3,
    speed: f32,
}

#[derive(Clone, Debug)]
enum Stage {
    Parked {
        at: TerminalId,
        timer: f32,
        /// Landed here (so orders move on after loading), as opposed to just placed.
        arrived: bool,
    },
    Flying {
        path: Vec<Wp>,
        next: usize,
        to: TerminalId,
    },
}

pub struct Plane {
    pub id: u32,
    /// The year it was built, which decides what it looks like.
    pub born: i32,
    /// x, height, z in world units.
    pub pos: Vec3,
    /// Rotation for rendering: `atan2(-dz, dx)`.
    pub yaw: f32,
    pub orders: Orders,
    pub stopped: bool,
    pub passengers: u32,
    /// From the catalogue: how much faster than the standard cruise it flies, seats, and money.
    pub pace: f32,
    pub capacity: u32,
    pub price: u32,
    pub running: u32,
    /// Where and when (game seconds) the passengers on board got on, for the fare.
    pub boarded: Option<(f32, f32, f64)>,
    speed: f32,
    stage: Stage,
}

impl Plane {
    pub fn status(&self, terminals: &Terminals) -> String {
        let name = |id: TerminalId| terminals.get(id).map_or("?".to_string(), |t| t.name.clone());
        if self.stopped {
            return "Stopped".to_string();
        }
        match &self.stage {
            Stage::Parked { at, .. } if self.orders.list.is_empty() => {
                format!("Parked at {}: add airports below", name(*at))
            }
            Stage::Parked { at, .. } => format!("Loading at {}", name(*at)),
            Stage::Flying { to, .. } => format!("Flying to {}", name(*to)),
        }
    }

    /// Height above the ground, for the shadow and landing gear.
    pub fn is_airborne(&self, world: &World) -> bool {
        self.pos.y > world.height_at(self.pos.x, self.pos.z) + 0.15
    }

    fn active(&self) -> bool {
        !self.stopped && (matches!(self.stage, Stage::Flying { .. }) || !self.orders.list.is_empty())
    }
}

#[derive(Default)]
pub struct Planes {
    pub list: Vec<Plane>,
    /// The current year; new planes are built in it.
    pub year: i32,
    /// Planes that have just landed and parked: (plane, airport).
    pub arrivals: Vec<(u32, TerminalId)>,
    next_id: u32,
}

/// Cruising height: above the highest terrain the generator makes.
pub fn cruise_height(world: &World) -> f32 {
    (world.height as f32 - 8.0) * BLOCK_HEIGHT
}

impl Planes {
    /// Parks a plane at an airport.
    pub fn spawn(&mut self, terminals: &Terminals, at: TerminalId) -> Option<u32> {
        let layout = terminals.get(at)?.airport()?;
        let id = self.next_id;
        self.next_id += 1;
        let p = layout.parking();
        self.list.push(Plane {
            id,
            born: self.year,
            pos: Vec3::new(p.x, layout.ground() + 0.02, p.y),
            yaw: layout.yaw(),
            orders: Orders::default(),
            stopped: false,
            passengers: 0,
            pace: 1.0,
            capacity: CAPACITY,
            price: 24_000,
            running: 2_200,
            boarded: None,
            speed: 0.0,
            stage: Stage::Parked {
                at,
                timer: 1.0,
                arrived: false,
            },
        });
        Some(id)
    }

    pub fn get(&self, id: u32) -> Option<&Plane> {
        self.list.iter().find(|p| p.id == id)
    }

    pub fn get_mut(&mut self, id: u32) -> Option<&mut Plane> {
        self.list.iter_mut().find(|p| p.id == id)
    }

    pub fn remove(&mut self, id: u32) {
        self.list.retain(|p| p.id != id);
    }

    pub fn set_orders(&mut self, id: u32, orders: Orders) {
        if let Some(p) = self.get_mut(id) {
            p.orders = orders;
        }
    }

    /// An airport has gone. Parked planes there go with it; planes heading
    /// for it divert to their next stop, or are lost if they have none.
    pub fn forget_terminal(&mut self, gone: TerminalId, world: &World, terminals: &Terminals) {
        let cruise = cruise_height(world);
        self.list.retain_mut(|p| {
            p.orders.forget(gone);
            match &p.stage {
                Stage::Parked { at, .. } if *at == gone => return false,
                Stage::Flying { to, .. } if *to == gone => {
                    let Some(next) = p.orders.target() else { return false };
                    let Some(layout) = terminals.get(next).and_then(|t| t.airport()) else {
                        return false;
                    };
                    let mut path = fly_to(world, p.pos, layout, cruise);
                    path.extend(landing(world, layout, p.pos.xz(), cruise));
                    p.stage = Stage::Flying {
                        path,
                        next: 0,
                        to: next,
                    };
                }
                _ => {}
            }
            true
        });
    }

    /// Moves every plane on by `dt` seconds. Returns whether anything needs
    /// the next frame.
    pub fn update(&mut self, dt: f32, world: &World, terminals: &Terminals) -> bool {
        let cruise = cruise_height(world);
        let Planes { list, arrivals, .. } = self;
        let mut active = false;
        for plane in list.iter_mut() {
            if !plane.stopped {
                step(plane, dt, world, terminals, cruise, arrivals);
            }
            active |= plane.active();
        }
        active
    }
}

fn approach(current: f32, target: f32, max_change: f32) -> f32 {
    current + (target - current).clamp(-max_change, max_change)
}

fn turn_towards(current: f32, target: f32, max: f32) -> f32 {
    let diff = (target - current + PI).rem_euclid(TAU) - PI;
    current + diff.clamp(-max, max)
}

fn step(
    plane: &mut Plane,
    dt: f32,
    world: &World,
    terminals: &Terminals,
    cruise: f32,
    arrivals: &mut Vec<(u32, TerminalId)>,
) {
    match plane.stage.clone() {
        Stage::Parked { at, timer, arrived } => {
            let timer = timer - dt;
            if timer > 0.0 {
                plane.stage = Stage::Parked { at, timer, arrived };
                return;
            }
            if arrived {
                plane.orders.advance();
            }
            let here = terminals.get(at).and_then(|t| t.airport());
            let next = plane.orders.target().filter(|&t| t != at);
            let there = next.and_then(|t| terminals.get(t)).and_then(|t| t.airport());
            plane.stage = match (here, next, there) {
                (Some(from), Some(to), Some(dest)) => Stage::Flying {
                    path: plan_leg(world, from, dest, cruise),
                    next: 0,
                    to,
                },
                // Nowhere to go (yet): wait and look again.
                _ => Stage::Parked {
                    at,
                    timer: LOAD_SECONDS,
                    arrived: false,
                },
            };
        }
        Stage::Flying { path, mut next, to } => {
            let mut budget_left = true;
            // Speed follows the waypoint we're heading for.
            let target_speed = path.get(next).map_or(0.0, |w| w.speed) * plane.pace;
            plane.speed = approach(plane.speed, target_speed, ACCEL * dt);
            let mut budget = plane.speed * dt;
            let mut heading_to = None;
            while budget_left && next < path.len() {
                let d = path[next].p - plane.pos;
                let dist = d.length();
                if dist <= budget {
                    plane.pos = path[next].p;
                    budget -= dist;
                    next += 1;
                } else {
                    plane.pos += d / dist * budget;
                    budget_left = false;
                }
                if d.xz().length() > 1e-3 {
                    heading_to = Some((-d.z).atan2(d.x));
                }
            }
            if let Some(target) = heading_to {
                let rate = if plane.is_airborne(world) {
                    AIR_TURN
                } else {
                    GROUND_TURN
                };
                plane.yaw = turn_towards(plane.yaw, target, rate * dt);
            }
            if next >= path.len() {
                plane.speed = 0.0;
                plane.stage = Stage::Parked {
                    at: to,
                    timer: LOAD_SECONDS,
                    arrived: true,
                };
                arrivals.push((plane.id, to));
            } else {
                plane.stage = Stage::Flying { path, next, to };
            }
        }
    }
}

/// Where the runway ends are: (local u, direction pointing out past the end).
fn runway_ends(l: &AirportLayout) -> [(f32, Vec2); 2] {
    [(0.5, -l.u_dir()), (AIRPORT_LEN as f32 - 0.5, l.u_dir())]
}

fn ground_wp(l: &AirportLayout, u: f32, v: f32, speed: f32) -> Wp {
    let p = l.point(u, v);
    Wp {
        p: Vec3::new(p.x, l.ground() + 0.02, p.y),
        speed,
    }
}

/// Lifts a point to at least the terrain clearance.
fn clear_of_terrain(world: &World, mut p: Vec3) -> Vec3 {
    p.y = p.y.max(world.height_at(p.x, p.z) + CLEARANCE);
    p
}

/// Waypoints from the air at `from` to the start of the approach, following
/// the terrain at `SAMPLE` spacing.
fn fly_to(world: &World, from: Vec3, to: &AirportLayout, cruise: f32) -> Vec<Wp> {
    let origin = from.xz();
    let [(_, o0), (_, o1)] = runway_ends(to);
    let (c, centre) = (cruise, to.centre());
    let outward = if o0.dot(origin - centre) >= o1.dot(origin - centre) {
        o0
    } else {
        o1
    };
    let touch = centre + outward * (AIRPORT_LEN as f32 / 2.0 - 0.5);
    let f1 = touch + outward * 50.0;
    let (a, b) = (from, Vec3::new(f1.x, c, f1.y));
    let n = ((b - a).xz().length() / SAMPLE).ceil().max(1.0) as u32;
    (1..=n)
        .map(|i| {
            let t = i as f32 / n as f32;
            Wp {
                p: clear_of_terrain(world, a.lerp(b, t)),
                speed: CRUISE_SPEED,
            }
        })
        .collect()
}

/// The approach, touchdown, roll-out and taxi to the apron. The runway end
/// chosen is the one on the side facing `origin`.
fn landing(world: &World, to: &AirportLayout, origin: Vec2, cruise: f32) -> Vec<Wp> {
    let centre = to.centre();
    let ends = runway_ends(to);
    let facing = |e: &(f32, Vec2)| e.1.dot(origin - centre);
    let (land, far) = if facing(&ends[0]) >= facing(&ends[1]) {
        (ends[0], ends[1])
    } else {
        (ends[1], ends[0])
    };
    let g = to.ground();
    let touch = to.point(land.0, 0.5);
    let at = |dist: f32, y: f32, speed: f32| Wp {
        p: clear_of_terrain(
            world,
            Vec3::new(touch.x + land.1.x * dist, y, touch.y + land.1.y * dist),
        ),
        speed,
    };
    let drop = cruise - g;
    vec![
        // Level out at the start of the approach, then come down a glide slope.
        Wp {
            p: Vec3::new(touch.x + land.1.x * 50.0, cruise, touch.y + land.1.y * 50.0),
            speed: CRUISE_SPEED,
        },
        at(26.0, g + drop * 0.45, 7.5),
        at(10.0, g + drop * 0.12 + 0.6, 6.5),
        Wp {
            p: Vec3::new(touch.x, g + 0.02, touch.y),
            speed: 5.5,
        },
        ground_wp(to, far.0, 0.5, 1.8),
        ground_wp(to, far.0, 1.5, 1.6),
        ground_wp(to, AIRPORT_LEN as f32 / 2.0, 1.5, 1.0),
    ]
}

/// A whole leg: taxi, take off, climb, cruise, land, taxi in.
fn plan_leg(world: &World, from: &AirportLayout, to: &AirportLayout, cruise: f32) -> Vec<Wp> {
    let bearing = (to.centre() - from.centre()).normalize_or_zero();
    let ends = runway_ends(from);
    // Fly off the end that points most nearly at the destination.
    let (take_off, start) = if ends[0].1.dot(bearing) > ends[1].1.dot(bearing) {
        (ends[0], ends[1])
    } else {
        (ends[1], ends[0])
    };
    let roll_end = from.point(take_off.0, 0.5);
    let roll_start = from.point(start.0, 0.5);
    let g = from.ground();
    let lift = roll_start.lerp(roll_end, 0.8);
    let climb = |dist: f32, y: f32, turn: f32| {
        let p = roll_end + take_off.1 * dist + bearing * turn;
        clear_of_terrain(world, Vec3::new(p.x, y, p.y))
    };

    let mut path = vec![
        ground_wp(from, start.0, 1.5, 1.6),
        ground_wp(from, start.0, 0.5, 1.6),
        Wp {
            p: Vec3::new(lift.x, g + 0.02, lift.y),
            speed: 7.0,
        },
        Wp {
            p: climb(0.0, g + 0.9, 0.0),
            speed: 8.0,
        },
        Wp {
            p: climb(14.0, g + 3.5, 0.0),
            speed: CRUISE_SPEED,
        },
        Wp {
            p: climb(10.0, cruise, 24.0),
            speed: CRUISE_SPEED,
        },
    ];
    let last = path.last().unwrap().p;
    path.extend(fly_to(world, last, to, cruise));
    path.extend(landing(world, to, from.centre(), cruise));
    path
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminals::Site;
    use crate::terrain::{Block, Material};

    const LEVEL: i32 = 8;

    fn flat_world(n: usize) -> World {
        let mut w = World::new(n, n, 60, 6);
        for z in 0..n {
            for x in 0..n {
                for y in 0..LEVEL as usize {
                    w.set(x, y, z, Block::full(Material::Grass));
                }
            }
        }
        w
    }

    fn airports(a: (i32, i32, bool), b: (i32, i32, bool)) -> (Terminals, TerminalId, TerminalId) {
        let mut t = Terminals::default();
        let mut add = |(x0, z0, along_x): (i32, i32, bool), name: &str| {
            t.build(
                Site::Airport(AirportLayout {
                    x0,
                    z0,
                    along_x,
                    level: LEVEL,
                }),
                name.into(),
            )
        };
        let a = add(a, "Alpha");
        let b = add(b, "Bravo");
        (t, a, b)
    }

    #[test]
    fn a_plane_flies_back_and_forth_taking_off_and_landing_on_the_runway() {
        let mut world = flat_world(300);
        // A hill under the route, to be flown over.
        for x in 120..140 {
            for z in 0..300 {
                for y in LEVEL as usize..(LEVEL as usize + 20) {
                    world.set(x, y, z, Block::full(Material::Rock));
                }
            }
        }
        let (terminals, a, b) = airports((20, 100, true), (240, 130, false));
        let mut planes = Planes::default();
        let id = planes.spawn(&terminals, a).unwrap();
        planes.set_orders(id, Orders::new(vec![b, a]));
        let ground = LEVEL as f32 * BLOCK_HEIGHT;
        let (mut landed, mut max_height, mut ticks) = (Vec::new(), 0.0f32, 0);
        while landed.len() < 4 && ticks < 40_000 {
            planes.update(0.05, &world, &terminals);
            ticks += 1;
            let p = planes.get(id).unwrap();
            max_height = max_height.max(p.pos.y);
            // Never inside the terrain.
            assert!(
                p.pos.y >= world.height_at(p.pos.x, p.pos.z) - 0.05,
                "underground at {:?}",
                p.pos
            );
            for (_, at) in planes.arrivals.drain(..) {
                landed.push((at, ticks as f32 * 0.05));
            }
        }
        assert_eq!(landed.iter().map(|l| l.0).collect::<Vec<_>>(), vec![b, a, b, a]);
        assert!(max_height > cruise_height(&world) - 0.1, "cruised at {max_height}");
        // It ends up parked on the apron, on the ground.
        let p = planes.get(id).unwrap();
        let parking = terminals.get(a).unwrap().airport().unwrap().parking();
        assert!((p.pos.xz() - parking).length() < 0.1, "parked at {:?}", p.pos);
        assert!((p.pos.y - ground).abs() < 0.1);
        // 220 tiles at about 9 tiles/s: a leg is half a minute or so, not instant, not forever.
        let leg = landed[1].1 - landed[0].1;
        assert!((20.0..90.0).contains(&leg), "a leg took {leg} s");
    }

    #[test]
    fn a_removed_destination_diverts_or_loses_the_plane() {
        let world = flat_world(300);
        let (mut terminals, a, b) = airports((20, 100, true), (240, 100, true));
        let mut planes = Planes::default();
        let id = planes.spawn(&terminals, a).unwrap();
        planes.set_orders(id, Orders::new(vec![b]));
        for _ in 0..400 {
            planes.update(0.05, &world, &terminals);
        }
        assert!(planes.get(id).unwrap().status(&terminals).starts_with("Flying"));
        // Remove the destination mid-flight: with no other stop, the plane is lost.
        terminals.remove_in((240, 100), (249, 102));
        planes.forget_terminal(b, &world, &terminals);
        assert!(planes.get(id).is_none());
        // Parked planes go with their airport.
        let id = planes.spawn(&terminals, a).unwrap();
        let gone = terminals.remove_in((20, 100), (29, 102));
        assert_eq!(gone.len(), 1);
        planes.forget_terminal(a, &world, &terminals);
        assert!(planes.get(id).is_none());
    }

    #[test]
    fn a_plane_without_orders_just_sits() {
        let world = flat_world(100);
        let (terminals, a, _) = airports((20, 20, true), (60, 60, true));
        let mut planes = Planes::default();
        let id = planes.spawn(&terminals, a).unwrap();
        for _ in 0..100 {
            assert!(!planes.update(0.1, &world, &terminals));
        }
        assert!(planes.get(id).unwrap().status(&terminals).starts_with("Parked"));
    }

    /// 200 planes flying between two airports for a simulated minute.
    #[test]
    fn two_hundred_planes_are_cheap() {
        let world = flat_world(300);
        let (terminals, a, b) = airports((20, 100, true), (240, 130, false));
        let mut planes = Planes::default();
        for i in 0..200 {
            let (from, to) = if i % 2 == 0 { (a, b) } else { (b, a) };
            let id = planes.spawn(&terminals, from).unwrap();
            planes.set_orders(id, Orders::new(vec![to, from]));
        }
        let (mut total, mut worst) = (std::time::Duration::ZERO, std::time::Duration::ZERO);
        let mut times = Vec::new();
        let frames = 3600;
        for _ in 0..frames {
            let t = std::time::Instant::now();
            planes.update(1.0 / 60.0, &world, &terminals);
            let e = t.elapsed();
            total += e;
            worst = worst.max(e);
            times.push(e);
            planes.arrivals.clear();
        }
        let avg = total / frames;
        eprintln!("200 planes: {avg:?} per frame on average, worst {worst:?}");
        assert!(avg.as_micros() < 200, "average {avg:?}");
        // The slowest 1% of frames, not the single worst: a test run in parallel gets descheduled now and then.
        times.sort();
        let p99 = times[times.len() * 99 / 100];
        assert!(p99.as_millis() < 5, "99th percentile frame {p99:?} (worst {worst:?})");
    }
}
