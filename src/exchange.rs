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
    for cargo in Cargo::ALL {
        station.service[cargo.index()].update(now, station.stock.get(cargo), 1.0);
    }
    for (cargo, per_person) in [(Cargo::Passengers, 0.01), (Cargo::Mail, 0.002)] {
        let rating = station.service[cargo.index()].rating;
        station.stock.add(cargo, people * per_person * cargo::share(rating));
    }
}

/// An industry's output reaching the stations that serve it: the best rating decides how much of it
/// is collected at all (66% or better takes everything), and the stations share that in proportion
/// to their ratings. Returns the whole units that were handed over.
pub fn collect(stations: &mut [&mut Station], cargo: Cargo, units: u32) -> u32 {
    let best = stations
        .iter()
        .map(|s| s.service[cargo.index()].rating)
        .fold(0.0f32, f32::max);
    let total: f32 = stations.iter().map(|s| s.service[cargo.index()].rating).sum();
    if best <= 0.0 || total <= 0.0 {
        return 0;
    }
    let taken = (units as f32 * cargo::share(best).min(1.0)).floor();
    for s in stations.iter_mut() {
        let part = taken * s.service[cargo.index()].rating / total;
        s.stock.add(cargo, part);
    }
    taken as u32
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

/// A train has pulled in: whatever the place wants comes off (`room` says how much of its cargo can be
/// taken: unlimited for people and mail, whatever the industries nearby have room for otherwise),
/// then it loads what's waiting. Calling is service whether or not there was anything to load.
pub fn arrive(station: &mut Station, train: &mut Train, now: f64, year: i32, room: u32) -> Delivery {
    let cargo = train.cargo;
    let units = train.load.min(room);
    train.load -= units;
    let boarded = if units > 0 { train.boarded.take() } else { None };
    let boarding = station.stock.take(cargo, train.capacity() - train.load);
    let at = station.centre();
    if boarding > 0 {
        if train.load == 0 {
            train.boarded = Some((at.0, at.1, now));
        }
        train.load += boarding;
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
                    let d = arrive(st, trains.train(tid), now, 2000, u32::MAX);
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

    /// The freight loop end to end, without a map: a mine makes coal, the station by it collects, a
    /// train hauls it, the power station takes it, and the train is paid by distance.
    #[test]
    fn coal_goes_from_the_mine_to_the_power_station_and_pays() {
        use crate::cargo::{Service, Stock};
        use crate::industry::{Industries, Kind};
        use crate::rails::X;
        let mut rails = Layer::new(64, 4);
        for x in 0..64 {
            rails.set(x, 1, X);
        }
        let mut trains = Trains::new(1);
        let id = trains.spawn(&rails, (5, 1), true, 3).unwrap();
        let train = trains.train(id);
        train.cargo = Cargo::Coal;

        let mut ind = Industries::default();
        let (mine, plant) = (ind.add(Kind::CoalMine, (8, 1)), ind.add(Kind::PowerStation, (50, 1)));
        let station = |tiles: Vec<(i32, i32)>| Station {
            id: 0,
            name: String::new(),
            tiles,
            stock: Stock::default(),
            service: [Service::default(); cargo::COUNT],
            people: None,
        };
        let (mut at_mine, mut at_plant) = (station(vec![(9, 2)]), station(vec![(51, 2)]));
        // A train has been calling at the mine, so it is well rated.
        at_mine.service[Cargo::Coal.index()].rating = 0.9;
        assert!(ind.near(&at_mine.tiles).contains(&mine) && ind.near(&at_plant.tiles).contains(&plant));

        let mut income = 0u32;
        let mut delivered = 0u32;
        for month in 0..6 {
            // The mine's month: its coal goes to the station that serves it.
            for (i, cargo, units) in ind.month() {
                if i == mine {
                    exchange_collect(&mut at_mine, cargo, units);
                }
            }
            // The train: load at the mine, unload at the plant.
            let now = month as f64 * 30.0;
            arrive(&mut at_mine, trains.train(id), now, 2000, 0);
            let room = ind.list[plant].room(Cargo::Coal);
            let d = arrive(&mut at_plant, trains.train(id), now + 20.0, 2000, room);
            ind.deliver(plant, Cargo::Coal, d.units);
            delivered += d.units;
            if let Some((x, z, since)) = d.boarded {
                let tiles = ((d.at.0 - x).powi(2) + (d.at.1 - z).powi(2)).sqrt();
                income += fare(Cargo::Coal, d.units, tiles, (now + 20.0 - since) as f32);
            }
        }
        eprintln!(
            "coal: {delivered} t delivered in 6 months, £{income}; plant stock {}",
            ind.list[plant].stock[Cargo::Coal.index()]
        );
        assert!(delivered >= 100, "the contract's hundred tonnes: {delivered}");
        assert!(income > 3_000, "£{income}");
        // What the plant got, it burns for power.
        assert!(ind.list[plant].stock[Cargo::Coal.index()] < delivered, "it burns coal");
    }

    fn exchange_collect(station: &mut Station, cargo: Cargo, units: u32) {
        collect(&mut [station], cargo, units);
    }
}
