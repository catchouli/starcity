//! What happens at a station: cargo trickles in from the people nearby, and
//! trains unload, get paid and load. Shared by the game and the headless
//! economy test (GAME-DESIGN.md §3, §5.2, §5.6).

use crate::cargo::{self, Cargo};
use crate::stations::Station;
use crate::trains::Train;

/// A train's top speed, which is what a station's rating sees (tiles per second).
pub const TRAIN_TOP_SPEED: f32 = 3.0;

/// Once a second: cargo arrives at a station. Each person nearby makes about
/// one trip every 100 seconds and sends a bag of mail a fifth as often; a
/// better-rated station gets more of both. `people` is the catchment's population.
pub fn accrue(station: &mut Station, people: f32, now: f64) {
    for (cargo, per_person) in [(Cargo::Passengers, 0.01), (Cargo::Mail, 0.002)] {
        let service = &mut station.service[cargo.index()];
        service.update(now, station.stock.get(cargo), 1.0);
        station
            .stock
            .add(cargo, people * per_person * cargo::share(service.rating));
    }
}

/// What a train left at a station.
pub struct Delivery {
    pub cargo: Cargo,
    pub units: u32,
    /// Where and when the units got on.
    pub boarded: Option<(f32, f32, f64)>,
    /// Where they got off.
    pub at: (f32, f32),
}

/// A train has pulled in: everything on board comes off, then it loads what's waiting.
/// Calling is service whether or not there was anything to load.
pub fn arrive(station: &mut Station, train: &mut Train, now: f64, year: i32) -> Delivery {
    let cargo = train.cargo;
    let units = std::mem::take(&mut train.load);
    let boarded = train.boarded.take();
    let boarding = station.stock.take(cargo, train.capacity());
    train.load = boarding;
    let at = station.centre();
    if boarding > 0 {
        train.boarded = Some((at.0, at.1, now));
    }
    station.service[cargo.index()].loaded(now, TRAIN_TOP_SPEED, year - train.born);
    Delivery {
        cargo,
        units,
        boarded,
        at,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cargo::fare;
    use crate::company;
    use crate::network::Layer;
    use crate::rails::Signals;
    use crate::scenario::{Railway, build_lines};
    use crate::stations::Stations;
    use crate::terrain::generation::{self, GenParams};
    use crate::towns::Towns;
    use crate::trains::Trains;

    /// The headless economy test (GAME-DESIGN.md §8): build real lines on a generated map, run
    /// the game's own loading, rating and fares for a few economy years, and check the opening
    /// pays back in about the time the design promises.
    #[test]
    fn a_first_railway_pays_for_itself_in_about_a_year_or_two() {
        let p = GenParams::default();
        let world = generation::generate(&p);
        let (sx, sz) = (world.size_x, world.size_z);
        let mut roads = Layer::new(sx, sz);
        let mut rails = Layer::new(sx, sz);
        let towns = Towns::generate(&world, &mut roads, &rails, p.seed, 2000);
        let signals = Signals::default();
        let mut stations = Stations::default();
        let lines = {
            let mut rw = Railway {
                world: &world,
                roads: &roads,
                buildings: &towns.occupied,
                signals: &signals,
                rails: &mut rails,
                stations: &mut stations,
            };
            build_lines(&mut rw, &towns, 12)
        };
        assert!(!lines.is_empty());
        // The line whose weaker end has the most people near it: a player puts stations in towns.
        let weaker = |l: &crate::scenario::Line| {
            let near = |sid| towns.population_near(&stations.get(sid).unwrap().tiles);
            near(l.from).min(near(l.to))
        };
        let line = lines.iter().max_by_key(|l| weaker(l)).unwrap();
        // One line only, with one train, so the earnings are that line's.
        let mut trains = Trains::new(1);
        let id = trains.spawn(&rails, line.start, line.forward, 3).unwrap();
        trains.set_orders(id, vec![line.to, line.from], 0);
        let platform_tiles = [
            stations.get(line.from).unwrap().tiles.clone(),
            stations.get(line.to).unwrap().tiles.clone(),
        ];
        let distance = {
            let (a, b) = (
                stations.get(line.from).unwrap().centre(),
                stations.get(line.to).unwrap().centre(),
            );
            ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
        };
        // The design's reference route: two towns of about 500 people (the stations here sit further
        // out than a player would put them, so the real catchment is overridden).
        let people = [500u32, 500];

        let years = 3.0;
        let (mut now, mut income, mut trips) = (0.0f64, 0u64, 0u32);
        while now < 360.0 * years {
            // Ten steps of a tenth of a second, then a second of town life.
            for _ in 0..10 {
                trains.update(0.1, &rails, &signals, &stations, 1);
                for (tid, sid) in std::mem::take(&mut trains.arrivals) {
                    let st = stations.list.get_mut(&sid).unwrap();
                    let d = arrive(st, trains.train(tid), now, 2000);
                    if let Some((x, z, since)) = d.boarded {
                        let miles = ((d.at.0 - x).powi(2) + (d.at.1 - z).powi(2)).sqrt();
                        income += fare(d.cargo, d.units, miles, (now - since) as f32) as u64;
                        trips += 1;
                    }
                }
                now += 0.1;
            }
            for (i, sid) in [line.from, line.to].into_iter().enumerate() {
                accrue(stations.list.get_mut(&sid).unwrap(), people[i] as f32, now);
            }
        }
        let per_year = income as f32 / years as f32;
        let station_tiles: usize = platform_tiles.iter().map(|t| t.len()).sum();
        // Track between the stations, at the game's prices (a straight-ish line), plus stations and a train.
        let capital = distance.round() as u32 * company::RAIL_PIECE
            + station_tiles as u32 * company::STATION_TILE
            + company::TRAIN;
        eprintln!(
            "line of {distance:.0} tiles between {people:?} people: {trips} loaded trips, £{per_year:.0} a year, capital about £{capital}, payback {:.2} years",
            capital as f32 / per_year
        );
        assert!(trips > 10, "the train never carried anyone");
        let payback = capital as f32 / per_year;
        assert!(
            (0.4..=1.6).contains(&payback),
            "payback {payback:.2} years (£{per_year:.0} a year on £{capital})"
        );
    }
}
