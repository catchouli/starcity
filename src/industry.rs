//! Industries: where cargo comes from and goes to (GAME-DESIGN.md §5). Primary
//! industries (mines, forests, farms) make cargo from nothing; secondary ones
//! (mills, plants, the factory) turn what you deliver into something dearer.
//! Placement, the monthly production step and acceptance are all here, tested
//! without a window.

// Some of the table is for the next steps (production drift, closures, the funding menu).
#![allow(dead_code)]

use crate::cargo::{self, Cargo};
use crate::network::Layer;
use crate::power;
use crate::terrain::edit::{self, Corners};
use crate::terrain::{Material, World};
use crate::towns::Towns;
use std::collections::HashMap;

/// A secondary industry holds this much of each input; past it, deliveries are refused.
pub const STOCK_CAP: u32 = 400;

/// How far (tiles) from any platform of a station an industry still counts as served.
pub const CATCHMENT: i32 = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    CoalMine,
    IronOreMine,
    Forest,
    Farm,
    PowerStation,
    SteelMill,
    Sawmill,
    FoodPlant,
    Factory,
    ServerFarm,
    SilicaQuarry,
    ChipFab,
    DataCentre,
    AiCampus,
    OilWell,
    Refinery,
    Spaceport,
    RegolithWorks,
    FusionPlant,
    NanoFab,
    ConvergenceArray,
}

impl Kind {
    pub const ALL: [Kind; 21] = [
        Kind::CoalMine,
        Kind::IronOreMine,
        Kind::Forest,
        Kind::Farm,
        Kind::PowerStation,
        Kind::SteelMill,
        Kind::Sawmill,
        Kind::FoodPlant,
        Kind::Factory,
        Kind::ServerFarm,
        Kind::SilicaQuarry,
        Kind::ChipFab,
        Kind::DataCentre,
        Kind::AiCampus,
        Kind::OilWell,
        Kind::Refinery,
        Kind::Spaceport,
        Kind::RegolithWorks,
        Kind::FusionPlant,
        Kind::NanoFab,
        Kind::ConvergenceArray,
    ];

    /// The model kind in `assets/models/industries/`.
    pub fn key(self) -> &'static str {
        match self {
            Kind::CoalMine => "coal_mine",
            Kind::IronOreMine => "iron_ore_mine",
            Kind::Forest => "forest",
            Kind::Farm => "farm",
            Kind::PowerStation => "power_station",
            Kind::SteelMill => "steel_mill",
            Kind::Sawmill => "sawmill",
            Kind::FoodPlant => "food_plant",
            Kind::Factory => "factory",
            Kind::ServerFarm => "server_farm",
            Kind::SilicaQuarry => "silica_quarry",
            Kind::ChipFab => "chip_fab",
            Kind::DataCentre => "data_centre",
            Kind::AiCampus => "ai_campus",
            Kind::OilWell => "oil_well",
            Kind::Refinery => "refinery",
            Kind::Spaceport => "spaceport",
            Kind::RegolithWorks => "regolith_works",
            Kind::FusionPlant => "fusion_plant",
            Kind::NanoFab => "nano_fab",
            Kind::ConvergenceArray => "convergence_array",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Kind::CoalMine => "Coal mine",
            Kind::IronOreMine => "Iron ore mine",
            Kind::Forest => "Forest",
            Kind::Farm => "Farm",
            Kind::PowerStation => "Power station",
            Kind::SteelMill => "Steel mill",
            Kind::Sawmill => "Sawmill",
            Kind::FoodPlant => "Food plant",
            Kind::Factory => "Factory",
            Kind::ServerFarm => "Server farm",
            Kind::SilicaQuarry => "Silica quarry",
            Kind::ChipFab => "Chip fab",
            Kind::DataCentre => "Data centre",
            Kind::AiCampus => "AI campus",
            Kind::OilWell => "Oil well",
            Kind::Refinery => "Refinery",
            Kind::Spaceport => "Spaceport",
            Kind::RegolithWorks => "Regolith works",
            Kind::FusionPlant => "Fusion plant",
            Kind::NanoFab => "Nano-fabricator",
            Kind::ConvergenceArray => "Convergence Array",
        }
    }

    /// Footprint in tiles (square).
    pub fn size(self) -> i32 {
        match self {
            Kind::CoalMine
            | Kind::IronOreMine
            | Kind::Forest
            | Kind::Sawmill
            | Kind::FoodPlant
            | Kind::ServerFarm
            | Kind::SilicaQuarry
            | Kind::OilWell => 3,
            Kind::Farm
            | Kind::PowerStation
            | Kind::SteelMill
            | Kind::Factory
            | Kind::ChipFab
            | Kind::DataCentre
            | Kind::Refinery
            | Kind::RegolithWorks
            | Kind::FusionPlant
            | Kind::NanoFab => 4,
            Kind::AiCampus | Kind::Spaceport | Kind::ConvergenceArray => 5,
        }
    }

    /// What the council charges to found one (GAME-DESIGN.md §5.4).
    pub fn fund_cost(self) -> u32 {
        match self {
            Kind::ServerFarm => 30_000,
            Kind::PowerStation => 60_000,
            Kind::ChipFab => 80_000,
            Kind::DataCentre => 100_000,
            Kind::AiCampus => 250_000,
            Kind::Spaceport => 250_000,
            Kind::Refinery => 60_000,
            Kind::RegolithWorks => 150_000,
            Kind::FusionPlant => 400_000,
            Kind::NanoFab => 300_000,
            Kind::ConvergenceArray => 5_000_000,
            k if k.is_primary() => 20_000,
            _ => 40_000,
        }
    }

    /// The first year it can be built.
    pub fn first_year(self) -> i32 {
        match self {
            Kind::DataCentre => 2005,
            Kind::Spaceport => 2027,
            Kind::RegolithWorks => 2032,
            Kind::FusionPlant => 2035,
            Kind::NanoFab => 2040,
            Kind::ConvergenceArray => 2046,
            Kind::AiCampus => 2020,
            _ => 2000,
        }
    }

    /// Whether new ones are founded in `year` (server farms give way to the cloud).
    pub fn available(self, year: i32) -> bool {
        year >= self.first_year() && (self != Kind::ServerFarm || year <= 2015)
    }

    /// How many a default map starts with (scaled by area).
    fn count(self) -> f32 {
        match self {
            Kind::CoalMine => 4.0,
            Kind::IronOreMine => 3.0,
            Kind::Forest => 5.0,
            Kind::Farm => 8.0,
            Kind::PowerStation => 3.0,
            Kind::SteelMill | Kind::Factory | Kind::ServerFarm | Kind::SilicaQuarry | Kind::DataCentre => 2.0,
            Kind::Sawmill | Kind::FoodPlant => 3.0,
            Kind::ChipFab
            | Kind::AiCampus
            | Kind::Refinery
            | Kind::Spaceport
            | Kind::RegolithWorks
            | Kind::FusionPlant
            | Kind::NanoFab
            | Kind::ConvergenceArray => 1.0,
            Kind::OilWell => 3.0,
        }
    }

    /// What one batch of work uses up.
    pub fn inputs(self) -> &'static [(Cargo, u32)] {
        match self {
            Kind::PowerStation => &[(Cargo::Coal, 1)],
            Kind::SteelMill => &[(Cargo::IronOre, 1), (Cargo::Coal, 1)],
            Kind::Sawmill => &[(Cargo::Timber, 1)],
            Kind::FoodPlant => &[(Cargo::Grain, 1)],
            Kind::Factory => &[(Cargo::Steel, 1), (Cargo::Planks, 1)],
            Kind::ServerFarm => &[(Cargo::Goods, 1)],
            Kind::ChipFab => &[(Cargo::Silicon, 1)],
            Kind::DataCentre | Kind::AiCampus => &[(Cargo::Chips, 1)],
            Kind::Refinery => &[(Cargo::Oil, 1)],
            Kind::RegolithWorks => &[(Cargo::Regolith, 1)],
            Kind::FusionPlant => &[(Cargo::Helium3, 1)],
            Kind::NanoFab => &[(Cargo::RareMetals, 1), (Cargo::Goods, 1)],
            Kind::ConvergenceArray => &[
                (Cargo::RareMetals, 600),
                (Cargo::NanoGoods, 600),
                (Cargo::Helium3, 300),
                (Cargo::LunarGlass, 600),
            ],
            Kind::Spaceport => &[(Cargo::Fuel, crate::space::FUEL), (Cargo::Goods, crate::space::PAYLOAD)],
            _ => &[],
        }
    }

    /// What one batch makes.
    pub fn outputs(self) -> &'static [(Cargo, u32)] {
        match self {
            Kind::CoalMine => &[(Cargo::Coal, 1)],
            Kind::IronOreMine => &[(Cargo::IronOre, 1)],
            Kind::Forest => &[(Cargo::Timber, 1)],
            Kind::Farm => &[(Cargo::Grain, 1)],
            Kind::SteelMill => &[(Cargo::Steel, 1)],
            Kind::Sawmill => &[(Cargo::Planks, 1)],
            Kind::FoodPlant => &[(Cargo::Food, 1)],
            Kind::Factory => &[(Cargo::Goods, 2)],
            Kind::SilicaQuarry => &[(Cargo::Silicon, 1)],
            Kind::ChipFab => &[(Cargo::Chips, 1)],
            Kind::OilWell => &[(Cargo::Oil, 1)],
            Kind::Refinery => &[(Cargo::Fuel, 1)],
            Kind::RegolithWorks => &[(Cargo::LunarGlass, 1)],
            Kind::NanoFab => &[(Cargo::NanoGoods, 1)],
            Kind::PowerStation
            | Kind::ServerFarm
            | Kind::DataCentre
            | Kind::AiCampus
            | Kind::Spaceport
            | Kind::FusionPlant
            | Kind::ConvergenceArray => &[],
        }
    }

    /// Compute-units one batch makes at the year-2000 efficiency (0 for everything that makes cargo).
    pub fn compute_scale(self) -> f64 {
        match self {
            Kind::ServerFarm => 1.0,
            Kind::DataCentre => 5.0,
            Kind::AiCampus => 20.0,
            _ => 0.0,
        }
    }

    pub fn is_primary(self) -> bool {
        self.inputs().is_empty()
    }

    /// Batches a month at full rating (primaries) or at most (secondaries).
    pub fn batches(self) -> u32 {
        match self {
            Kind::CoalMine => 120,
            Kind::IronOreMine => 100,
            Kind::Forest => 90,
            Kind::Farm => 80,
            Kind::SteelMill => 100,
            Kind::Sawmill => 100,
            Kind::FoodPlant => 90,
            Kind::Factory => 60,
            Kind::PowerStation => 40,
            Kind::ServerFarm | Kind::DataCentre | Kind::AiCampus => 40,
            Kind::SilicaQuarry => 80,
            Kind::ChipFab => 50,
            Kind::OilWell => 70,
            Kind::Refinery => 70,
            Kind::Spaceport => 3,
            Kind::RegolithWorks => 60,
            Kind::FusionPlant => 20,
            Kind::NanoFab => 40,
            Kind::ConvergenceArray => 1,
        }
    }

    /// Megawatts drawn (negative is made): the power station makes 100 while it has coal.
    pub fn power_mw(self) -> f32 {
        match self {
            Kind::PowerStation => -100.0,
            Kind::SteelMill => 20.0,
            Kind::Factory => 10.0,
            Kind::Sawmill | Kind::FoodPlant | Kind::ServerFarm => 5.0,
            Kind::ChipFab => 30.0,
            Kind::DataCentre => 20.0,
            Kind::AiCampus => 60.0,
            Kind::Refinery => 20.0,
            Kind::Spaceport => 50.0,
            Kind::RegolithWorks => 30.0,
            Kind::FusionPlant => -400.0,
            Kind::NanoFab => 40.0,
            Kind::ConvergenceArray => 200.0,
            _ => 0.0,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Industry {
    pub id: usize,
    pub kind: Kind,
    /// North-west tile of the footprint.
    pub tile: (i32, i32),
    /// Waiting inputs (secondaries and power stations).
    pub stock: [u32; cargo::COUNT],
    /// What the grid supplied last month, 0 to 1 (1 for industries that need no power).
    pub power: f32,
    pub made_last_month: u32,
    pub moved_last_month: u32,
    /// How much of its full output a primary industry makes now: it drifts with how well it is served.
    pub production: f32,
    /// Months in a row its best station has rated it under 15% (or nothing has served it).
    pub neglected: u32,
    /// Months left before it shuts, once it has announced it will.
    pub closing: Option<u32>,
    pub closed: bool,
    /// Months a spaceport's pad stays shut after a failed launch.
    pub pad_closed: u32,
}

/// Something that happened to an industry this month.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    /// It has been neglected too long and will shut within the year.
    Closing(usize),
    /// It has shut: its ground is free and a replacement should be founded.
    Closed(usize),
}

impl Industry {
    pub fn size(&self) -> i32 {
        self.kind.size()
    }

    pub fn contains(&self, t: (i32, i32)) -> bool {
        (self.tile.0..self.tile.0 + self.size()).contains(&t.0)
            && (self.tile.1..self.tile.1 + self.size()).contains(&t.1)
    }

    pub fn tiles(&self) -> Vec<(i32, i32)> {
        let s = self.size();
        (0..s * s).map(|i| (self.tile.0 + i % s, self.tile.1 + i / s)).collect()
    }

    pub fn centre(&self) -> (f32, f32) {
        let s = self.size() as f32;
        (self.tile.0 as f32 + s / 2.0, self.tile.1 as f32 + s / 2.0)
    }

    /// Whether any tile in `tiles` is within `radius` tiles (square) of this industry.
    pub fn near(&self, tiles: &[(i32, i32)], radius: i32) -> bool {
        let s = self.size();
        tiles.iter().any(|&(x, z)| {
            x >= self.tile.0 - radius
                && x < self.tile.0 + s + radius
                && z >= self.tile.1 - radius
                && z < self.tile.1 + s + radius
        })
    }

    /// How many units of `cargo` it would take now.
    pub fn room(&self, cargo: Cargo) -> u32 {
        if self.kind == Kind::ConvergenceArray {
            let need = self
                .kind
                .inputs()
                .iter()
                .find(|&&(c, _)| c == cargo)
                .map_or(0, |&(_, n)| n);
            need.saturating_sub(self.stock[cargo.index()])
        } else if self.kind.inputs().iter().any(|&(c, _)| c == cargo) {
            STOCK_CAP.saturating_sub(self.stock[cargo.index()])
        } else {
            0
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Industries {
    pub list: Vec<Industry>,
    by_tile: HashMap<(i32, i32), usize>,
    drift_rng: u64,
    /// After the singularity (§10.5): factories make without waiting for their inputs.
    pub post_singularity: bool,
    /// A storm: spaceports keep their fuel and goods and launch nothing this month.
    pub launches_held: bool,
}

/// A primary industry closes after this many months of poor service, if its output has shrunk below this.
pub const CLOSE_AFTER_MONTHS: u32 = 24;
pub const CLOSE_BELOW: f32 = 0.5;

/// The mark industries leave on `Towns::occupied`.
pub const OCCUPIED: u8 = 3;

impl Industries {
    pub fn at(&self, tile: (i32, i32)) -> Option<usize> {
        self.by_tile.get(&tile).copied()
    }

    /// The Convergence Array's progress: the share of each delivery it still needs that has arrived (0 to 1).
    pub fn convergence_progress(ind: &Industry) -> f32 {
        let inputs = ind.kind.inputs();
        if inputs.is_empty() {
            return 0.0;
        }
        inputs
            .iter()
            .map(|&(c, n)| (ind.stock[c.index()] as f32 / n as f32).min(1.0))
            .sum::<f32>()
            / inputs.len() as f32
    }

    pub fn get(&self, id: usize) -> Option<&Industry> {
        self.list.get(id).filter(|i| !i.closed)
    }

    pub fn add(&mut self, kind: Kind, tile: (i32, i32)) -> usize {
        let id = self.list.len();
        let ind = Industry {
            id,
            kind,
            tile,
            stock: [0; cargo::COUNT],
            power: 1.0,
            made_last_month: 0,
            moved_last_month: 0,
            production: 1.0,
            neglected: 0,
            closing: None,
            closed: false,
            pad_closed: 0,
        };
        for t in ind.tiles() {
            self.by_tile.insert(t, id);
        }
        self.list.push(ind);
        id
    }

    /// A closed industry's ground is free again: returns its tiles.
    pub fn release(&mut self, id: usize) -> Vec<(i32, i32)> {
        let tiles = self.list[id].tiles();
        for t in &tiles {
            if self.by_tile.get(t) == Some(&id) {
                self.by_tile.remove(t);
            }
        }
        tiles
    }

    /// Industries within a station's catchment.
    pub fn near(&self, tiles: &[(i32, i32)]) -> Vec<usize> {
        self.list
            .iter()
            .filter(|i| !i.closed && i.near(tiles, CATCHMENT))
            .map(|i| i.id)
            .collect()
    }

    /// Takes a delivery: adds up to the room there is. Returns how many units were taken.
    pub fn deliver(&mut self, id: usize, cargo: Cargo, units: u32) -> u32 {
        let Some(ind) = self.list.get_mut(id) else { return 0 };
        let taken = units.min(ind.room(cargo));
        ind.stock[cargo.index()] += taken;
        taken
    }

    /// Powered grid: how much of what each industry wants the grid gives it
    /// (power stations are sources, burning coal in stock).
    pub fn grid(&self) -> Vec<f32> {
        let mut sources = Vec::new();
        let mut consumers = Vec::new();
        let mut consumer_ids = Vec::new();
        for ind in self.list.iter().filter(|i| !i.closed) {
            let mw = ind.kind.power_mw();
            if mw < 0.0 {
                // A power station makes power in proportion to the fuel it has to burn (coal, or Helium-3).
                let burns = ind.kind.inputs()[0].0;
                let fuel = (ind.stock[burns.index()] as f32 / ind.kind.batches() as f32).min(1.0);
                sources.push(power::Source {
                    at: ind.centre(),
                    mw: -mw * fuel,
                });
            } else if mw > 0.0 {
                consumers.push(power::Consumer { at: ind.centre(), mw });
                consumer_ids.push(ind.id);
            }
        }
        let supplied = power::solve(&sources, &consumers);
        let mut out = vec![1.0; self.list.len()];
        for (id, s) in consumer_ids.into_iter().zip(supplied) {
            out[id] = s;
        }
        out
    }

    /// Once a month each primary industry has a small chance of a production change: up (3% to 15%)
    /// with 70% odds when its best station rates over 60%, down otherwise (GAME-DESIGN.md §5.2).
    /// `ratings[i]` is the best rating of the stations that serve industry `i`.
    pub fn drift(&mut self, ratings: &[f32]) -> Vec<Event> {
        let mut events = Vec::new();
        if self.drift_rng == 0 {
            self.drift_rng = 0x9E37_79B9_7F4A_7C15;
        }
        for ind in &mut self.list {
            if !ind.kind.is_primary() || ind.closed {
                continue;
            }
            // Neglect: two years of poor service and a shrunken output, and it announces it is closing.
            if ratings.get(ind.id).is_some_and(|&r| r < 0.15) {
                ind.neglected += 1;
            } else {
                ind.neglected = 0;
                ind.closing = None;
            }
            match ind.closing {
                Some(0) | Some(1) => {
                    ind.closing = None;
                    ind.closed = true;
                    ind.made_last_month = 0;
                    events.push(Event::Closed(ind.id));
                    continue;
                }
                Some(n) => ind.closing = Some(n - 1),
                None if ind.neglected >= CLOSE_AFTER_MONTHS && ind.production < CLOSE_BELOW => {
                    ind.closing = Some(12);
                    events.push(Event::Closing(ind.id));
                }
                None => {}
            }
            let mut next = || {
                self.drift_rng ^= self.drift_rng << 13;
                self.drift_rng ^= self.drift_rng >> 7;
                self.drift_rng ^= self.drift_rng << 17;
                (self.drift_rng >> 11) as f32 / (1u64 << 53) as f32
            };
            if next() > 0.12 {
                continue;
            }
            let well_served = ratings.get(ind.id).is_some_and(|&r| r > 0.6);
            let up = next() < if well_served { 0.7 } else { 0.3 };
            let step = 0.03 + 0.12 * next();
            ind.production = if up {
                ind.production * (1.0 + step)
            } else {
                ind.production * (1.0 - step)
            }
            .clamp(0.1, 3.0);
        }
        events
    }

    /// One economy month. Power stations burn coal; secondaries turn stock into
    /// output; primaries make their cargo. Returns what was made, to hand to the stations
    /// that serve each industry.
    pub fn month(&mut self) -> Vec<(usize, Cargo, u32)> {
        let grid = self.grid();
        let mut made = Vec::new();
        for ind in self.list.iter_mut().filter(|i| !i.closed) {
            ind.power = grid[ind.id];
            let efficiency = if ind.kind.power_mw() > 0.0 {
                power::efficiency(ind.power)
            } else {
                1.0
            };
            ind.made_last_month = 0;
            let kind = ind.kind;
            if kind.power_mw() < 0.0 {
                let fuel = kind.inputs()[0].0.index();
                let burn = ind.stock[fuel].min(kind.batches());
                ind.stock[fuel] -= burn;
                ind.made_last_month = burn;
                continue;
            }
            // The Array is a building site: what's delivered stays there.
            if kind == Kind::ConvergenceArray {
                continue;
            }
            if kind == Kind::Spaceport && self.launches_held {
                continue;
            }
            if kind == Kind::Spaceport && ind.pad_closed > 0 {
                ind.pad_closed -= 1;
                continue;
            }
            let batches = if kind.is_primary() {
                (kind.batches() as f32 * ind.production).round() as u32
            } else {
                // As many batches as every input allows, at most what the machinery can do.
                let by_stock = kind
                    .inputs()
                    .iter()
                    .map(|&(c, n)| ind.stock[c.index()] / n)
                    .min()
                    .unwrap_or(0);
                let cap = (kind.batches() as f32 * efficiency).round() as u32;
                if self.post_singularity && !kind.outputs().is_empty() {
                    // Synthesis: everything the machinery can make, from nothing.
                    for &(c, per) in kind.outputs() {
                        made.push((ind.id, c, cap * per));
                        ind.made_last_month += cap * per;
                    }
                    continue;
                }
                for &(c, n) in kind.inputs() {
                    ind.stock[c.index()] -= by_stock.min(cap) * n;
                }
                by_stock.min(cap)
            };
            if kind.compute_scale() > 0.0 || kind == Kind::Spaceport {
                ind.made_last_month = batches;
            }
            for &(c, per) in kind.outputs() {
                let units = batches * per;
                if units > 0 {
                    made.push((ind.id, c, units));
                    ind.made_last_month += units;
                }
            }
        }
        made
    }
}

// --- Placement --------------------------------------------------------------------------

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n.max(1)
    }
}

fn hash2(seed: u32, x: i32, z: i32) -> f32 {
    let mut h =
        (x as u32).wrapping_mul(0x85EB_CA6B) ^ (z as u32).wrapping_mul(0xC2B2_AE35) ^ seed.wrapping_mul(0x27D4_EB2F);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    h = h.wrapping_mul(0x297A_2D39);
    h ^= h >> 15;
    (h & 0xFFFF) as f32 / 65535.0
}

/// Smooth noise at a scale of `cell` tiles: where a resource is likely.
fn field(seed: u32, x: i32, z: i32, cell: i32) -> f32 {
    let (fx, fz) = (x as f32 / cell as f32, z as f32 / cell as f32);
    let (x0, z0) = (fx.floor() as i32, fz.floor() as i32);
    let (tx, tz) = (fx - x0 as f32, fz - z0 as f32);
    let s = |t: f32| t * t * (3.0 - 2.0 * t);
    let (tx, tz) = (s(tx), s(tz));
    let a = hash2(seed, x0, z0);
    let b = hash2(seed, x0 + 1, z0);
    let c = hash2(seed, x0, z0 + 1);
    let d = hash2(seed, x0 + 1, z0 + 1);
    (a + (b - a) * tx) + ((c + (d - c) * tx) - (a + (b - a) * tx)) * tz
}

/// What the placement code knows about the map.
struct Ctx<'a> {
    world: &'a World,
    towns: &'a Towns,
    roads: &'a Layer,
    rails: &'a Layer,
    seed: u32,
    /// Land heights (blocks) at the 50th and 75th percentile.
    median: u16,
    high: u16,
}

impl Ctx<'_> {
    fn town_distance(&self, p: (f32, f32)) -> f32 {
        self.towns
            .list
            .iter()
            .map(|t| ((t.centre.0 as f32 - p.0).powi(2) + (t.centre.1 as f32 - p.1).powi(2)).sqrt())
            .fold(f32::MAX, f32::min)
    }

    fn near_water(&self, p: (i32, i32), r: i32) -> bool {
        (-r..=r).step_by(2).any(|dz| {
            (-r..=r).step_by(2).any(|dx| {
                self.world
                    .surface(p.0 + dx, p.1 + dz)
                    .is_some_and(|(b, _)| b < self.world.sea_level)
            })
        })
    }

    fn material(&self, t: (i32, i32)) -> Option<Material> {
        self.world.top_block(t.0, t.1).map(|(_, b)| b.material)
    }

    /// Whether `kind` suits a site with this north-west tile, and how well (higher is better).
    fn score(&self, kind: Kind, tile: (i32, i32), placed: &Industries) -> Option<f32> {
        let s = kind.size();
        let c = (tile.0 as f32 + s as f32 / 2.0, tile.1 as f32 + s as f32 / 2.0);
        let ci = (tile.0 + s / 2, tile.1 + s / 2);
        let (base, _) = self.world.surface(ci.0, ci.1)?;
        if base <= self.world.sea_level {
            return None;
        }
        let ground = self.material(ci)?;
        let town = self.town_distance(c);
        let nearest_of = |k: Kind| {
            placed
                .list
                .iter()
                .filter(|i| i.kind == k)
                .map(|i| ((i.centre().0 - c.0).powi(2) + (i.centre().1 - c.1).powi(2)).sqrt())
                .fold(f32::MAX, f32::min)
        };
        // Spacing from others of the same kind.
        let spacing = match kind {
            Kind::CoalMine => 40.0,
            Kind::IronOreMine => 60.0,
            Kind::Forest => 20.0,
            Kind::Farm => 20.0,
            _ => 30.0,
        };
        if nearest_of(kind) < spacing || town < 6.0 {
            return None;
        }
        let rule = match kind {
            Kind::CoalMine => {
                (base >= self.median && ground != Material::Sand).then(|| field(self.seed, ci.0, ci.1, 48))
            }
            Kind::IronOreMine => {
                (base >= self.high && ground != Material::Sand).then(|| field(self.seed ^ 77, ci.0, ci.1, 48))
            }
            Kind::Forest => {
                (ground == Material::Grass && base < self.high).then(|| field(self.seed ^ 31, ci.0, ci.1, 32))
            }
            Kind::Farm => (ground == Material::Grass && town < 60.0).then(|| 1.0 - town / 60.0),
            Kind::PowerStation => (self.near_water(ci, 10) || town < 25.0).then_some(1.0),
            Kind::SteelMill => (town >= 20.0)
                .then(|| 1.0 / (1.0 + nearest_of(Kind::CoalMine).min(nearest_of(Kind::IronOreMine)) / 60.0)),
            Kind::Sawmill => {
                let f = nearest_of(Kind::Forest);
                (ground == Material::Grass && (40.0..=120.0).contains(&f)).then_some(1.0)
            }
            Kind::FoodPlant => (town < 15.0).then_some(1.0),
            Kind::Factory => (town < 20.0).then_some(1.0),
            Kind::ServerFarm => (6.0..25.0).contains(&town).then(|| 1.0 - town / 25.0),
            Kind::SilicaQuarry => ((ground == Material::Sand || base >= self.median) && ground != Material::Grass)
                .then(|| field(self.seed ^ 19, ci.0, ci.1, 40) + 0.2),
            Kind::ChipFab => (town >= 8.0 && self.near_water(ci, 20)).then(|| 1.0 / (1.0 + town / 40.0)),
            Kind::DataCentre => (6.0..35.0).contains(&town).then(|| 1.0 - town / 35.0),
            Kind::AiCampus => (15.0..60.0).contains(&town).then(|| 1.0 / (1.0 + town / 60.0)),
            Kind::OilWell => self
                .near_water(ci, 14)
                .then(|| field(self.seed ^ 55, ci.0, ci.1, 40) + 0.2),
            Kind::Refinery => (town >= 12.0).then(|| 1.0 / (1.0 + nearest_of(Kind::OilWell).min(200.0) / 60.0)),
            Kind::Spaceport => (town >= 30.0 && self.near_water(ci, 10)).then(|| 1.0 / (1.0 + town / 80.0)),
            Kind::RegolithWorks | Kind::NanoFab => (10.0..45.0).contains(&town).then(|| 1.0 - town / 45.0),
            Kind::FusionPlant => (12.0..40.0).contains(&town).then_some(1.0),
            Kind::ConvergenceArray => (20.0..70.0).contains(&town).then(|| 1.0 / (1.0 + town / 70.0)),
        }?;
        Some(rule)
    }

    /// A site's ground levelled to one height, if that doesn't move too much and doesn't
    /// disturb anything built.
    fn level_site(&self, kind: Kind, tile: (i32, i32)) -> Option<Vec<((i32, i32), u16)>> {
        let s = kind.size();
        let mut corners = Corners::with_limits(self.world, 3, 90);
        let (cx, cz) = (tile.0 + s / 2, tile.1 + s / 2);
        let h = corners.tile(cx, cz)?.iter().copied().min().unwrap();
        for dz in 0..s {
            for dx in 0..s {
                let t = (tile.0 + dx, tile.1 + dz);
                if !self.world.in_bounds(t.0, t.1)
                    || self.roads.get(t.0, t.1) != 0
                    || self.rails.get(t.0, t.1) != 0
                    || self.towns.occupied.get(t.0, t.1) != 0
                    || self
                        .world
                        .surface(t.0, t.1)
                        .is_none_or(|(b, _)| b <= self.world.sea_level)
                {
                    return None;
                }
                if !corners.level(t.0, t.1, h) {
                    return None;
                }
            }
        }
        if corners.steps() > 14 {
            return None;
        }
        // Nothing nearby may be moved by it (houses, roads, other industry).
        for (x, z) in corners.affected_tiles() {
            let inside = (tile.0..tile.0 + s).contains(&x) && (tile.1..tile.1 + s).contains(&z);
            let built = self.roads.get(x, z) != 0 || self.rails.get(x, z) != 0 || self.towns.occupied.get(x, z) != 0;
            if !inside && built && corners.surface(x, z) != self.world.surface(x, z) {
                return None;
            }
        }
        Some(corners.changes())
    }
}

impl Industries {
    /// Places industries on a fresh map after the towns: primaries where their resource
    /// fields and terrain suit, secondaries near towns and suppliers. Sites are levelled flat
    /// (the world is changed here). Returns the industries and the tiles that moved.
    pub fn generate(
        world: &mut World,
        towns: &mut Towns,
        roads: &Layer,
        rails: &Layer,
        seed: u32,
        year: i32,
    ) -> (Industries, Vec<(i32, i32)>) {
        let mut out = Industries::default();
        let moved = out.found(world, towns, roads, rails, seed, year);
        (out, moved)
    }

    /// Founds the industries `year` allows that the map doesn't have yet (all of them on a fresh
    /// map; the cloud and the AI campuses as their years arrive). Returns the tiles that moved.
    pub fn found(
        &mut self,
        world: &mut World,
        towns: &mut Towns,
        roads: &Layer,
        rails: &Layer,
        seed: u32,
        year: i32,
    ) -> Vec<(i32, i32)> {
        let size = world.size_x as i32;
        // Counts for the default 512 x 512 map, scaled by area.
        let scale = (size * size) as f32 / (512.0 * 512.0);
        let want = |n: f32| ((n * scale).round() as usize).max(1);
        let plan: Vec<(Kind, usize)> = Kind::ALL
            .iter()
            .filter(|k| k.available(year) && !self.list.iter().any(|i| i.kind == **k && !i.closed))
            .map(|&k| (k, want(k.count())))
            .collect();
        self.found_plan(world, towns, roads, rails, seed, plan)
    }

    /// Founds `count` of each kind in `plan`, wherever suits best.
    pub fn found_plan(
        &mut self,
        world: &mut World,
        towns: &mut Towns,
        roads: &Layer,
        rails: &Layer,
        seed: u32,
        plan: Vec<(Kind, usize)>,
    ) -> Vec<(i32, i32)> {
        let mut moved = Vec::new();
        let size = world.size_x as i32;
        if plan.is_empty() {
            return moved;
        }
        let mut rng = Rng(((seed as u64) ^ (self.list.len() as u64) << 32).wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1);
        // Land heights: the 50th and 75th percentiles.
        let mut heights: Vec<u16> = (0..size)
            .step_by(4)
            .flat_map(|z| (0..size).step_by(4).map(move |x| (x, z)))
            .filter_map(|(x, z)| world.surface(x, z).map(|(b, _)| b))
            .filter(|&b| b > world.sea_level)
            .collect();
        heights.sort_unstable();
        let at = |q: f32| {
            heights
                .get(((heights.len() as f32 * q) as usize).min(heights.len().saturating_sub(1)))
                .copied()
                .unwrap_or(0)
        };
        let (median, high) = (at(0.5), at(0.75));

        for (kind, count) in plan {
            for _ in 0..count {
                // Sample places, rank by how well they suit, and take the first that can be levelled.
                let mut candidates = Vec::new();
                {
                    let ctx = Ctx {
                        world,
                        towns,
                        roads,
                        rails,
                        seed,
                        median,
                        high,
                    };
                    for _ in 0..1500 {
                        let t = (
                            4 + rng.below((size - 12) as u64) as i32,
                            4 + rng.below((size - 12) as u64) as i32,
                        );
                        if let Some(score) = ctx.score(kind, t, self) {
                            candidates.push((score, t));
                        }
                    }
                    candidates.sort_by(|a, b| b.0.total_cmp(&a.0));
                }
                for &(_, tile) in candidates.iter().take(40) {
                    let changes = Ctx {
                        world,
                        towns,
                        roads,
                        rails,
                        seed,
                        median,
                        high,
                    }
                    .level_site(kind, tile);
                    let Some(changes) = changes else { continue };
                    moved.extend(edit::apply(world, &changes));
                    let id = self.add(kind, tile);
                    for t in self.list[id].tiles() {
                        towns.occupied.set(t.0, t.1, OCCUPIED);
                    }
                    break;
                }
            }
        }
        moved.sort();
        moved.dedup();
        moved
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terrain::generation::{self, GenParams};

    fn map(seed: u32) -> (World, Towns, Layer, Layer) {
        let params = GenParams {
            seed,
            ..GenParams::default()
        };
        let world = generation::generate(&params);
        let mut roads = Layer::new(world.size_x, world.size_z);
        let rails = Layer::new(world.size_x, world.size_z);
        let towns = Towns::generate(&world, &mut roads, &rails, seed, 2000);
        (world, towns, roads, rails)
    }

    #[test]
    fn a_map_gets_a_working_set_of_industries() {
        for seed in [1u32, 7, 42, 2024, 99] {
            let (mut world, mut towns, roads, rails) = map(seed);
            let (ind, moved) = Industries::generate(&mut world, &mut towns, &roads, &rails, seed, 2000);
            let count = |k: Kind| ind.list.iter().filter(|i| i.kind == k).count();
            eprintln!(
                "seed {seed}: coal {} ore {} forest {} farm {} power {} steel {} sawmill {} food {} factory {}",
                count(Kind::CoalMine),
                count(Kind::IronOreMine),
                count(Kind::Forest),
                count(Kind::Farm),
                count(Kind::PowerStation),
                count(Kind::SteelMill),
                count(Kind::Sawmill),
                count(Kind::FoodPlant),
                count(Kind::Factory)
            );
            assert!(ind.list.len() >= 20, "seed {seed}: only {} industries", ind.list.len());
            // A coal mine to haul from and somewhere to burn it.
            assert!(
                count(Kind::CoalMine) >= 2 && count(Kind::PowerStation) >= 1,
                "seed {seed}"
            );
            assert!(!moved.is_empty() || ind.list.is_empty());
            // Every site is flat, dry, on its own ground, and marked.
            for i in &ind.list {
                let level = world.surface(i.tile.0, i.tile.1).unwrap().0;
                for t in i.tiles() {
                    let (base, shape) = world.surface(t.0, t.1).unwrap();
                    assert!(
                        shape.0 == 0 && base == level && base > world.sea_level,
                        "seed {seed}: {} not flat at {t:?}",
                        i.kind.name()
                    );
                    assert_eq!(towns.occupied.get(t.0, t.1), OCCUPIED);
                    assert_eq!(roads.get(t.0, t.1), 0);
                    assert_eq!(ind.at(t), Some(i.id));
                }
            }
            // Spacing between mines.
            for a in ind.list.iter().filter(|i| i.kind == Kind::CoalMine) {
                for b in ind.list.iter().filter(|i| i.kind == Kind::CoalMine && i.id > a.id) {
                    let d = ((a.centre().0 - b.centre().0).powi(2) + (a.centre().1 - b.centre().1).powi(2)).sqrt();
                    assert!(d >= 40.0, "coal mines {d} tiles apart");
                }
            }
        }
    }

    #[test]
    fn primaries_make_cargo_and_secondaries_turn_stock_into_dearer_cargo() {
        let mut ind = Industries::default();
        let mine = ind.add(Kind::CoalMine, (0, 0));
        let mill = ind.add(Kind::SteelMill, (50, 50));
        let made = ind.month();
        assert!(made.contains(&(mine, Cargo::Coal, 120)));
        assert!(!made.iter().any(|m| m.0 == mill), "an empty mill makes nothing");
        // Two inputs: the scarcer one limits output (1 ore + 1 coal -> 1 steel).
        ind.deliver(mill, Cargo::IronOre, 50);
        ind.deliver(mill, Cargo::Coal, 30);
        let made = ind.month();
        assert!(made.contains(&(mill, Cargo::Steel, 30)), "{made:?}");
        assert_eq!(ind.list[mill].stock[Cargo::IronOre.index()], 20);
        assert_eq!(ind.list[mill].stock[Cargo::Coal.index()], 0);
        // The factory is the one industry that multiplies.
        let factory = ind.add(Kind::Factory, (0, 90));
        ind.deliver(factory, Cargo::Steel, 10);
        ind.deliver(factory, Cargo::Planks, 10);
        let made = ind.month();
        assert!(made.contains(&(factory, Cargo::Goods, 20)), "{made:?}");
    }

    #[test]
    fn deliveries_are_refused_when_the_stock_is_full_or_not_wanted() {
        let mut ind = Industries::default();
        let sawmill = ind.add(Kind::Sawmill, (0, 0));
        assert_eq!(ind.deliver(sawmill, Cargo::Coal, 10), 0, "a sawmill doesn't want coal");
        assert_eq!(ind.deliver(sawmill, Cargo::Timber, 1_000), STOCK_CAP);
        assert_eq!(ind.list[sawmill].room(Cargo::Timber), 0);
        assert_eq!(ind.deliver(sawmill, Cargo::Timber, 5), 0);
    }

    #[test]
    fn power_stations_burn_coal_to_run_the_mills_nearby() {
        let mut ind = Industries::default();
        let power = ind.add(Kind::PowerStation, (0, 0));
        let mill = ind.add(Kind::SteelMill, (10, 0));
        let far = ind.add(Kind::Sawmill, (200, 0));
        // No coal: no power anywhere.
        let grid = ind.grid();
        assert_eq!(grid[mill], 0.0);
        ind.deliver(power, Cargo::Coal, 100);
        let grid = ind.grid();
        assert_eq!(grid[mill], 1.0, "the mill is in reach");
        assert_eq!(grid[far], 0.0, "the sawmill is out of reach and runs on its generator");
        ind.month();
        assert_eq!(ind.list[power].stock[Cargo::Coal.index()], 60, "40 burnt");
        // A mill with only the generator still works, slowly.
        ind.deliver(far, Cargo::Timber, 300);
        let made = ind.month();
        let planks = made.iter().find(|m| m.0 == far).unwrap().2;
        assert!(planks > 0 && planks < Kind::Sawmill.batches(), "{planks}");
    }

    #[test]
    fn catchment_covers_the_footprint_plus_three_tiles() {
        let mut ind = Industries::default();
        let id = ind.add(Kind::CoalMine, (10, 10));
        assert_eq!(ind.near(&[(15, 11)]), vec![id], "two tiles off the east edge");
        assert!(ind.near(&[(17, 11)]).is_empty(), "four tiles off");
        assert_eq!(ind.near(&[(9, 9)]), vec![id]);
    }

    #[test]
    fn a_server_farm_turns_goods_into_batches_of_compute_and_a_short_grid_slows_it() {
        let mut ind = Industries::default();
        let farm = ind.add(Kind::ServerFarm, (0, 0));
        ind.deliver(farm, Cargo::Goods, 100);
        // No power station within reach: the farm runs on its own generator at 30%.
        ind.month();
        let slow = ind.list[farm].made_last_month;
        assert_eq!(
            slow,
            (Kind::ServerFarm.batches() as f32 * power::GENERATOR_FLOOR).round() as u32
        );
        assert_eq!(ind.list[farm].stock[Cargo::Goods.index()], 100 - slow);
        // With a stocked power station beside it, the full forty batches.
        let plant = ind.add(Kind::PowerStation, (6, 0));
        ind.deliver(plant, Cargo::Coal, 100);
        ind.month();
        assert_eq!(ind.list[farm].made_last_month, Kind::ServerFarm.batches());
        // Without goods it makes nothing.
        ind.list[farm].stock[Cargo::Goods.index()] = 0;
        ind.month();
        assert_eq!(ind.list[farm].made_last_month, 0);
    }

    #[test]
    fn the_chip_chain_is_placed_and_the_cloud_arrives_with_its_years() {
        let (mut with_chain, mut with_dc, mut with_ai) = (0, 0, 0);
        let seeds = [1u32, 7, 42, 2024, 99];
        for seed in seeds {
            let (mut world, mut towns, roads, rails) = map(seed);
            let (mut ind, _) = Industries::generate(&mut world, &mut towns, &roads, &rails, seed, 2000);
            let has = |ind: &Industries, k: Kind| ind.list.iter().any(|i| i.kind == k);
            assert!(has(&ind, Kind::ServerFarm), "seed {seed}: no server farm");
            assert!(!has(&ind, Kind::DataCentre) && !has(&ind, Kind::AiCampus), "too early");
            with_chain += (has(&ind, Kind::SilicaQuarry) && has(&ind, Kind::ChipFab)) as i32;
            ind.found(&mut world, &mut towns, &roads, &rails, seed, 2005);
            with_dc += has(&ind, Kind::DataCentre) as i32;
            ind.found(&mut world, &mut towns, &roads, &rails, seed, 2020);
            with_ai += has(&ind, Kind::AiCampus) as i32;
            // Founding again changes nothing.
            let n = ind.list.len();
            ind.found(&mut world, &mut towns, &roads, &rails, seed, 2020);
            assert_eq!(ind.list.len(), n);
        }
        eprintln!("chip chain {with_chain}/5, data centres {with_dc}/5, AI campuses {with_ai}/5");
        assert!(with_chain >= 4 && with_dc >= 4 && with_ai >= 4);
    }

    #[test]
    fn well_served_industries_grow_and_neglected_ones_shrink() {
        let (mut served, mut neglected) = (0.0f32, 0.0f32);
        for trial in 0..20u64 {
            let mut ind = Industries::default();
            ind.drift_rng = 0x1234_5678 + trial * 7919;
            ind.add(Kind::CoalMine, (0, 0));
            ind.add(Kind::SteelMill, (20, 0));
            for _ in 0..120 {
                ind.drift(&[0.9, 0.9]);
            }
            served += ind.list[0].production;
            assert_eq!(ind.list[1].production, 1.0, "secondaries follow their inputs instead");
            let mut ind = Industries::default();
            ind.drift_rng = 0x8765_4321 + trial * 7919;
            ind.add(Kind::CoalMine, (0, 0));
            for _ in 0..120 {
                ind.drift(&[0.1]);
            }
            neglected += ind.list[0].production;
        }
        assert!(served / 20.0 > 1.5, "served {}", served / 20.0);
        assert!(neglected / 20.0 < 0.8, "neglected {}", neglected / 20.0);
    }

    #[test]
    fn a_neglected_shrunken_mine_announces_then_closes_and_frees_its_ground() {
        let mut ind = Industries::default();
        let a = ind.add(Kind::CoalMine, (0, 0));
        let b = ind.add(Kind::CoalMine, (50, 0));
        ind.list[a].production = 0.3;
        let (mut closing, mut closed) = (None, None);
        for month in 0..60 {
            for e in ind.drift(&[0.0, 0.9]) {
                match e {
                    Event::Closing(id) => closing = closing.or(Some((month, id))),
                    Event::Closed(id) => closed = closed.or(Some((month, id))),
                }
            }
        }
        let (m1, id1) = closing.expect("announced");
        let (m2, id2) = closed.expect("closed");
        assert_eq!((id1, id2), (a, a));
        assert!(m1 >= CLOSE_AFTER_MONTHS as usize - 1 && m2 - m1 >= 11, "{m1} {m2}");
        assert!(!ind.list[b].closed, "a served mine never closes");
        assert!(ind.get(a).is_none() && ind.near(&[(1, 1)]).is_empty());
        ind.release(a);
        assert_eq!(ind.at((1, 1)), None);
        // Starting to be served again cancels the closure.
        let c = ind.add(Kind::Forest, (90, 0));
        ind.list[c].production = 0.2;
        for _ in 0..30 {
            ind.drift(&[0.0, 0.9, 0.0, 0.0][..c.min(3) + 1]);
        }
        assert!(ind.list[c].closing.is_some() || ind.list[c].closed);
    }

    #[test]
    fn oil_becomes_fuel_and_fuel_with_goods_becomes_launches() {
        let mut ind = Industries::default();
        let plant = ind.add(Kind::PowerStation, (0, 0));
        ind.deliver(plant, Cargo::Coal, 400);
        let well = ind.add(Kind::OilWell, (10, 0));
        let refinery = ind.add(Kind::Refinery, (14, 0));
        let port = ind.add(Kind::Spaceport, (20, 0));
        let made = ind.month();
        assert!(made.iter().any(|&(i, c, n)| i == well && c == Cargo::Oil && n == 70));
        ind.deliver(refinery, Cargo::Oil, 100);
        let made = ind.month();
        assert!(
            made.iter()
                .any(|&(i, c, n)| i == refinery && c == Cargo::Fuel && n == 70),
            "{made:?}"
        );
        ind.deliver(port, Cargo::Fuel, 400);
        ind.deliver(port, Cargo::Goods, 400);
        ind.month();
        assert_eq!(ind.list[port].made_last_month, 2, "goods for two launches");
        assert_eq!(ind.list[port].stock[Cargo::Goods.index()], 100);
        // A shut pad launches nothing for the month.
        ind.deliver(port, Cargo::Goods, 300);
        ind.list[port].pad_closed = 1;
        ind.month();
        assert_eq!(ind.list[port].made_last_month, 0);
        ind.month();
        assert!(ind.list[port].made_last_month > 0);
    }

    #[test]
    fn spaceports_and_oil_are_founded_in_their_years() {
        let mut found = 0;
        let mut oil = 0;
        for seed in [1u32, 7, 42, 2024, 99] {
            let (mut world, mut towns, roads, rails) = map(seed);
            let (mut ind, _) = Industries::generate(&mut world, &mut towns, &roads, &rails, seed, 2000);
            oil += ind.list.iter().any(|i| i.kind == Kind::OilWell) as i32;
            assert!(!ind.list.iter().any(|i| i.kind == Kind::Spaceport));
            ind.found(&mut world, &mut towns, &roads, &rails, seed, 2027);
            found += ind.list.iter().any(|i| i.kind == Kind::Spaceport) as i32;
        }
        assert!(oil >= 4, "oil wells on {oil}/5 maps");
        assert!(found >= 3, "spaceports on {found}/5 maps");
    }

    #[test]
    fn helium_3_runs_a_fusion_plant_that_powers_a_nano_fab_and_regolith_becomes_glass() {
        let mut ind = Industries::default();
        let fusion = ind.add(Kind::FusionPlant, (0, 0));
        let fab = ind.add(Kind::NanoFab, (8, 0));
        let works = ind.add(Kind::RegolithWorks, (16, 0));
        ind.deliver(fab, Cargo::RareMetals, 100);
        ind.deliver(fab, Cargo::Goods, 100);
        ind.deliver(works, Cargo::Regolith, 100);
        // No Helium-3: the fab and the works run on their own generators.
        ind.month();
        assert!(ind.list[fab].power < 0.01);
        ind.deliver(fusion, Cargo::Helium3, 40);
        let made = ind.month();
        assert!(ind.list[fab].power > 0.99, "400 MW is plenty");
        assert!(made.iter().any(|&(i, c, _)| i == fab && c == Cargo::NanoGoods));
        assert!(made.iter().any(|&(i, c, _)| i == works && c == Cargo::LunarGlass));
        assert!(ind.list[fusion].stock[Cargo::Helium3.index()] < 40, "it burns Helium-3");
    }

    #[test]
    fn the_convergence_array_keeps_what_it_is_given_until_it_has_enough() {
        let mut ind = Industries::default();
        let array = ind.add(Kind::ConvergenceArray, (0, 0));
        assert_eq!(ind.list[array].room(Cargo::Helium3), 300);
        assert_eq!(ind.deliver(array, Cargo::Helium3, 500), 300, "only what it still needs");
        ind.deliver(array, Cargo::RareMetals, 200);
        ind.month();
        assert_eq!(
            ind.list[array].stock[Cargo::Helium3.index()],
            300,
            "a building site, not a factory"
        );
        let p = Industries::convergence_progress(&ind.list[array]);
        assert!((p - (1.0 + 200.0 / 600.0) / 4.0).abs() < 1e-4, "{p}");
        for (c, n) in [
            (Cargo::RareMetals, 400),
            (Cargo::NanoGoods, 600),
            (Cargo::LunarGlass, 600),
        ] {
            ind.deliver(array, c, n);
        }
        assert_eq!(Industries::convergence_progress(&ind.list[array]), 1.0);
    }

    #[test]
    fn after_the_singularity_factories_make_from_nothing() {
        let mut ind = Industries::default();
        let mill = ind.add(Kind::Sawmill, (0, 0));
        let farm = ind.add(Kind::ServerFarm, (10, 0));
        ind.deliver(farm, Cargo::Goods, 100);
        assert!(ind.month().iter().all(|&(i, _, _)| i != mill), "no timber, no planks");
        ind.post_singularity = true;
        let made = ind.month();
        assert!(made.iter().any(|&(i, c, n)| i == mill && c == Cargo::Planks && n > 0));
        assert!(ind.list[farm].made_last_month > 0, "compute sites still work as before");
    }
}
