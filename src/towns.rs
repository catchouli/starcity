//! Towns: named settlements of roads and buildings that grow outwards.
//!
//! A town starts as a crossroads at a flat spot. Each growth step it extends
//! a road, puts up a house beside a road, or adds a floor to a building near
//! its centre. It uses the same road planner as the player, so towns join up
//! with player roads and, as they grow, with each other. Passengers delivered
//! to a town's stations make it grow faster.

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
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Building {
    pub town: TownId,
    pub floors: u8,
    /// Picks the colours.
    pub style: u8,
}

impl Building {
    pub fn population(self) -> u32 {
        self.floors as u32 * 8
    }
}

pub struct Towns {
    pub list: Vec<Town>,
    pub buildings: BTreeMap<(i32, i32), Building>,
    /// 1 on tiles with a building, so roads and track avoid them.
    pub occupied: Layer,
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
            rng: 1,
        }
    }

    /// Places towns on flat land and grows each a little. Builds their roads
    /// into `roads`.
    pub fn generate(world: &World, roads_layer: &mut Layer, rails: &Layer, seed: u32) -> Towns {
        let mut towns = Towns::empty(world.size_x, world.size_z);
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
        candidates.sort_by(|a, b| b.0.cmp(&a.0));
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
            for _ in 0..steps {
                towns.grow(i, world, roads_layer, rails);
            }
        }
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
        (4.0 + (self.population(town) as f32).sqrt() * 0.6).min(40.0) as i32
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

    /// Runs growth over `dt` seconds. Returns (road tiles changed, whether buildings changed).
    pub fn update(
        &mut self,
        dt: f32,
        world: &World,
        roads_layer: &mut Layer,
        rails: &Layer,
    ) -> (Vec<(i32, i32)>, bool) {
        let mut road_tiles = Vec::new();
        let mut built = false;
        for i in 0..self.list.len() {
            let town = &mut self.list[i];
            town.served *= 0.995f32.powf(dt);
            // Steady growth, faster with good passenger service.
            town.growth += dt * (0.05 + town.served * 0.01);
            while self.list[i].growth >= 1.0 {
                self.list[i].growth -= 1.0;
                let (roads_built, b) = self.grow(i, world, roads_layer, rails);
                road_tiles.extend(roads_built);
                built |= b;
            }
        }
        (road_tiles, built)
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
                        },
                    );
                    self.occupied.set(t.0, t.1, 1);
                    return (Vec::new(), true);
                }
            } else {
                // Taller buildings towards the centre as the town grows.
                let max_floors = (1 + self.population(id) / 300).min(8) as u8;
                let inner = radius / 2 + 1;
                let pick: Vec<(i32, i32)> = self
                    .buildings
                    .iter()
                    .filter(|(t, b)| b.town == id && b.floors < max_floors && near(**t, inner))
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

    /// Removes buildings in an inclusive rectangle. Returns whether any were removed.
    pub fn demolish(&mut self, a: (i32, i32), b: (i32, i32)) -> bool {
        let before = self.buildings.len();
        let (x0, x1) = (a.0.min(b.0), a.0.max(b.0));
        let (z0, z1) = (a.1.min(b.1), a.1.max(b.1));
        let occupied = &mut self.occupied;
        self.buildings.retain(|&(x, z), _| {
            let inside = (x0..=x1).contains(&x) && (z0..=z1).contains(&z);
            if inside {
                occupied.set(x, z, 0);
            }
            !inside
        });
        self.buildings.len() != before
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
        let mut towns = Towns::generate(&world, &mut roads_layer, &rails, 7);
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
            towns.update(1.0, &world, &mut roads_layer, &rails);
        }
        let after: u32 = towns.list.iter().map(|t| towns.population(t.id)).sum();
        assert!(after > before, "towns grow: {before} -> {after}");
    }
}
