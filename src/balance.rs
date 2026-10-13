//! The balance report: fifty years played headless by a sensible, unhurried model player, using the
//! game's own rules wherever they exist (the company's books and headquarters levels, fares measured
//! on a real line, compute, events, hosting, launches). It answers "when can a player reach X?" so
//! the dials in GAME-DESIGN.md can be tuned against numbers instead of guesses.
//!
//! Run it with `cargo test balance -- --nocapture` to see the year-by-year table.
//!
//! The model player (written down so it can be argued with):
//! - joins town pairs by rail, nearest pairs first, whenever the money for the next line (and a
//!   cushion) is there; a line pays what the headless economy test measured for two towns of 500,
//!   in proportion to the smaller town, through the year's events;
//! - upgrades the headquarters as soon as every requirement is met;
//! - from the Regional office, feeds a server farm with goods; from 2005 a data centre with chips;
//!   from 2020 AI campuses (one more each time there's £1M spare); all compute is sold as hosting;
//! - from 2030 runs a spaceport once there's £2M spare;
//! - in 2046, if the Citadel stands and £5M is spare, builds the Convergence Array over two years.
//!
//! Towns grow at 5% a year for every point of Prosperity (served towns have stations, service and
//! later expressways), which is the growth `towns.rs` gives in rough terms.

#[cfg(test)]
mod tests {
    use crate::company::{self, Assets, Company, Facts, Item};
    use crate::compute::{self, Compute};
    use crate::events;
    use crate::space;
    use crate::terrain::generation::{self, GenParams};
    use crate::towns::Towns;

    /// From the headless economy test: a line between two towns of 500 people.
    const LINE_INCOME: f32 = 11_900.0;
    const LINE_PIECES: u32 = 92;
    const LINE_STATION_TILES: u32 = 8;
    const LINE_TRAIN: u32 = 7_200;
    const LINE_TRAIN_RUNNING: u32 = 680;
    /// A freight line feeding a compute site (two of them for the chip chain).
    const FREIGHT_LINE: u32 = 15_000;

    #[derive(Default, Debug, Clone)]
    struct Milestones {
        hq: Vec<(u8, f32)>,
        tiers: Vec<(usize, f32)>,
        convergence: Option<f32>,
    }

    #[test]
    fn balance_report() {
        let p = GenParams::default();
        let world = generation::generate(&p);
        let mut roads = crate::network::Layer::new(world.size_x, world.size_z);
        let rails = crate::network::Layer::new(world.size_x, world.size_z);
        let towns = Towns::generate(&world, &mut roads, &rails, p.seed, 2000);
        // Town pairs, nearest first.
        let mut pairs: Vec<(f32, usize, usize)> = Vec::new();
        for (i, a) in towns.list.iter().enumerate() {
            for (j, b) in towns.list.iter().enumerate().skip(i + 1) {
                let d = (((a.centre.0 - b.centre.0).pow(2) + (a.centre.1 - b.centre.1).pow(2)) as f32).sqrt();
                if d < 160.0 {
                    pairs.push((d, i, j));
                }
            }
        }
        pairs.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut pop: Vec<f32> = towns.list.iter().map(|t| t.pop.max(100) as f32).collect();
        let mut served = vec![false; pop.len()];

        let mut c = Company::new(2000);
        // The first contract's seed money (§9.3).
        c.earn(Item::Contracts, 20_000);
        let mut lines: Vec<(usize, usize, f32)> = Vec::new();
        let mut assets = Assets::default();
        let mut comp = Compute::default();
        let (mut farm, mut dc, mut campuses, mut spaceport) = (false, false, 0u32, false);
        let mut array_started: Option<f32> = None;
        let mut m = Milestones::default();
        let mut table = Vec::new();

        for month in 0..(51 * 12) {
            let date = 2000.0 + month as f32 / 12.0;
            let year = date.floor() as i32;
            let world_mods = events::active(date, p.seed);
            c.world = world_mods;

            // Build lines while the money allows.
            while let Some(&(d, a, b)) = pairs.first() {
                let cost = (d * 1.4) as u32 * company::RAIL_PIECE + LINE_STATION_TILES * company::STATION_TILE + LINE_TRAIN;
                if c.cash < cost as i64 + 5_000 || assets.trains >= c.vehicle_limit() {
                    break;
                }
                pairs.remove(0);
                c.spend(Item::Construction, cost - LINE_TRAIN);
                c.spend(Item::Vehicles, LINE_TRAIN);
                lines.push((a, b, d));
                served[a] = true;
                served[b] = true;
                assets.trains += 1;
                assets.train_value += LINE_TRAIN;
                assets.train_running += LINE_TRAIN_RUNNING;
                assets.rail_pieces += (d * 1.4) as u32;
                assets.station_tiles += LINE_STATION_TILES;
            }

            // A month of fares: the line pays in proportion to its smaller town and its length.
            let mut fares = 0.0;
            for &(a, b, d) in &lines {
                fares += LINE_INCOME / 12.0 * (pop[a].min(pop[b]) / 500.0).min(40.0) * (d / LINE_PIECES as f32);
            }
            let fares = fares * world_mods.passengers;
            c.earn(Item::Passengers, fares as u32);

            // Compute.
            let facts = Facts {
                rail_pieces: assets.rail_pieces,
                stations: assets.station_tiles / 4,
                trains: assets.trains,
                town_served: served.iter().zip(&pop).filter(|(s, _)| **s).map(|(_, p)| *p as u32).max().unwrap_or(0),
                vehicles: assets.trains,
                compute_sites: farm as u32 + dc as u32 + campuses,
                compute_total: comp.total,
                compute_tier: comp.tier,
                serves_data_centre: dc,
                serves_ai_campus: campuses > 0,
            };
            if !farm && c.hq_level >= 2 && year <= 2015 && c.cash > FREIGHT_LINE as i64 * 2 {
                c.spend(Item::Construction, FREIGHT_LINE);
                farm = true;
            }
            if !dc && year >= 2005 && c.hq_level >= 2 && c.cash > FREIGHT_LINE as i64 * 3 {
                c.spend(Item::Construction, FREIGHT_LINE * 2);
                dc = true;
            }
            if year >= 2020 && c.hq_level >= 2 && (campuses == 0 && c.cash > 100_000 || c.cash > 1_000_000) {
                c.spend(Item::Construction, if campuses == 0 { FREIGHT_LINE * 2 } else { 250_000 + FREIGHT_LINE });
                campuses += 1;
            }
            let ymid = year as f64 + 0.5;
            let (mut cu, mut batches) = (0.0, 0.0);
            for (on, scale) in [(farm, 1.0), (dc, 5.0), (campuses > 0, 20.0 * campuses as f64)] {
                if on {
                    cu += compute::monthly(40, scale, ymid);
                    batches += 40.0 * scale;
                }
            }
            c.earn(Item::Hosting, compute::hosting_income(batches, year));
            if let Some(t) = comp.month(cu, 30.0) {
                m.tiers.push((t, date));
            }

            // Space.
            if !spaceport && year >= 2030 && c.cash > 2_000_000 {
                c.spend(Item::Construction, 250_000 + FREIGHT_LINE * 2);
                spaceport = true;
            }
            if spaceport {
                let to = space::Destination::for_year(date);
                for _ in 0..2 {
                    let l = space::launch(date, 0.5, to);
                    c.earn(Item::Launches, l.revenue);
                    c.charge(Item::Running, l.cost);
                }
                c.charge(Item::Upkeep, space::UPKEEP);
            }

            // The Array.
            if array_started.is_none() && year >= 2046 && c.hq_level >= 6 && c.cash > 5_000_000 {
                c.spend(Item::Construction, 5_000_000);
                array_started = Some(date);
            }
            if let Some(start) = array_started {
                if m.convergence.is_none() && date - start >= 2.0 && comp.total >= compute::CONVERGENCE {
                    m.convergence = Some(date);
                }
            }

            // The headquarters, as soon as it can.
            if c.upgrade(&assets, &facts) {
                m.hq.push((c.hq_level, date));
            }
            // The month's books.
            c.tick(30.0, year, &assets);

            // Towns grow, faster where served and connected.
            for (i, p) in pop.iter_mut().enumerate() {
                let prosperity = if served[i] { 2.0 + if year >= 2035 { 0.8 } else { 0.0 } } else { 1.0 };
                *p *= 1.0 + 0.05 * prosperity / 12.0 * world_mods.growth;
            }

            if month % 60 == 0 || month == 51 * 12 - 1 {
                table.push(format!(
                    "{year}  cash {:>14}  value {:>14}  lines {:>3}  biggest served {:>8.0}  HQ {}  compute {:>11}  tier {}",
                    company::pounds(c.cash),
                    company::pounds(c.value(&assets)),
                    lines.len(),
                    facts.town_served,
                    c.hq_level,
                    compute::show(comp.total),
                    comp.tier
                ));
            }
        }

        eprintln!("\n--- Balance report (default map, model player) ---");
        for row in &table {
            eprintln!("{row}");
        }
        for (level, when) in &m.hq {
            eprintln!("HQ level {level} ({}) in {when:.1}", company::level(*level).name);
        }
        for (tier, when) in &m.tiers {
            eprintln!("Compute milestone {} in {when:.1}", compute::TIERS[tier - 1].0);
        }
        eprintln!("Convergence: {:?}", m.convergence.map(|d| format!("{d:.1}")));

        // Loose guard rails: the opening works, and the early levels come in the first decade.
        let when = |l: u8| m.hq.iter().find(|(x, _)| *x == l).map(|&(_, d)| d);
        assert!(when(1).is_some_and(|d| d < 2003.0), "Depot office: {:?}", when(1));
        assert!(when(2).is_some_and(|d| d < 2012.0), "Regional office: {:?}", when(2));
    }
}
