//! The company headquarters: a building on a 4 x 4 plot in the starting
//! town, there from the first frame as a garage and upgraded in levels
//! (GAME-DESIGN.md §9.1).

use crate::network::Layer;
use crate::terrain::World;
use crate::towns::{TownId, Towns};

/// Side of the plot, in tiles. Every level's model fits inside it.
pub const PLOT: i32 = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hq {
    /// North-west tile of the plot.
    pub anchor: (i32, i32),
    pub town: TownId,
}

impl Hq {
    pub fn contains(&self, tile: (i32, i32)) -> bool {
        (self.anchor.0..self.anchor.0 + PLOT).contains(&tile.0)
            && (self.anchor.1..self.anchor.1 + PLOT).contains(&tile.1)
    }

    pub fn tiles(&self) -> Vec<(i32, i32)> {
        let mut out = Vec::with_capacity((PLOT * PLOT) as usize);
        for dz in 0..PLOT {
            for dx in 0..PLOT {
                out.push((self.anchor.0 + dx, self.anchor.1 + dz));
            }
        }
        out
    }

    /// Middle of the plot in world coordinates.
    pub fn centre(&self) -> (f32, f32) {
        (
            self.anchor.0 as f32 + PLOT as f32 / 2.0,
            self.anchor.1 as f32 + PLOT as f32 / 2.0,
        )
    }
}

/// The company town: a modest one (about 700 people) with another town a
/// walk away (40 to 130 tiles), so a first route exists.
pub fn choose_town(towns: &Towns) -> Option<TownId> {
    let dist = |a: (i32, i32), b: (i32, i32)| (((a.0 - b.0).pow(2) + (a.1 - b.1).pow(2)) as f32).sqrt();
    towns
        .list
        .iter()
        .map(|t| {
            let neighbour = towns
                .list
                .iter()
                .filter(|o| o.id != t.id)
                .any(|o| (40.0..=130.0).contains(&dist(t.centre, o.centre)));
            let pop = towns.population(t.id) as i32;
            // Lower is better: a town far from 700 people, or with no neighbour, ranks behind.
            let score = (pop - 700).abs() + if neighbour { 0 } else { 100_000 };
            (score, t.id)
        })
        .min()
        .map(|(_, id)| id)
}

/// The ground work for a plot with this north-west tile: levels all 16 tiles to one height (the
/// lowest corner under the middle), moving corners by at most 3 blocks and never disturbing anything
/// built nearby. `None` if the plot hits a road, track, water or another building site, or the ground
/// is too steep. Houses on the plot are fine: they are cleared.
pub fn plot_levelling(
    world: &World,
    towns: &Towns,
    roads: &Layer,
    rails: &Layer,
    anchor: (i32, i32),
) -> Option<Vec<((i32, i32), u16)>> {
    use crate::terrain::edit::Corners;
    let mut corners = Corners::with_limits(world, 3, 120);
    let h = corners
        .tile(anchor.0 + PLOT / 2, anchor.1 + PLOT / 2)?
        .iter()
        .copied()
        .min()?;
    for dz in 0..PLOT {
        for dx in 0..PLOT {
            let t = (anchor.0 + dx, anchor.1 + dz);
            if !world.in_bounds(t.0, t.1)
                || roads.get(t.0, t.1) != 0
                || rails.get(t.0, t.1) != 0
                || towns.occupied.get(t.0, t.1) > 1
                || world.surface(t.0, t.1).is_none_or(|(b, _)| b <= world.sea_level)
            {
                return None;
            }
            if !corners.level(t.0, t.1, h) {
                return None;
            }
        }
    }
    // Moving ground must not disturb anything built beside the plot.
    for (x, z) in corners.affected_tiles() {
        let inside = (anchor.0..anchor.0 + PLOT).contains(&x) && (anchor.1..anchor.1 + PLOT).contains(&z);
        let built = roads.get(x, z) != 0 || rails.get(x, z) != 0 || towns.occupied.get(x, z) > 1;
        let house = towns.occupied.get(x, z) == 1;
        // A house outside the plot can't sit on moved ground either.
        if !inside && (built || house) && corners.surface(x, z) != world.surface(x, z) {
            return None;
        }
    }
    Some(corners.changes())
}

/// Finds flat, clear ground for the plot close to the town centre, clears any
/// houses on it, and reserves it. `None` if the town has no room.
/// Whether a plot with this north-west tile is flat, dry and clear (houses are fine: they get
/// cleared; roads, track, ports and airports aren't).
pub fn plot_fits(world: &World, towns: &Towns, roads: &Layer, rails: &Layer, ax: i32, az: i32) -> bool {
    let Some((level, _)) = world.surface(ax, az) else {
        return false;
    };
    (0..PLOT).all(|dz| {
        (0..PLOT).all(|dx| {
            let (x, z) = (ax + dx, az + dz);
            world
                .surface(x, z)
                .is_some_and(|(base, shape)| shape.0 == 0 && base == level && base > world.sea_level)
                && roads.get(x, z) == 0
                && rails.get(x, z) == 0
                // 1 is a house (cleared when built); anything else is a port or airport.
                && towns.occupied.get(x, z) <= 1
        })
    })
}

/// Finds flat, clear ground for the plot close to the town centre, clears any
/// houses on it, and reserves it. `None` if the town has no room.
pub fn place(world: &World, towns: &mut Towns, roads: &Layer, rails: &Layer, town: TownId) -> Option<Hq> {
    let centre = towns.list.get(town as usize)?.centre;
    let fits = |ax: i32, az: i32, towns: &Towns| plot_fits(world, towns, roads, rails, ax, az);
    // Square rings around the centre, nearest first.
    for ring in 0..40i32 {
        let mut best: Option<((i32, i32), i32)> = None;
        for dz in -ring..=ring {
            for dx in -ring..=ring {
                if dx.abs().max(dz.abs()) != ring {
                    continue;
                }
                let anchor = (centre.0 + dx - PLOT / 2, centre.1 + dz - PLOT / 2);
                if !fits(anchor.0, anchor.1, towns) {
                    continue;
                }
                // Prefer plots with fewer houses to knock down.
                let houses = (0..PLOT)
                    .flat_map(|z| (0..PLOT).map(move |x| (x, z)))
                    .filter(|&(x, z)| towns.occupied.get(anchor.0 + x, anchor.1 + z) == 1)
                    .count() as i32;
                if best.is_none_or(|(_, h)| houses < h) {
                    best = Some((anchor, houses));
                }
            }
        }
        if let Some((anchor, _)) = best {
            let hq = Hq { anchor, town };
            towns.demolish(anchor, (anchor.0 + PLOT - 1, anchor.1 + PLOT - 1));
            for (x, z) in hq.tiles() {
                towns.occupied.set(x, z, 2);
            }
            return Some(hq);
        }
    }
    None
}

/// Moves the headquarters to a new plot: the old ground is freed, the new plot cleared of houses
/// and reserved. The caller has checked `plot_fits` and that a town is near.
pub fn relocate(towns: &mut Towns, old: &Hq, anchor: (i32, i32), town: TownId) -> Hq {
    for (x, z) in old.tiles() {
        towns.occupied.set(x, z, 0);
    }
    let hq = Hq { anchor, town };
    towns.demolish(anchor, (anchor.0 + PLOT - 1, anchor.1 + PLOT - 1));
    for (x, z) in hq.tiles() {
        towns.occupied.set(x, z, 2);
    }
    hq
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terrain::generation::{self, GenParams};

    fn generated(seed: u32) -> (World, Towns, Layer, Layer) {
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
    fn every_map_gets_a_garage_in_a_modest_town_with_room() {
        for seed in [1u32, 7, 42, 2024, 99] {
            let (world, mut towns, roads, rails) = generated(seed);
            let town = choose_town(&towns).expect("a town");
            let hq = place(&world, &mut towns, &roads, &rails, town).unwrap_or_else(|| panic!("no room, seed {seed}"));
            let level = world.surface(hq.anchor.0, hq.anchor.1).unwrap().0;
            for (x, z) in hq.tiles() {
                assert_eq!(towns.occupied.get(x, z), 2);
                assert!(!towns.buildings.contains_key(&(x, z)), "house left on the plot");
                assert_eq!(world.surface(x, z).unwrap().0, level);
                assert_eq!(roads.get(x, z), 0);
            }
            let c = towns.list[town as usize].centre;
            assert!(
                (hq.centre().0 - c.0 as f32).abs() < 30.0,
                "seed {seed}: HQ is far from its town"
            );
        }
    }

    #[test]
    fn the_plot_contains_its_tiles() {
        let hq = Hq {
            anchor: (10, 20),
            town: 0,
        };
        assert!(hq.contains((10, 20)) && hq.contains((13, 23)));
        assert!(!hq.contains((14, 20)) && !hq.contains((9, 20)));
        assert_eq!(hq.tiles().len(), 16);
    }

    /// The opening: from the garage's town to the next town along, a railway has to be possible and
    /// cheap enough for the starting money (GAME-DESIGN.md §9.2).
    #[test]
    fn the_first_railway_is_possible_and_affordable() {
        use crate::company;
        use crate::rails::{self, Signals};
        let (mut tried, mut ok) = (0, 0);
        for seed in [1u32, 7, 42, 2024, 99, 5, 11, 123] {
            let (mut world, mut towns, roads, rails_layer) = generated(seed);
            let town = choose_town(&towns).expect("a town");
            let Some(hq) = place(&world, &mut towns, &roads, &rails_layer, town) else {
                continue;
            };
            let c = towns.list[town as usize].centre;
            let dist = |o: &crate::towns::Town| (((o.centre.0 - c.0).pow(2) + (o.centre.1 - c.1).pow(2)) as f32).sqrt();
            let Some(other) = towns
                .list
                .iter()
                .filter(|o| o.id != town && (30.0..=130.0).contains(&dist(o)))
                .min_by(|a, b| dist(a).total_cmp(&dist(b)))
            else {
                continue;
            };
            // Open ground at the edge of each town, as a player would pick.
            let open = |centre: (i32, i32)| {
                (0..25i32).find_map(|r| {
                    (-r..=r)
                        .flat_map(|dz| (-r..=r).map(move |dx| (dx, dz)))
                        .find_map(|(dx, dz)| {
                            let t = (centre.0 + dx, centre.1 + dz);
                            let clear = (-1..=1).all(|oz| {
                                (-1..=1).all(|ox| {
                                    roads.get(t.0 + ox, t.1 + oz) == 0
                                        && towns.occupied.get(t.0 + ox, t.1 + oz) == 0
                                        && world
                                            .surface(t.0 + ox, t.1 + oz)
                                            .is_some_and(|(b, _)| b > world.sea_level)
                                })
                            });
                            (dx.abs().max(dz.abs()) == r && clear).then_some(t)
                        })
                })
            };
            let _ = hq;
            let (Some(a), Some(b)) = (open(c), open(other.centre)) else {
                continue;
            };
            let plan = rails::plan_route(&world, &rails_layer, &roads, &towns.occupied, &Signals::default(), a, b);
            tried += 1;
            if plan.blocked.is_some() {
                eprintln!("seed {seed}: first railway blocked: {:?}", plan.blocked);
                continue;
            }
            ok += 1;
            let cost = plan.pieces.len() as u32 * company::RAIL_PIECE + company::terraform_cost(&plan);
            eprintln!(
                "seed {seed}: {} tiles, {} corner-steps, track costs £{cost}",
                plan.pieces.len(),
                plan.terraform_steps
            );
            assert!(cost <= 20_000, "seed {seed}: the first railway costs £{cost}");
            let _ = &mut world;
        }
        assert!(tried >= 5 && ok * 10 >= tried * 8, "{ok} of {tried}");
    }

    #[test]
    fn moving_the_headquarters_frees_the_old_plot_and_clears_the_new_one() {
        let (world, mut towns, roads, rails) = generated(42);
        let town = choose_town(&towns).unwrap();
        let old = place(&world, &mut towns, &roads, &rails, town).unwrap();
        // Another town with room.
        let other = towns.list.iter().map(|t| t.id).find(|&t| t != town).unwrap();
        let spot = {
            let mut probe = generated(42).1;
            place(&world, &mut probe, &roads, &rails, other).unwrap().anchor
        };
        assert!(plot_fits(&world, &towns, &roads, &rails, spot.0, spot.1));
        let new = relocate(&mut towns, &old, spot, other);
        for t in old.tiles() {
            assert_eq!(towns.occupied.get(t.0, t.1), 0, "the old plot is free again");
        }
        for t in new.tiles() {
            assert_eq!(towns.occupied.get(t.0, t.1), 2);
            assert!(!towns.buildings.contains_key(&t));
        }
        assert_eq!(new.town, other);
        // Its own plot can't be built on twice.
        assert!(!plot_fits(&world, &towns, &roads, &rails, spot.0, spot.1));
        assert!(crate::company::relocation_cost(2) > crate::company::relocation_cost(1));
    }
}
