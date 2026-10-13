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
            if !self.agent_adds_a_train() {
                break;
            }
            self.agent_budget -= 1.0;
        }
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
        let Some(model) = self.trains.list.iter().find(|t| t.id == id) else { return false };
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
