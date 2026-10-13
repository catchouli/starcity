//! The agents (GAME-DESIGN.md §5.8): software managers that spend a little compute each month to
//! add trains where cargo is piling up, within the company's budget and its headquarters' limit.

use super::StarcityApp;
use crate::company::{self, Item};
use crate::stations::StationId;
use crate::trains::SpawnError;

impl StarcityApp {
    /// Once a month: the agents use what their share of compute allows.
    pub(super) fn agents_month(&mut self) {
        let allowance = self.company.effects.agent_actions;
        if allowance <= 0.0 {
            self.agent_budget = 0.0;
            return;
        }
        self.agent_budget = (self.agent_budget + allowance).min(4.0);
        while self.agent_budget >= 1.0 {
            // The charter decides what kind of thing to try first; the other is the fallback.
            self.launch_rng ^= self.launch_rng << 13;
            self.launch_rng ^= self.launch_rng >> 7;
            self.launch_rng ^= self.launch_rng << 17;
            let roll = (self.launch_rng >> 11) as f32 / (1u64 << 53) as f32;
            let people_first = self.charter_active() && self.charter.agent_goal(roll) == crate::charter::PEOPLE;
            let done = if people_first {
                self.agent_connects_a_town() || self.agent_adds_a_train()
            } else {
                self.agent_adds_a_train() || (self.charter_active() && self.agent_connects_a_town())
            };
            if !done {
                break;
            }
            self.agent_budget -= 1.0;
        }
    }

    /// Whether a town has a station within reach of its centre.
    fn town_served(&self, centre: (i32, i32)) -> bool {
        self.stations.list.values().any(|s| {
            s.tiles
                .iter()
                .any(|&(x, z)| (x - centre.0).abs() <= 14 && (z - centre.1).abs() <= 14)
        })
    }

    /// The People agents: a rail line and a train from the biggest town nobody serves to the nearest
    /// town that is served. False if there's nothing to do or it can't be built or afforded.
    fn agent_connects_a_town(&mut self) -> bool {
        let mut lonely: Vec<(u32, (i32, i32))> = self
            .towns
            .list
            .iter()
            .filter(|t| t.pop >= 150 && !self.town_served(t.centre))
            .map(|t| (t.pop, t.centre))
            .collect();
        lonely.sort_by_key(|x| std::cmp::Reverse(x.0));
        let order = crate::catalog::TrainOrder::default();
        for &(_, from) in lonely.iter().take(3) {
            let Some(to) = self
                .towns
                .list
                .iter()
                .filter(|t| t.centre != from && self.town_served(t.centre))
                .map(|t| t.centre)
                .filter(|c| {
                    let d2 = (c.0 - from.0).pow(2) + (c.1 - from.1).pow(2);
                    (30 * 30..160 * 160).contains(&d2)
                })
                .min_by_key(|c| (c.0 - from.0).pow(2) + (c.1 - from.1).pow(2))
            else {
                continue;
            };
            let distance = (((to.0 - from.0).pow(2) + (to.1 - from.1).pow(2)) as f32).sqrt();
            let estimate = (distance * 1.5) as u32 * company::RAIL_PIECE + 8 * company::STATION_TILE + order.price();
            if self.facts.vehicles >= self.company.vehicle_limit()
                || !self.company.can_afford(self.company.build_price(estimate) * 2)
            {
                return false;
            }
            let before = self.rails.clone();
            let stations_before = self.stations.tile_count();
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
            let line = crate::scenario::build_line_between(&mut railway, from, to);
            let terraform = std::mem::take(&mut railway.terraform);
            let Some(line) = line else {
                continue;
            };
            let moved = crate::terrain::edit::apply(&mut self.world, &terraform);
            self.levelled_pending.extend(moved);
            let mut changed = Vec::new();
            for z in 0..self.rails.size_z as i32 {
                for x in 0..self.rails.size_x as i32 {
                    if self.rails.get(x, z) != before.get(x, z) {
                        changed.push((x, z));
                    }
                }
            }
            let station_tiles = self.stations.tile_count().saturating_sub(stations_before) as u32;
            let cost = changed.len() as u32 * company::RAIL_PIECE + station_tiles * company::STATION_TILE;
            self.company
                .spend(Item::Construction, cost.min(self.company.cash.max(0) as u32));
            if let Ok(id) = self.trains.spawn(&self.rails, line.start, line.forward, order.cars()) {
                self.company.spend(Item::Vehicles, order.price());
                let t = self.trains.train(id);
                t.cargo = order.cargo;
                t.max_speed = order.speed();
                t.price = order.price();
                t.running = order.running();
                t.loco = order.loco;
                self.trains.set_orders(id, vec![line.to, line.from], 0);
            }
            self.mark_infra(&changed);
            self.track_version += 1;
            self.plan_epoch += 1;
            self.props_changed = true;
            self.static_changed = true;
            let name = |c: (i32, i32)| {
                self.towns
                    .list
                    .iter()
                    .find(|t| t.centre == c)
                    .map(|t| t.name.clone())
                    .unwrap_or_default()
            };
            let now = self.clock.days();
            let text = format!(
                "{}: connected {} to {} by rail ({} of track)",
                company::assistant_name(self.year).split(" (").next().unwrap_or("Agent"),
                name(from),
                name(to),
                changed.len()
            );
            self.show_message(&text, now + 6.0);
            return true;
        }
        false
    }

    /// Clones the train whose line has the most cargo waiting. False if nothing needs it or can be done.
    fn agent_adds_a_train(&mut self) -> bool {
        // The line with the most waiting cargo for its capacity, at least half a train's worth at each end.
        let mut best: Option<(f32, u32)> = None;
        for t in &self.trains.list {
            if t.orders.len() != 2 || t.capacity() == 0 {
                continue;
            }
            let waiting: Vec<u32> = t
                .orders
                .iter()
                .filter_map(|sid| self.stations.list.get(sid))
                .map(|s| s.stock.get(t.cargo))
                .collect();
            if waiting.len() != 2 {
                continue;
            }
            let cap = t.capacity() as f32;
            let score = waiting.iter().sum::<u32>() as f32 / cap;
            if waiting.iter().all(|&w| w as f32 >= cap * 0.5) && best.is_none_or(|(s, _)| score > s) {
                best = Some((score, t.id));
            }
        }
        let Some((_, id)) = best else { return false };
        let Some(model) = self.trains.list.iter().find(|t| t.id == id) else {
            return false;
        };
        let (orders, cars, cargo, speed, price, running, loco) = (
            model.orders.clone(),
            model.cars,
            model.cargo,
            model.max_speed,
            model.price,
            model.running,
            model.loco,
        );
        if self.facts.vehicles >= self.company.vehicle_limit() || !self.company.can_afford(price * 2) {
            return false;
        }
        // Somewhere to put it: a platform tile with track, facing either way.
        let tiles: Vec<(i32, i32)> = orders
            .iter()
            .filter_map(|sid: &StationId| self.stations.list.get(sid))
            .flat_map(|s| s.tiles.clone())
            .collect();
        for tile in tiles {
            for forward in [true, false] {
                match self.trains.spawn(&self.rails, tile, forward, cars) {
                    Ok(new) => {
                        self.company.spend(Item::Vehicles, price);
                        let t = self.trains.train(new);
                        t.cargo = cargo;
                        t.max_speed = speed;
                        t.price = price;
                        t.running = running;
                        t.loco = loco;
                        self.trains.set_orders(new, orders.clone(), 0);
                        self.props_changed = true;
                        let names: Vec<_> = orders
                            .iter()
                            .filter_map(|sid| self.stations.list.get(sid))
                            .map(|s| s.name.clone())
                            .collect();
                        let now = self.clock.days();
                        let text = format!(
                            "{}: added a train for {} ({}) to the {} line",
                            company::assistant_name(self.year).split(" (").next().unwrap_or("Agent"),
                            cargo.name().to_lowercase(),
                            super::company_ui::money(price as i64),
                            names.join(" - ")
                        );
                        self.show_message(&text, now + 5.0);
                        return true;
                    }
                    Err(SpawnError::NoTrack | SpawnError::Junction | SpawnError::TooShort | SpawnError::Occupied) => {}
                }
            }
        }
        false
    }
}
