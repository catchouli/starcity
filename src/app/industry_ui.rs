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

    /// When the year turns, the industries it brings in are founded: data centres from 2005, AI campuses
    /// from 2020 (GAME-DESIGN.md §5.8).
    fn found_industries(&mut self) {
        if self.founded_year == self.year {
            return;
        }
        self.founded_year = self.year;
        let before = self.industries.list.len();
        let moved = self.industries.found(
            &mut self.world,
            &mut self.towns,
            &self.roads,
            &self.rails,
            self.params.seed,
            self.year,
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
            .earn(crate::company::Item::Hosting, (batches * crate::compute::hosting_per_batch(self.year) as f64) as u32);
        if let Some(tier) = self.compute.month(cu, crate::company::MONTH_SECONDS as f64) {
            let name = crate::compute::TIERS[tier - 1].0;
            let now = self.clock.days();
            self.show_message(&format!("Compute milestone: {name} ({})", crate::compute::show(self.compute.total)), now);
        }
    }

    /// Hands delivered cargo to the industries a station serves, filling them one after another.
    pub(super) fn deliver_to_industries(&mut self, tiles: &[(i32, i32)], cargo: Cargo, mut units: u32) {
        for id in self.industries.near(tiles) {
            if units == 0 {
                break;
            }
            units -= self.industries.deliver(id, cargo, units);
        }
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
                        .color(egui::Color32::from_rgb(0x90, 0x30, 0x10)),
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
pub(super) fn room_for(industries: &crate::industry::Industries, tiles: &[(i32, i32)], cargo: Cargo) -> u32 {
    if matches!(cargo, Cargo::Passengers | Cargo::Mail) {
        return u32::MAX;
    }
    industries
        .near(tiles)
        .into_iter()
        .map(|id| industries.list[id].room(cargo))
        .sum()
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
                "Makes {} up to {} a month",
                list.first().map_or("", |_| kind.outputs()[0].0.name()),
                kind.batches() * kind.outputs()[0].1
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
