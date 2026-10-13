//! Industries in the app: the monthly production step that feeds stations, the room
//! they have for deliveries, and the industry window (GAME-DESIGN.md §5).

use super::StarcityApp;
use crate::cargo::Cargo;
use crate::exchange;
use crate::industry::{CATCHMENT, Industry};
use crate::ui::win95;
use eframe::egui;

impl StarcityApp {
    /// Once an economy month: industries make their cargo and the stations that serve them collect it.
    pub(super) fn industry_month(&mut self) {
        self.found_industries();
        let ratings: Vec<f32> = self
            .industries
            .list
            .iter()
            .map(|ind| {
                let cargo = ind.kind.outputs().first().map(|&(c, _)| c);
                cargo.map_or(0.0, |c| {
                    self.stations
                        .list
                        .values()
                        .filter(|s| ind.near(&s.tiles, CATCHMENT))
                        .map(|s| s.service[c.index()].rating)
                        .fold(0.0, f32::max)
                })
            })
            .collect();
        let events = self.industries.drift(&ratings);
        self.industry_events(events);
        self.power_towns();
        let made = self.industries.month();
        self.compute_month();
        let mut moved = vec![0u32; self.industries.list.len()];
        for (id, cargo, units) in made {
            let ind = &self.industries.list[id];
            let mut serving: Vec<&mut crate::stations::Station> = self
                .stations
                .list
                .values_mut()
                .filter(|s| ind.near(&s.tiles, CATCHMENT))
                .collect();
            moved[id] += exchange::collect(&mut serving, cargo, units);
        }
        for (ind, m) in self.industries.list.iter_mut().zip(moved) {
            ind.moved_last_month = m;
        }
    }

    /// Industries that announce they are closing, or have shut: say so, free the ground, and found a
    /// replacement somewhere else so there is always a new place to build (GAME-DESIGN.md §5.2).
    fn industry_events(&mut self, events: Vec<crate::industry::Event>) {
        use crate::industry::Event;
        for e in events {
            let now = self.clock.days();
            match e {
                Event::Closing(id) => {
                    let ind = &self.industries.list[id];
                    let town = self
                        .towns
                        .nearest((ind.centre().0 as i32, ind.centre().1 as i32), 80)
                        .map(|t| format!(" near {}", self.towns.list[t as usize].name))
                        .unwrap_or_default();
                    let text = format!("{}{town} is closing: nothing serves it any more", ind.kind.name());
                    self.show_message(&text, now + 5.0);
                }
                Event::Closed(id) => {
                    let kind = self.industries.list[id].kind;
                    for t in self.industries.release(id) {
                        self.towns.occupied.set(t.0, t.1, 0);
                    }
                    if self.selected_industry == Some(id) {
                        self.selected_industry = None;
                    }
                    let moved = self.industries.found_plan(
                        &mut self.world,
                        &mut self.towns,
                        &self.roads,
                        &self.rails,
                        self.params.seed ^ id as u32,
                        vec![(kind, 1)],
                    );
                    self.levelled_pending.extend(moved);
                    self.static_changed = true;
                    self.show_message(&format!("A {} has shut. A new one opens elsewhere.", kind.name().to_lowercase()), now + 5.0);
                }
            }
        }
    }

    /// Towns with a working power station within 40 tiles grow a little faster (GAME-DESIGN.md §5.5).
    fn power_towns(&mut self) {
        let stations: Vec<(f32, f32)> = self
            .industries
            .list
            .iter()
            .filter(|i| !i.closed && i.kind == crate::industry::Kind::PowerStation && i.made_last_month > 0)
            .map(|i| i.centre())
            .collect();
        for t in &mut self.towns.list {
            t.powered = stations
                .iter()
                .any(|s| ((s.0 - t.centre.0 as f32).powi(2) + (s.1 - t.centre.1 as f32).powi(2)).sqrt() <= 40.0);
        }
    }

    /// Pays the council to found an industry on the best site the map has.
    pub(super) fn fund_industry(&mut self, kind: crate::industry::Kind) {
        let now = self.clock.days();
        if !self.company.sandbox && self.company.hq_level < 2 {
            self.show_message("Funding industries needs the Regional office", now);
            return;
        }
        let cost = kind.fund_cost();
        if !self.company.can_afford(self.company.build_price(cost)) {
            self.show_message(&format!("Not enough money: {} needed", super::company_ui::money(cost as i64)), now);
            return;
        }
        let before = self.industries.list.len();
        let moved = self.industries.found_plan(
            &mut self.world,
            &mut self.towns,
            &self.roads,
            &self.rails,
            self.params.seed ^ (before as u32).wrapping_mul(2654435761),
            vec![(kind, 1)],
        );
        if self.industries.list.len() == before {
            self.show_message("The council can't find a suitable site for it", now);
            return;
        }
        self.company.spend(crate::company::Item::Construction, cost);
        self.levelled_pending.extend(moved);
        self.static_changed = true;
        self.selected_industry = Some(before);
        let (x, z) = self.industries.list[before].centre();
        self.camera.focus = glam::Vec3::new(x, self.world.height_at(x, z), z);
        self.scene_dirty = true;
        self.show_message(&format!("The council has founded a {}", kind.name().to_lowercase()), now + 4.0);
    }

    /// The Fund an industry window: what can be founded this year, and for how much.
    pub(super) fn fund_window(&mut self, ctx: &egui::Context) {
        let mut open = self.ui.show_fund;
        let year = self.tech_year();
        let cash = self.company.cash;
        let (sandbox, level) = (self.company.sandbox, self.company.hq_level);
        let mut chosen = None;
        win95::window(ctx, "Fund an industry", &mut open, egui::pos2(300.0, 120.0), |ui| {
            ui.set_width(300.0);
            ui.label("The council founds it on the best site the map has. You then connect it.");
            if !sandbox && level < 2 {
                ui.label(egui::RichText::new("Needs the Regional office.").color(win95::warn()));
            }
            ui.separator();
            for kind in crate::industry::Kind::ALL.into_iter().filter(|k| k.available(year)) {
                let cost = kind.fund_cost();
                ui.horizontal(|ui| {
                    ui.label(kind.name());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let label = super::company_ui::money(cost as i64);
                        if ui.add_enabled(cash >= cost as i64, egui::Button::new(label)).clicked() {
                            chosen = Some(kind);
                        }
                    });
                });
            }
        });
        self.ui.show_fund = open;
        if let Some(kind) = chosen {
            self.fund_industry(kind);
        }
    }

    /// When the year turns, the industries it brings in are founded: data centres from 2005, AI campuses
    /// from 2020 (GAME-DESIGN.md §5.8).
    fn found_industries(&mut self) {
        let year = self.tech_year();
        if self.founded_year == year {
            return;
        }
        self.founded_year = year;
        let before = self.industries.list.len();
        let moved = self.industries.found(
            &mut self.world,
            &mut self.towns,
            &self.roads,
            &self.rails,
            self.params.seed,
            year,
        );
        if self.industries.list.len() > before {
            self.levelled_pending.extend(moved);
            self.static_changed = true;
            let names: Vec<_> = self.industries.list[before..]
                .iter()
                .map(|i| i.kind.name())
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect();
            let now = self.clock.days();
            self.show_message(&format!("New industry: {}", names.join(", ")), now);
        }
    }

    /// The server farms' month: batches of work become compute, which is also sold as hosting.
    fn compute_month(&mut self) {
        let efficiency = crate::compute::efficiency(self.year as f64 + 0.5);
        let (mut cu, mut batches) = (0.0, 0.0f64);
        for ind in &self.industries.list {
            let scale = ind.kind.compute_scale();
            if scale > 0.0 {
                cu += ind.made_last_month as f64 * crate::compute::BATCH_CU * scale * efficiency;
                batches += ind.made_last_month as f64 * scale;
            }
        }
        self.company
            .earn(crate::company::Item::Hosting, (batches * self.compute_alloc.hosting() as f64 * crate::compute::hosting_per_batch(self.year) as f64) as u32);
        let tier_reached = self.compute.month(cu, crate::company::MONTH_SECONDS as f64);
        self.refresh_effects();
        if let Some(tier) = tier_reached {
            let name = crate::compute::TIERS[tier - 1].0;
            let now = self.clock.days();
            self.show_message(&format!("Compute milestone: {name} ({})", crate::compute::show(self.compute.total)), now);
        }
    }

    /// Hands delivered cargo to the industries a station serves, filling them one after another.
    /// Returns what was left over, which a town may take.
    pub(super) fn deliver_to_industries(&mut self, tiles: &[(i32, i32)], cargo: Cargo, mut units: u32) -> u32 {
        for id in self.industries.near(tiles) {
            if units == 0 {
                break;
            }
            units -= self.industries.deliver(id, cargo, units);
        }
        units
    }

    pub(super) fn industry_window(&mut self, ctx: &egui::Context) {
        let Some(id) = self.selected_industry else { return };
        let Some(ind) = self.industries.get(id).cloned() else {
            self.selected_industry = None;
            return;
        };
        let serving: Vec<String> = self
            .stations
            .list
            .values()
            .filter(|s| ind.near(&s.tiles, CATCHMENT))
            .map(|s| s.name.clone())
            .collect();
        let town = self
            .towns
            .nearest((ind.centre().0 as i32, ind.centre().1 as i32), 60)
            .map(|t| self.towns.list[t as usize].name.clone());
        let mut open = true;
        win95::window(
            ctx,
            &ind.kind.name().to_string(),
            &mut open,
            egui::pos2(420.0, 110.0),
            |ui| {
                ui.set_width(260.0);
                if let Some(town) = &town {
                    ui.label(format!("Near {town}"));
                }
                describe(ui, &ind);
                ui.separator();
                if serving.is_empty() {
                    ui.label(
                        egui::RichText::new(
                            "No station serves it yet: build one within three tiles to collect or deliver cargo.",
                        )
                        .color(win95::warn()),
                    );
                } else {
                    ui.label(format!("Served by {}", serving.join(", ")));
                }
            },
        );
        if !open {
            self.selected_industry = None;
        }
    }
}

/// How many units of `cargo` the industries by a station would take (people and mail: any number).
pub(super) fn room_for(
    industries: &crate::industry::Industries,
    towns: &crate::towns::Towns,
    tiles: &[(i32, i32)],
    cargo: Cargo,
) -> u32 {
    if matches!(cargo, Cargo::Passengers | Cargo::Mail) {
        return u32::MAX;
    }
    // Towns take food, goods and fuel without limit (GAME-DESIGN.md §5.5).
    if is_for_towns(cargo) && towns.population_near(tiles) > 0 {
        return u32::MAX;
    }
    industries
        .near(tiles)
        .into_iter()
        .map(|id| industries.list[id].room(cargo))
        .fold(0u32, u32::saturating_add)
}

/// What towns buy.
pub(super) fn is_for_towns(cargo: Cargo) -> bool {
    matches!(cargo, Cargo::Food | Cargo::Goods | Cargo::Fuel)
}

fn describe(ui: &mut egui::Ui, ind: &Industry) {
    let kind = ind.kind;
    if kind.compute_scale() > 0.0 {
        ui.label(format!("Turns {} into compute: the score. It also sells spare compute as hosting.", inputs_text(ind)));
        ui.label(format!(
            "Last month: {} batches, {}",
            ind.made_last_month,
            crate::compute::show(ind.made_last_month as f64 * crate::compute::BATCH_CU)
        ));
    } else if !kind.outputs().is_empty() {
        let list: Vec<String> = kind
            .outputs()
            .iter()
            .map(|&(c, n)| format!("{} {}", n, c.name()))
            .collect();
        if kind.is_primary() {
            ui.label(format!(
                "Makes {} up to {} a month ({:.0}% of what it once did)",
                list.first().map_or("", |_| kind.outputs()[0].0.name()),
                (kind.batches() as f32 * ind.production).round() as u32 * kind.outputs()[0].1,
                ind.production * 100.0
            ));
        } else {
            ui.label(format!("Turns {} into {}", inputs_text(ind), list.join(" and ")));
        }
    } else {
        ui.label("Burns coal to make electricity for industry within 30 tiles");
    }
    for &(cargo, _) in kind.inputs() {
        ui.label(format!(
            "{}: {} / {} {} waiting",
            cargo.name(),
            ind.stock[cargo.index()],
            crate::industry::STOCK_CAP,
            cargo.unit()
        ));
    }
    if kind.power_mw() > 0.0 {
        ui.label(match ind.power {
            p if p >= 0.99 => format!("Power: {} MW, fully supplied", kind.power_mw()),
            p if p > 0.0 => format!(
                "Power: {:.0}% supplied: a power station nearby is short of coal",
                p * 100.0
            ),
            _ => "Power: none in reach. Running on its generator (30% speed). Build a power station within 30 tiles."
                .to_string(),
        });
    }
    if kind.compute_scale() == 0.0 {
        ui.label(format!(
            "Last month: made {}, collected by stations {}",
            ind.made_last_month, ind.moved_last_month
        ));
    }
}

fn inputs_text(ind: &Industry) -> String {
    ind.kind
        .inputs()
        .iter()
        .map(|&(c, n)| format!("{} {}", n, c.name()))
        .collect::<Vec<_>>()
        .join(" + ")
}
