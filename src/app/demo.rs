//! Demo mode as a living showcase: besides what `start_demo` builds on day one, the demo keeps the
//! map up to date with the era as time passes (and as the date slider is dragged). New industries
//! are joined by freight lines when they're founded, the hyper expressway appears in 2035, sea walls
//! go up before the first rise. Dragging back in time tears down what belongs to later years.

use super::*;
use std::collections::HashSet;

/// Something the demo built, and the year it belongs to.
pub(super) struct DemoBuilt {
    pub year: i32,
    pub rails: Vec<(i32, i32)>,
    pub trains: Vec<u32>,
    pub what: Built,
}

pub(super) enum Built {
    Freight,
    Expressway,
    Walls(Vec<(i32, i32)>),
}

#[derive(Default)]
pub(super) struct DemoState {
    pub built: Vec<DemoBuilt>,
    /// Industries the demo has already tried to join up.
    pub connected: HashSet<usize>,
}

impl StarcityApp {
    /// Once a month in the demo (and after seeking): bring the map up to date with the era.
    pub(super) fn demo_month(&mut self) {
        if self.demo.is_none() {
            return;
        }
        self.demo_connect();
        self.demo_expressway();
        self.demo_walls();
    }

    /// Trucks in the demo: from each food plant and factory near a town, a road (if it needs one) to
    /// the town, a road stop at each end, and a truck carrying the food or goods in. Returns how many.
    pub(super) fn demo_trucks(&mut self) -> usize {
        use crate::terminals::{Orders, Site};
        let mut made = 0;
        let sites: Vec<(usize, crate::cargo::Cargo)> = self
            .industries
            .list
            .iter()
            .filter(|i| !i.closed)
            .filter_map(|i| match i.kind {
                crate::industry::Kind::FoodPlant => Some((i.id, crate::cargo::Cargo::Food)),
                crate::industry::Kind::Factory => Some((i.id, crate::cargo::Cargo::Goods)),
                _ => None,
            })
            .collect();
        for (id, cargo) in sites {
            let ind = self.industries.list[id].clone();
            let (cx, cz) = (ind.tile.0 + ind.size() / 2, ind.tile.1 + ind.size() / 2);
            let Some(town) = self.towns.nearest((cx, cz), 30) else { continue };
            let tc = self.towns.list[town as usize].centre;
            // The town's road tile nearest the industry, and a start just outside the industry's yard
            // on the side facing the town.
            let Some(end) = (tc.1 - 10..=tc.1 + 10)
                .flat_map(|z| (tc.0 - 10..=tc.0 + 10).map(move |x| (x, z)))
                .filter(|&(x, z)| self.roads.get(x, z) != 0 && self.terminals.at((x, z)).is_none())
                .min_by_key(|&(x, z)| (x - cx).abs() + (z - cz).abs())
            else {
                continue;
            };
            let s = ind.size();
            let start = (
                (end.0).clamp(ind.tile.0 - 1, ind.tile.0 + s),
                (end.1).clamp(ind.tile.1 - 1, ind.tile.1 + s),
            );
            if ind.contains(start) {
                continue;
            }
            let corner = (end.0, start.1);
            let plans: Vec<crate::network::Plan> = [(start, corner), (corner, end)]
                .into_iter()
                .filter(|(a, b)| a != b)
                .map(|(a, b)| crate::roads::plan(&self.world, &self.roads, &self.rails, &self.towns.occupied, a, b))
                .collect();
            if plans.is_empty() || plans.iter().any(|p| p.blocked.is_some()) {
                continue;
            }
            let mut infra = Vec::new();
            for p in &plans {
                infra.extend(p.build(&mut self.roads));
            }
            self.mark_infra(&infra);
            let stops: Vec<TerminalId> = [start, end]
                .into_iter()
                .map(|tile| {
                    let id = self.terminals.build(Site::BusStop { tile }, String::new());
                    self.name_terminal(id);
                    id
                })
                .collect();
            if let Some(truck) = self.buses.spawn(&self.roads, start) {
                if let Some(b) = self.buses.get_mut(truck) {
                    b.cargo = cargo;
                    b.capacity = 20;
                    b.speed = 1.6;
                }
                self.buses.set_orders(truck, Orders::new(vec![stops[0], stops[1]]));
                made += 1;
            }
        }
        self.static_changed = true;
        self.props_changed = true;
        made
    }

    /// Freight lines for industries founded since the demo last looked.
    fn demo_connect(&mut self) {
        let Some(demo) = &self.demo else { return };
        let fresh: HashSet<usize> = self
            .industries
            .list
            .iter()
            .filter(|i| !i.closed && !demo.connected.contains(&i.id))
            .map(|i| i.id)
            .collect();
        if fresh.is_empty() {
            return;
        }
        let before = self.rails.clone();
        let mut railway = crate::scenario::Railway {
            world: &self.world,
            roads: &self.roads,
            buildings: &self.towns.occupied,
            signals: &self.signals,
            rails: &mut self.rails,
            stations: &mut self.stations,
            level: true,
            terraform: Vec::new(),
        };
        let lines = crate::scenario::build_freight_for(&mut railway, &self.industries, 12, Some(&fresh));
        let terraform = std::mem::take(&mut railway.terraform);
        let moved = crate::terrain::edit::apply(&mut self.world, &terraform);
        self.levelled_pending.extend(moved);
        let mut trains = Vec::new();
        for f in &lines {
            if let Ok(id) = self.trains.spawn(&self.rails, f.line.start, f.line.forward, 3) {
                self.trains.set_orders(id, vec![f.line.to, f.line.from], 0);
                self.trains.train(id).cargo = f.cargo;
                trains.push(id);
            }
        }
        let rails = self.rails_changed_since(&before);
        let year = self.year;
        log::info!("demo {year}: {} new freight lines for {} new industries", lines.len(), fresh.len());
        let demo = self.demo.as_mut().unwrap();
        demo.connected.extend(fresh);
        if !rails.is_empty() {
            demo.built.push(DemoBuilt {
                year,
                rails: rails.clone(),
                trains,
                what: Built::Freight,
            });
            let ids: Vec<StationId> = self.stations.list.keys().copied().collect();
            for id in ids {
                if self.stations.get(id).is_some_and(|s| s.name.is_empty()) {
                    self.name_station(id);
                }
            }
            self.mark_infra(&rails);
            self.track_version += 1;
            self.plan_epoch += 1;
            self.props_changed = true;
            self.static_changed = true;
        }
    }

    fn rails_changed_since(&self, before: &Layer) -> Vec<(i32, i32)> {
        let mut out = Vec::new();
        for z in 0..self.rails.size_z as i32 {
            for x in 0..self.rails.size_x as i32 {
                if self.rails.get(x, z) != before.get(x, z) {
                    out.push((x, z));
                }
            }
        }
        out
    }

    /// From 2035: an expressway between the two biggest cities, as an L of two straight runs.
    pub(super) fn demo_expressway(&mut self) {
        if self.demo.is_none() || self.tech_year() < crate::expressway::FROM || !self.express.height.is_empty() {
            return;
        }
        let mut towns: Vec<(u32, (i32, i32))> = self.towns.list.iter().map(|t| (t.pop, t.centre)).collect();
        towns.sort_by(|a, b| b.0.cmp(&a.0));
        let (Some(&(_, a)), Some(&(_, b))) = (towns.first(), towns.get(1)) else { return };
        let corner = (b.0, a.1);
        for (from, to) in [(a, corner), (corner, b)] {
            let plan = self.express.plan(from, to);
            self.express.build(&plan, &self.world);
        }
        log::info!("demo {}: hyper expressway, {} pieces", self.year, self.express.pieces());
        self.static_changed = true;
        if let Some(demo) = &mut self.demo {
            demo.built.push(DemoBuilt {
                year: crate::expressway::FROM,
                rails: Vec::new(),
                trains: Vec::new(),
                what: Built::Expressway,
            });
        }
    }

    /// A few years before the first rise: sea walls along the shore of the town with most to lose.
    fn demo_walls(&mut self) {
        let date = self.clock.year_f64() as f32;
        let first = crate::flood::RISES[0];
        if date < first - 3.0 || self.demo.as_ref().is_some_and(|d| d.built.iter().any(|b| matches!(b.what, Built::Walls(_)))) {
            return;
        }
        let risk = self.flood.at_risk(&self.world, &self.walls);
        // The town with most buildings at risk.
        let Some(town) = self.towns.list.iter().max_by_key(|t| {
            risk.iter()
                .filter(|&&p| self.towns.buildings.get(&p).is_some_and(|b| b.town == t.id))
                .count()
        }) else {
            return;
        };
        let centre = town.centre;
        // Wall the at-risk tiles that touch the water near it, where nothing is built.
        let tiles: Vec<(i32, i32)> = risk
            .iter()
            .copied()
            .filter(|&(x, z)| (x - centre.0).abs() <= 30 && (z - centre.1).abs() <= 30)
            .filter(|&(x, z)| [(1, 0), (-1, 0), (0, 1), (0, -1)].iter().any(|(dx, dz)| self.world.underwater(x + dx, z + dz)))
            .filter(|&(x, z)| self.roads.get(x, z) == 0 && self.rails.get(x, z) == 0 && self.towns.occupied.get(x, z) == 0)
            .collect();
        log::info!("demo {}: {} tiles of sea wall by {}", self.year, tiles.len(), town.name);
        for &(x, z) in &tiles {
            self.walls.set(x, z, 1);
            self.towns.occupied.set(x, z, 4);
        }
        self.flood.walls_changed(&self.world, &self.walls);
        self.static_changed = true;
        let year = self.year;
        if let Some(demo) = &mut self.demo {
            demo.built.push(DemoBuilt {
                year,
                rails: Vec::new(),
                trains: Vec::new(),
                what: Built::Walls(tiles),
            });
        }
    }

    /// Dragging the date back: what the demo built in later years comes down again.
    pub(super) fn demo_rewind(&mut self, year: i32) {
        let Some(demo) = &mut self.demo else { return };
        let (later, keep): (Vec<DemoBuilt>, Vec<DemoBuilt>) = std::mem::take(&mut demo.built).into_iter().partition(|b| b.year > year);
        demo.built = keep;
        log::info!("demo: back to {year}, taking down {} later builds", later.len());
        for b in later {
            for id in b.trains {
                self.trains.remove(id);
            }
            for &(x, z) in &b.rails {
                self.rails.set(x, z, 0);
            }
            self.mark_infra(&b.rails);
            match b.what {
                Built::Freight => {}
                Built::Expressway => {
                    let (sx, sz) = (self.world.size_x as i32 - 1, self.world.size_z as i32 - 1);
                    self.express.clear((0, 0), (sx, sz));
                }
                Built::Walls(tiles) => {
                    for (x, z) in tiles {
                        self.walls.set(x, z, 0);
                        self.towns.occupied.set(x, z, 0);
                    }
                }
            }
        }
        // Industries that were closed by the rewind can be joined up again when they're refounded.
        let closed: Vec<usize> = self.industries.list.iter().filter(|i| i.closed).map(|i| i.id).collect();
        if let Some(demo) = &mut self.demo {
            for id in closed {
                demo.connected.remove(&id);
            }
        }
        self.track_version += 1;
        self.plan_epoch += 1;
        self.props_changed = true;
        self.static_changed = true;
    }
}
