//! Towns: named settlements of roads and buildings that grow outwards.
//!
//! A town starts as a crossroads at a flat spot. Each growth step it extends
//! a road, puts up a house beside a road, or adds a floor to a building near
//! its centre. It uses the same road planner as the player, so towns join up
//! with player roads and, as they grow, with each other. Passengers delivered
//! to a town's stations make it grow faster.

use crate::models::Models;
use crate::network::Layer;
use crate::roads;
use crate::terrain::World;
use std::collections::BTreeMap;

pub type TownId = u32;

/// How far a station's catchment reaches from its platforms, in tiles.
pub const CATCHMENT: i32 = 4;

pub struct Town {
    pub id: TownId,
    pub name: String,
    pub centre: (i32, i32),
    /// Progress towards the next growth step.
    growth: f32,
    /// Recent passenger service (decays over time); speeds up growth.
    pub served: f32,
    pub delivered: u32,
    /// Population, counted every few seconds (it is a pass over every building).
    pub pop: u32,
    /// Seconds of food and of goods the town has left in its shops.
    pub food: f32,
    pub goods: f32,
    /// Seconds of nano-goods and lunar glass: the future's luxuries, a big reason to grow.
    pub luxury: f32,
    /// A working power station is within 40 tiles: a little faster growth.
    pub powered: bool,
    /// How much reason it has to grow (1 to 5), recomputed monthly from its connections, service,
    /// supply, industry and compute nearby (§5.10).
    pub prosperity: f32,
}

/// Above this many people a town wants food, and above the second it wants goods too (§5.5).
pub const WANTS_FOOD: u32 = 500;
pub const WANTS_GOODS: u32 = 1500;
/// One delivery that covers a town's needs lasts three game months.
pub const SUPPLY_SECONDS: f32 = 90.0;

impl Town {
    /// How fast it grows compared with a town that has what it wants: slowed without food or goods,
    /// and a little faster while both are on the shelves.
    pub fn growth_factor(&self) -> f32 {
        let mut f = if self.powered { 1.15 } else { 1.0 } * self.prosperity.max(1.0);
        if self.pop > WANTS_FOOD {
            f *= if self.food > 0.0 { 1.25 } else { 0.4 };
        }
        if self.pop > WANTS_GOODS {
            f *= if self.goods > 0.0 { 1.15 } else { 0.6 };
        }
        f
    }

    /// Units of a cargo that cover its need for three months.
    pub fn need(&self) -> f32 {
        (self.pop as f32 / 20.0).max(10.0)
    }

    /// A delivery of anything a town buys.
    pub fn receive(&mut self, cargo: crate::cargo::Cargo, units: u32) {
        use crate::cargo::Cargo;
        match cargo {
            Cargo::Food => self.stock(false, units),
            Cargo::Goods => self.stock(true, units),
            Cargo::NanoGoods | Cargo::LunarGlass => {
                let seconds = units as f32 / self.need() * SUPPLY_SECONDS * 2.0;
                self.luxury = (self.luxury + seconds).min(4.0 * SUPPLY_SECONDS);
            }
            _ => {}
        }
    }

    /// A delivery of food (or goods): puts that much on the shelves, up to six months' worth.
    pub fn stock(&mut self, goods: bool, units: u32) {
        let seconds = units as f32 / self.need() * SUPPLY_SECONDS;
        let shelf = if goods { &mut self.goods } else { &mut self.food };
        *shelf = (*shelf + seconds).min(2.0 * SUPPLY_SECONDS);
    }
}

/// A building. Big ones (an orb on 3 x 3 tiles) have an entry on every tile
/// they cover: the north-west tile is the anchor, with `size` set, and the
/// rest point back at it with `part_of`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Building {
    pub town: TownId,
    pub floors: u8,
    /// Picks between models of the same kind.
    pub style: u8,
    /// The year it was built, which decides what it looks like. Old buildings
    /// are redeveloped now and then, taking the look of the year they're
    /// rebuilt in.
    pub year: i32,
    /// Footprint in tiles (on the anchor).
    pub size: (u8, u8),
    pub part_of: Option<(i32, i32)>,
}

impl Building {
    pub fn population(self) -> u32 {
        if self.part_of.is_some() {
            return 0;
        }
        self.floors as u32 * people_per_floor(self.year) * self.size.0 as u32 * self.size.1 as u32
    }
}

/// People living on a floor of a building built in `year`: eight until 2020, thirty by 2050 (§5.10).
pub fn people_per_floor(year: i32) -> u32 {
    8 + ((year - 2020).clamp(0, 30) as u32 * 22) / 30
}

/// The most floors a building can have in `year`: 14 until 2025, 40 by 2035, 90 by 2050 or so.
pub fn floor_cap(year: i32) -> u32 {
    if year <= 2025 {
        (8 + (year - 2000).max(0) / 8).min(14) as u32
    } else {
        (14 + (year - 2025) as u32 * 26 / 10).min(90)
    }
}

/// What makes a town want to grow (§5.10), each counted with diminishing returns.
#[derive(Clone, Copy, Debug, Default)]
pub struct Reasons {
    /// Stations serving it.
    pub stations: u32,
    /// Recent passenger service (`Town::served`).
    pub served: f32,
    pub fed: bool,
    pub goods: bool,
    /// Nano-goods or lunar glass on the shelves.
    pub luxury: bool,
    /// The hyper expressway reaches it.
    pub expressway: bool,
    /// Kinds of working industry within 30 tiles.
    pub industry_kinds: u32,
    /// A server farm, data centre or AI campus within 30 tiles.
    pub compute: bool,
}

/// The Prosperity score: 1 for a town with no reason to grow, up to 5 for a powerhouse.
pub fn prosperity(r: &Reasons) -> f32 {
    let p = 1.0
        + 0.35 * r.stations.min(4) as f32
        + 0.8 * (r.served / 50.0).min(1.0)
        + if r.fed { 0.4 } else { 0.0 }
        + if r.goods { 0.4 } else { 0.0 }
        + if r.luxury { 0.6 } else { 0.0 }
        + if r.expressway { 0.8 } else { 0.0 }
        + 0.1 * r.industry_kinds.min(8) as f32
        + if r.compute { 0.5 } else { 0.0 };
    p.min(5.0)
}

/// What a place of this many people is called.
pub fn size_name(pop: u32) -> &'static str {
    match pop {
        0..300 => "Village",
        300..2_000 => "Town",
        2_000..20_000 => "City",
        20_000..200_000 => "Metropolis",
        200_000..2_000_000 => "Megacity",
        _ => "Arcology city",
    }
}

/// Buildings last about this many years before they're considered for
/// redevelopment, and are replaced at about this rate.
const MIN_AGE: i32 = 12;
const RENEWAL_SECONDS: f32 = 20.0 * 365.0;

pub struct Towns {
    pub list: Vec<Town>,
    pub buildings: BTreeMap<(i32, i32), Building>,
    /// 1 on tiles with a building, so roads and track avoid them.
    pub occupied: Layer,
    /// The current year; new buildings are built in it.
    pub year: i32,
    /// Progress towards the next redevelopment.
    renewal: f32,
    pop_clock: f32,
    /// How fast towns grow right now, from the world's events.
    pub growth_mult: f32,
    rng: u64,
}

const PREFIXES: [&str; 24] = [
    "Ash", "Brook", "Elm", "Oak", "Kings", "Mill", "Stone", "Thorn", "Bram", "Hawk", "Lang", "Rother", "Pen", "Cal",
    "Dun", "Wil", "Hart", "Fern", "Gold", "Marl", "Red", "Wex", "Chester", "Holm",
];
const SUFFIXES: [&str; 16] = [
    "ford", "ley", "ton", "bury", "field", "wick", "ham", "stead", "by", "worth", "mouth", "bridge", "dale", "combe",
    "haven", "well",
];

impl Towns {
    pub fn empty(size_x: usize, size_z: usize) -> Towns {
        Towns {
            list: Vec::new(),
            buildings: BTreeMap::new(),
            occupied: Layer::new(size_x, size_z),
            year: 2000,
            renewal: 0.0,
            pop_clock: 0.0,
            growth_mult: 1.0,
            rng: 1,
        }
    }

    /// Places towns on flat land and grows each a little. Builds their roads
    /// into `roads`.
    /// `year` is the year the game starts in: towns have grown over the
    /// 30 years before it, so their buildings are of different ages.
    pub fn generate(world: &World, roads_layer: &mut Layer, rails: &Layer, seed: u32, year: i32) -> Towns {
        let mut towns = Towns::empty(world.size_x, world.size_z);
        towns.year = year;
        towns.rng = (seed as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1;
        let size = world.size_x as i32;
        let wanted = ((size * size) / 9000).clamp(2, 24) as usize;
        let spacing = (size / 5).max(20);

        // Score candidate centres by how much flat dry land surrounds them.
        let flat = |x: i32, z: i32| {
            world
                .surface(x, z)
                .is_some_and(|(base, shape)| shape.0 == 0 && base > world.sea_level)
        };
        let mut candidates = Vec::new();
        for z in (8..size - 8).step_by(4) {
            for x in (8..size - 8).step_by(4) {
                if !flat(x, z) {
                    continue;
                }
                let score = (-5..=5)
                    .flat_map(|dz| (-5..=5).map(move |dx| (dx, dz)))
                    .filter(|&(dx, dz)| flat(x + dx, z + dz))
                    .count();
                if score >= 70 {
                    let jitter = towns.next() % 20;
                    candidates.push((score as u64 * 10 + jitter, (x, z)));
                }
            }
        }
        candidates.sort_by_key(|x| std::cmp::Reverse(x.0));
        for (_, c) in candidates {
            if towns.list.len() >= wanted {
                break;
            }
            let far_enough = towns.list.iter().all(|t| {
                let (dx, dz) = (t.centre.0 - c.0, t.centre.1 - c.1);
                dx * dx + dz * dz >= spacing * spacing
            });
            if far_enough {
                towns.found(world, roads_layer, rails, c);
            }
        }
        // Give each town a head start of varying size.
        for i in 0..towns.list.len() {
            let steps = 30 + towns.next() % 90;
            for step in 0..steps {
                // The head start spans the 30 years before the game begins.
                towns.year = year - 30 + (step * 30 / steps) as i32;
                towns.grow(i, world, roads_layer, rails);
            }
        }
        towns.year = year;
        towns.count_people();
        towns
    }

    fn next(&mut self) -> u64 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        self.rng
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n.max(1)
    }

    fn unique_name(&mut self) -> String {
        for _ in 0..100 {
            let name = format!(
                "{}{}",
                PREFIXES[self.below(PREFIXES.len() as u64) as usize],
                SUFFIXES[self.below(SUFFIXES.len() as u64) as usize]
            );
            if self.list.iter().all(|t| t.name != name) {
                return name;
            }
        }
        format!("Town {}", self.list.len() + 1)
    }

    /// Founds a town: a crossroads at `centre`.
    fn found(&mut self, world: &World, roads_layer: &mut Layer, rails: &Layer, centre: (i32, i32)) {
        let id = self.list.len() as TownId;
        let name = self.unique_name();
        self.list.push(Town {
            id,
            name,
            centre,
            growth: 0.0,
            served: 0.0,
            delivered: 0,
            pop: 0,
            food: 0.0,
            goods: 0.0,
            luxury: 0.0,
            powered: false,
            prosperity: 1.0,
        });
        for dir in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let len = 3 + self.below(4) as i32;
            for l in (1..=len).rev() {
                let end = (centre.0 + dir.0 * l, centre.1 + dir.1 * l);
                let plan = roads::plan(world, roads_layer, rails, &self.occupied, centre, end);
                if plan.blocked.is_none() {
                    plan.build(roads_layer);
                    break;
                }
            }
        }
    }

    pub fn population(&self, town: TownId) -> u32 {
        self.buildings
            .values()
            .filter(|b| b.town == town)
            .map(|b| b.population())
            .sum()
    }

    /// How far from its centre a town builds, growing with its population.
    fn radius(&self, town: TownId) -> i32 {
        // Big cities keep widening, out to 70 tiles.
        (4.0 + (self.population(town) as f32).sqrt() * 0.6).min(70.0) as i32
    }

    /// The town nearest a tile, within `max` tiles of its centre.
    pub fn nearest(&self, tile: (i32, i32), max: i32) -> Option<TownId> {
        self.list
            .iter()
            .map(|t| {
                let (dx, dz) = (t.centre.0 - tile.0, t.centre.1 - tile.1);
                (dx * dx + dz * dz, t.id)
            })
            .filter(|&(d, _)| d <= max * max)
            .min()
            .map(|(_, id)| id)
    }

    /// Counts every town's people into `Town::pop` (a pass over every building).
    pub fn count_people(&mut self) {
        let mut pops = vec![0u32; self.list.len()];
        for b in self.buildings.values() {
            if let Some(p) = pops.get_mut(b.town as usize) {
                *p += b.population();
            }
        }
        for (t, p) in self.list.iter_mut().zip(pops) {
            t.pop = p;
        }
    }

    /// Runs growth over `dt` seconds. Returns (road tiles changed, whether buildings changed).
    pub fn update(
        &mut self,
        dt: f32,
        world: &World,
        roads_layer: &mut Layer,
        rails: &Layer,
        models: &Models,
    ) -> (Vec<(i32, i32)>, bool) {
        let mut road_tiles = Vec::new();
        let mut built = false;
        self.pop_clock += dt;
        if self.pop_clock >= 10.0 || self.list.iter().all(|t| t.pop == 0) {
            self.pop_clock = 0.0;
            self.count_people();
        }
        for i in 0..self.list.len() {
            let town = &mut self.list[i];
            town.served *= 0.995f32.powf(dt);
            town.food = (town.food - dt).max(0.0);
            town.goods = (town.goods - dt).max(0.0);
            town.luxury = (town.luxury - dt).max(0.0);
            // Steady growth, faster with good passenger service and with the shops stocked.
            town.growth += dt * (0.05 + town.served * 0.01) * town.growth_factor() * self.growth_mult;
            while self.list[i].growth >= 1.0 {
                self.list[i].growth -= 1.0;
                let (roads_built, b) = self.grow(i, world, roads_layer, rails);
                road_tiles.extend(roads_built);
                built |= b;
            }
        }
        // Old buildings are slowly replaced with ones of the current look.
        self.renewal += dt * self.buildings.len() as f32 / RENEWAL_SECONDS;
        while self.renewal >= 1.0 {
            self.renewal -= 1.0;
            built |= self.renew_one(world, roads_layer, rails, models);
        }
        (road_tiles, built)
    }

    /// Redevelops one old building whose replacement would look different:
    /// rebuilt as a current model in place, or (once there are models that big)
    /// merged with its neighbours into a larger building. Returns whether
    /// anything changed.
    fn renew_one(&mut self, world: &World, roads_layer: &Layer, rails: &Layer, models: &Models) -> bool {
        let year = self.year;
        // Candidates: old anchors that would get a different model today.
        for _ in 0..16 {
            if self.buildings.is_empty() {
                return false;
            }
            let n = self.below(self.buildings.len() as u64) as usize;
            let (&tile, &b) = self.buildings.iter().nth(n).unwrap();
            if b.part_of.is_some() || year - b.year < MIN_AGE {
                continue;
            }
            let seed = (tile.0 as u32).wrapping_mul(73856093) ^ (tile.1 as u32).wrapping_mul(19349663);
            let now = models.pick_building(year, b.floors, b.size, seed.wrapping_add(b.style as u32));
            let then = models.pick_building(b.year, b.floors, b.size, seed.wrapping_add(b.style as u32));
            if now == then {
                continue;
            }
            // Big buildings first, where there's room and a model for them.
            if b.size == (1, 1) && b.floors >= 4 {
                let biggest = models.largest_building_footprint(year);
                for side in (2..=biggest).rev() {
                    if let Some(anchor) = self.room_for(world, roads_layer, rails, tile, b.town, side) {
                        let floors = (b.floors + 4).max(8);
                        if models.has_building(year, floors, (side, side)) {
                            self.merge(anchor, side, b.town, floors, year);
                            return true;
                        }
                    }
                }
            }
            let style = self.below(4) as u8;
            let entry = self.buildings.get_mut(&tile).unwrap();
            entry.year = year;
            entry.style = style;
            return true;
        }
        false
    }

    /// Winds the town back to `year`: buildings from later years are re-aged
    /// to just before it, and buildings too big to have existed (orbs) go
    /// back to ordinary ones. For seeking back through time when testing.
    pub fn rewind(&mut self, year: i32) {
        let big: Vec<(i32, i32)> = self
            .buildings
            .iter()
            .filter(|(_, b)| b.part_of.is_none() && b.size != (1, 1) && b.year > year)
            .map(|(t, _)| *t)
            .collect();
        for anchor in big {
            let b = self.buildings[&anchor];
            for dx in 0..b.size.0 as i32 {
                for dz in 0..b.size.1 as i32 {
                    if (dx, dz) != (0, 0) {
                        let t = (anchor.0 + dx, anchor.1 + dz);
                        self.buildings.remove(&t);
                        self.occupied.set(t.0, t.1, 0);
                    }
                }
            }
            let entry = self.buildings.get_mut(&anchor).unwrap();
            entry.size = (1, 1);
            entry.floors = entry.floors.min(6);
        }
        let tiles: Vec<(i32, i32)> = self
            .buildings
            .iter()
            .filter(|(_, b)| b.year > year)
            .map(|(t, _)| *t)
            .collect();
        for t in tiles {
            let age = self.below(10) as i32;
            self.buildings.get_mut(&t).unwrap().year = year - age;
        }
        self.year = year;
        self.count_people();
    }

    /// Where a `side` x `side` building containing `tile` could stand: every
    /// tile either free flat land at the same height or a small building of
    /// this town, and no road or track.
    fn room_for(
        &self,
        world: &World,
        roads_layer: &Layer,
        rails: &Layer,
        tile: (i32, i32),
        town: TownId,
        side: u8,
    ) -> Option<(i32, i32)> {
        let s = side as i32;
        let level = world.surface(tile.0, tile.1)?.0;
        for ox in 0..s {
            for oz in 0..s {
                let anchor = (tile.0 - ox, tile.1 - oz);
                let fits = (0..s).all(|dx| {
                    (0..s).all(|dz| {
                        let t = (anchor.0 + dx, anchor.1 + dz);
                        let flat = world
                            .surface(t.0, t.1)
                            .is_some_and(|(base, shape)| shape.0 == 0 && base == level && base > world.sea_level);
                        let clear = roads_layer.get(t.0, t.1) == 0 && rails.get(t.0, t.1) == 0;
                        let ours_or_free = match self.buildings.get(&t) {
                            Some(b) => b.town == town && b.part_of.is_none() && b.size == (1, 1),
                            None => self.occupied.get(t.0, t.1) == 0,
                        };
                        flat && clear && ours_or_free
                    })
                });
                if fits {
                    return Some(anchor);
                }
            }
        }
        None
    }

    /// Replaces whatever is on a `side` x `side` area with one big building.
    fn merge(&mut self, anchor: (i32, i32), side: u8, town: TownId, floors: u8, year: i32) {
        let style = self.below(4) as u8;
        for dx in 0..side as i32 {
            for dz in 0..side as i32 {
                let t = (anchor.0 + dx, anchor.1 + dz);
                let is_anchor = (dx, dz) == (0, 0);
                self.buildings.insert(
                    t,
                    Building {
                        town,
                        floors: if is_anchor { floors } else { 0 },
                        style,
                        year,
                        size: if is_anchor { (side, side) } else { (1, 1) },
                        part_of: if is_anchor { None } else { Some(anchor) },
                    },
                );
                self.occupied.set(t.0, t.1, 1);
            }
        }
    }

    /// One growth step for town `i`. Returns (road tiles built, whether a building changed).
    fn grow(&mut self, i: usize, world: &World, roads_layer: &mut Layer, rails: &Layer) -> (Vec<(i32, i32)>, bool) {
        let id = self.list[i].id;
        let centre = self.list[i].centre;
        let radius = self.radius(id);
        let near = |t: (i32, i32), r: i32| {
            let (dx, dz) = (t.0 - centre.0, t.1 - centre.1);
            dx * dx + dz * dz <= r * r
        };
        // Road tiles within reach, to grow from.
        let mut road_tiles = Vec::new();
        for z in centre.1 - radius..=centre.1 + radius {
            for x in centre.0 - radius..=centre.0 + radius {
                if roads_layer.get(x, z) != 0 && near((x, z), radius) {
                    road_tiles.push((x, z));
                }
            }
        }
        if road_tiles.is_empty() {
            return (Vec::new(), false);
        }
        let free = |towns: &Towns, t: (i32, i32)| {
            world.in_bounds(t.0, t.1)
                && roads_layer.get(t.0, t.1) == 0
                && rails.get(t.0, t.1) == 0
                && towns.occupied.get(t.0, t.1) == 0
                && world
                    .surface(t.0, t.1)
                    .is_some_and(|(base, shape)| shape.0 == 0 && base > world.sea_level)
        };
        for _ in 0..8 {
            let roll = self.below(100);
            let from = road_tiles[self.below(road_tiles.len() as u64) as usize];
            let dir = [(1, 0), (-1, 0), (0, 1), (0, -1)][self.below(4) as usize];
            if roll < 30 {
                // Extend a road, keeping a gap from parallel roads so houses fit between.
                let len = 2 + self.below(3) as i32;
                let end = (from.0 + dir.0 * len, from.1 + dir.1 * len);
                if !near(end, radius + 2) {
                    continue;
                }
                let side = (dir.1, dir.0);
                let crowded = (1..=len).any(|l| {
                    let t = (from.0 + dir.0 * l, from.1 + dir.1 * l);
                    roads_layer.get(t.0, t.1) != 0
                        || roads_layer.get(t.0 + side.0, t.1 + side.1) != 0
                        || roads_layer.get(t.0 - side.0, t.1 - side.1) != 0
                });
                if crowded {
                    continue;
                }
                let plan = roads::plan(world, roads_layer, rails, &self.occupied, from, end);
                if plan.blocked.is_none() {
                    return (plan.build(roads_layer), false);
                }
            } else if roll < 85 {
                // A new house beside the road.
                let t = (from.0 + dir.0, from.1 + dir.1);
                if free(self, t) {
                    let style = self.below(4) as u8;
                    self.buildings.insert(
                        t,
                        Building {
                            town: id,
                            floors: 1,
                            style,
                            year: self.year,
                            size: (1, 1),
                            part_of: None,
                        },
                    );
                    self.occupied.set(t.0, t.1, 1);
                    return (Vec::new(), true);
                }
            } else {
                // Taller buildings towards the centre as the town grows.
                // Later years build taller.
                let cap = floor_cap(self.year).min(255);
                let max_floors = (1 + self.population(id) / 300).min(cap) as u8;
                let inner = radius / 2 + 1;
                let pick: Vec<(i32, i32)> = self
                    .buildings
                    .iter()
                    .filter(|(t, b)| {
                        b.town == id
                            && b.part_of.is_none()
                            && b.size == (1, 1)
                            && b.floors < max_floors
                            && near(**t, inner)
                    })
                    .map(|(t, _)| *t)
                    .collect();
                if !pick.is_empty() {
                    let t = pick[self.below(pick.len() as u64) as usize];
                    self.buildings.get_mut(&t).unwrap().floors += 1;
                    return (Vec::new(), true);
                }
            }
        }
        (Vec::new(), false)
    }

    /// Removes buildings in an inclusive rectangle (a big building goes
    /// entirely if any tile of it is inside). Returns whether any were removed.
    pub fn demolish(&mut self, a: (i32, i32), b: (i32, i32)) -> bool {
        let (x0, x1) = (a.0.min(b.0), a.0.max(b.0));
        let (z0, z1) = (a.1.min(b.1), a.1.max(b.1));
        let mut anchors: Vec<(i32, i32)> = self
            .buildings
            .iter()
            .filter(|((x, z), _)| (x0..=x1).contains(x) && (z0..=z1).contains(z))
            .map(|(t, b)| b.part_of.unwrap_or(*t))
            .collect();
        anchors.sort();
        anchors.dedup();
        for anchor in &anchors {
            let Some(b) = self.buildings.get(anchor).copied() else {
                continue;
            };
            for dx in 0..b.size.0 as i32 {
                for dz in 0..b.size.1 as i32 {
                    let t = (anchor.0 + dx, anchor.1 + dz);
                    self.buildings.remove(&t);
                    self.occupied.set(t.0, t.1, 0);
                }
            }
        }
        !anchors.is_empty()
    }

    /// People living within a station's catchment of any of these tiles.
    pub fn population_near(&self, tiles: &[(i32, i32)]) -> u32 {
        let Some(&first) = tiles.first() else { return 0 };
        let (mut x0, mut z0, mut x1, mut z1) = (first.0, first.1, first.0, first.1);
        for &(x, z) in tiles {
            (x0, z0, x1, z1) = (x0.min(x), z0.min(z), x1.max(x), z1.max(z));
        }
        let mut total = 0;
        for z in z0 - CATCHMENT..=z1 + CATCHMENT {
            for x in x0 - CATCHMENT..=x1 + CATCHMENT {
                if let Some(b) = self.buildings.get(&(x, z)) {
                    total += b.population();
                }
            }
        }
        total
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terrain::generation::{self, GenParams};

    #[test]
    fn towns_are_founded_named_and_grow() {
        let p = GenParams {
            size: 256,
            ..Default::default()
        };
        let world = generation::generate(&p);
        let mut roads_layer = Layer::new(256, 256);
        let rails = Layer::new(256, 256);
        let mut towns = Towns::generate(&world, &mut roads_layer, &rails, 7, 2000);
        assert!(towns.list.len() >= 2, "{} towns", towns.list.len());
        let names: std::collections::HashSet<_> = towns.list.iter().map(|t| t.name.clone()).collect();
        assert_eq!(names.len(), towns.list.len(), "names are unique");
        for t in &towns.list {
            assert!(roads_layer.get(t.centre.0, t.centre.1) != 0, "{} has roads", t.name);
            assert!(towns.population(t.id) > 0, "{} has people", t.name);
        }
        // Buildings sit beside roads, never on them.
        for (&(x, z), _) in &towns.buildings {
            assert_eq!(roads_layer.get(x, z), 0);
            let beside = [(1, 0), (-1, 0), (0, 1), (0, -1)]
                .iter()
                .any(|(dx, dz)| roads_layer.get(x + dx, z + dz) != 0);
            assert!(beside, "building at ({x}, {z}) is beside a road");
        }
        let before: u32 = towns.list.iter().map(|t| towns.population(t.id)).sum();
        for _ in 0..600 {
            towns.update(1.0, &world, &mut roads_layer, &rails, &crate::models::Models::default());
        }
        let after: u32 = towns.list.iter().map(|t| towns.population(t.id)).sum();
        assert!(after > before, "towns grow: {before} -> {after}");
    }

    #[test]
    fn towns_modernise_over_the_years_and_can_be_rewound() {
        let models = crate::models::Models::load_embedded();
        let p = GenParams {
            size: 256,
            ..Default::default()
        };
        let world = generation::generate(&p);
        let mut roads_layer = Layer::new(256, 256);
        let rails = Layer::new(256, 256);
        let mut towns = Towns::generate(&world, &mut roads_layer, &rails, 7, 2000);
        // Buildings begin with a spread of ages before 2000.
        let oldest = towns.buildings.values().map(|b| b.year).min().unwrap();
        assert!(
            oldest < 1990 && towns.buildings.values().all(|b| b.year <= 2000),
            "oldest {oldest}"
        );
        let before = towns.buildings.values().filter(|b| b.year < 2000).count();

        for year in 2001..=2046 {
            towns.year = year;
            for _ in 0..12 {
                towns.update(30.0, &world, &mut roads_layer, &rails, &models);
            }
        }
        let anchors: Vec<_> = towns.buildings.iter().filter(|(_, b)| b.part_of.is_none()).collect();
        let recent = anchors.iter().filter(|(_, b)| b.year >= 2025).count();
        assert!(
            recent * 3 > anchors.len(),
            "{recent} of {} buildings are from 2025 or later",
            anchors.len()
        );
        assert!(anchors.iter().any(|(_, b)| b.year >= 2030), "nothing was redeveloped");
        let old_left = towns.buildings.values().filter(|b| b.year < 2000).count();
        assert!(
            old_left * 4 < before.max(1) * 3,
            "{old_left} of {before} old buildings are still standing"
        );
        // Big buildings appear once there are models for them, and all of them have a footprint on the map.
        let big: Vec<_> = anchors.iter().filter(|(_, b)| b.size != (1, 1)).collect();
        assert!(!big.is_empty(), "no merged buildings by 2046");
        for ((x, z), b) in big.iter().map(|(t, b)| (**t, **b)) {
            for dx in 0..b.size.0 as i32 {
                for dz in 0..b.size.1 as i32 {
                    let part = towns
                        .buildings
                        .get(&(x + dx, z + dz))
                        .expect("every tile of a big building is recorded");
                    assert_eq!(part.part_of, if (dx, dz) == (0, 0) { None } else { Some((x, z)) });
                    assert_eq!(towns.occupied.get(x + dx, z + dz), 1);
                    assert_eq!(roads_layer.get(x + dx, z + dz), 0, "a big building covers a road");
                }
            }
        }
        // Population counts each big building once.
        let sum: u32 = towns.list.iter().map(|t| towns.population(t.id)).sum();
        assert_eq!(sum, anchors.iter().map(|(_, b)| b.population()).sum::<u32>());

        // Winding back to 2005: nothing from later, and no orbs.
        towns.rewind(2005);
        assert!(towns.buildings.values().all(|b| b.year <= 2005));
        assert!(
            towns
                .buildings
                .values()
                .all(|b| b.part_of.is_none() && b.size == (1, 1))
        );
        let occupied: usize = (0..256)
            .flat_map(|z| (0..256).map(move |x| (x, z)))
            .filter(|&(x, z)| towns.occupied.get(x, z) != 0)
            .count();
        assert_eq!(occupied, towns.buildings.len(), "freed tiles are free again");
    }

    fn blank(pop: u32) -> Town {
        Town {
            id: 0,
            name: String::new(),
            centre: (0, 0),
            growth: 0.0,
            served: 0.0,
            delivered: 0,
            pop,
            food: 0.0,
            goods: 0.0,
            luxury: 0.0,
            powered: false,
            prosperity: 1.0,
        }
    }

    #[test]
    fn big_towns_want_food_then_goods_and_grow_faster_when_fed() {
        assert_eq!(blank(300).growth_factor(), 1.0, "small towns don't care");
        let mut t = blank(800);
        assert!((t.growth_factor() - 0.4).abs() < 1e-6);
        t.stock(false, t.need() as u32);
        assert!(t.food > 80.0 && (t.growth_factor() - 1.25).abs() < 1e-6);
        let mut big = blank(2000);
        assert!((big.growth_factor() - 0.24).abs() < 1e-6, "no food, no goods");
        big.stock(false, 1000);
        big.stock(true, 1000);
        assert!(big.food <= 2.0 * SUPPLY_SECONDS, "the shelves fill up");
        assert!(big.growth_factor() > 1.4);
    }

    #[test]
    fn shelves_empty_over_time_and_the_town_slows_again() {
        let (world, mut roads, rails, mut towns) = {
            let p = crate::terrain::generation::GenParams::default();
            let world = crate::terrain::generation::generate(&p);
            let roads = Layer::new(world.size_x, world.size_z);
            let rails = Layer::new(world.size_x, world.size_z);
            let mut r2 = Layer::new(world.size_x, world.size_z);
            let towns = Towns::generate(&world, &mut r2, &rails, p.seed, 2000);
            (world, roads, rails, towns)
        };
        let _ = &mut roads;
        towns.list[0].pop = 900;
        towns.list[0].stock(false, 200);
        let models = crate::models::Models::default();
        let mut r = Layer::new(world.size_x, world.size_z);
        for _ in 0..100 {
            towns.update(1.0, &world, &mut r, &rails, &models);
        }
        assert!(
            (towns.list[0].food - 80.0).abs() < 0.5,
            "a full shelf (six months) less 100 seconds: {}",
            towns.list[0].food
        );
        for _ in 0..100 {
            towns.update(1.0, &world, &mut r, &rails, &models);
        }
        assert_eq!(towns.list[0].food, 0.0);
    }

    #[test]
    fn cities_get_taller_and_denser_with_the_years_and_have_names() {
        assert_eq!(people_per_floor(2000), 8);
        assert_eq!(people_per_floor(2050), 30);
        assert_eq!(floor_cap(2000), 8);
        assert_eq!(floor_cap(2025), 11);
        assert!(floor_cap(2035) >= 40 && floor_cap(2050) <= 90);
        assert_eq!(size_name(150), "Village");
        assert_eq!(size_name(25_000), "Metropolis");
    }

    #[test]
    fn prosperity_rewards_every_reason_to_grow_and_tops_out() {
        assert_eq!(prosperity(&Reasons::default()), 1.0);
        let fed = Reasons {
            stations: 2,
            served: 60.0,
            fed: true,
            goods: true,
            ..Default::default()
        };
        assert!(prosperity(&fed) > 3.0);
        let all = Reasons {
            stations: 9,
            served: 900.0,
            fed: true,
            goods: true,
            luxury: true,
            expressway: true,
            industry_kinds: 20,
            compute: true,
        };
        assert_eq!(prosperity(&all), 5.0);
    }
}
