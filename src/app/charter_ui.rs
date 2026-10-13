//! The governor's window (GAME-DESIGN.md §1.2): the charter the agents follow, the two ledgers, and
//! the ending at the close of 2050.

use super::StarcityApp;
use crate::charter::{self, Charter, PRIORITIES};
use crate::ui::win95;
use eframe::egui;

/// The year the charter opens: when agents arrive (§5.8).
pub const CHARTER_FROM: i32 = 2028;

impl StarcityApp {
    /// Whether the charter is in force: once the company's technology reaches the agents.
    pub(super) fn charter_active(&self) -> bool {
        self.tech_year() >= CHARTER_FROM
    }

    /// Once a month: the Wellbeing ledger, the charter's effects, and (at the end of 2050) the ending.
    pub(super) fn governor_month(&mut self) {
        let people = self.people();
        self.wellbeing = charter::wellbeing(&people);
        let active = self.charter_active();
        self.company.build_factor = if active { self.charter.build_cost_factor() } else { 1.0 };
        if !self.ending_shown && self.clock.year_f64() >= 2050.95 {
            self.ending_shown = true;
            self.ending = Some(charter::ending(&self.charter, self.wellbeing, self.compute.tier));
        }
    }

    /// What Wellbeing is measured from.
    fn people(&self) -> charter::People {
        let total: u32 = self.towns.list.iter().map(|t| t.pop).sum();
        let served: u32 = self
            .towns
            .list
            .iter()
            .filter(|t| {
                self.stations.list.values().any(|s| {
                    s.tiles
                        .iter()
                        .any(|&(x, z)| (x - t.centre.0).abs() <= 14 && (z - t.centre.1).abs() <= 14)
                })
            })
            .map(|t| t.pop)
            .sum();
        let wanting: Vec<_> = self
            .towns
            .list
            .iter()
            .filter(|t| t.pop > crate::towns::WANTS_FOOD)
            .collect();
        let supplied = wanting
            .iter()
            .filter(|t| t.food > 0.0 && (t.pop <= crate::towns::WANTS_GOODS || t.goods > 0.0))
            .count();
        let consumers: Vec<f32> = self
            .industries
            .list
            .iter()
            .filter(|i| !i.closed && i.kind.power_mw() > 0.0)
            .map(|i| i.power)
            .collect();
        let coal: u32 = self
            .industries
            .list
            .iter()
            .filter(|i| !i.closed && i.kind == crate::industry::Kind::PowerStation)
            .map(|i| i.made_last_month)
            .sum();
        charter::People {
            served: if total == 0 { 0.0 } else { served as f32 / total as f32 },
            supplied: if wanting.is_empty() { 1.0 } else { supplied as f32 / wanting.len() as f32 },
            power: if consumers.is_empty() { 1.0 } else { consumers.iter().sum::<f32>() / consumers.len() as f32 },
            clean: 1.0 / (1.0 + coal as f32 / 200.0),
        }
    }

    pub(super) fn charter_window(&mut self, ctx: &egui::Context) {
        let mut open = self.ui.show_charter;
        let active = self.charter_active();
        let mut c: Charter = self.charter;
        if !self.ui.show_charter {
            return;
        }
        // Measured live while the window is open.
        self.wellbeing = charter::wellbeing(&self.people());
        let (wellbeing, value) = (self.wellbeing, self.company.value(&self.assets));
        win95::window(ctx, "Charter", &mut open, egui::pos2(300.0, 120.0), |ui| {
            ui.set_width(360.0);
            ui.label(egui::RichText::new("The two ledgers").strong());
            ui.label(format!("Money: company value {}", super::company_ui::money(value)));
            ui.label(format!("Wellbeing: {wellbeing:.0} / 100 (towns served, shops stocked, power, clean air)"));
            ui.separator();
            if !active {
                ui.label(
                    egui::RichText::new(format!(
                        "The charter opens when agents arrive, around {CHARTER_FROM}. Until then you run things by hand."
                    ))
                    .color(win95::warn()),
                );
            }
            ui.label("What the company's agents should value. The four always add up to 100%.");
            ui.add_enabled_ui(active, |ui| {
                for (i, (name, what)) in PRIORITIES.iter().enumerate() {
                    let mut pct = c.weights[i] * 100.0;
                    ui.horizontal(|ui| {
                        ui.add_sized([56.0, 18.0], egui::Label::new(egui::RichText::new(*name).strong()));
                        if ui.add(egui::Slider::new(&mut pct, 0.0..=100.0).suffix("%").integer()).changed() {
                            c.set(i, pct / 100.0);
                        }
                    });
                    ui.label(egui::RichText::new(*what).small());
                }
            });
            ui.separator();
            ui.label(
                egui::RichText::new(
                    "At the end of 2050 the singularity reads the two ledgers and the charter. What you built is how it ends.",
                )
                .small(),
            );
        });
        self.charter = c;
        self.ui.show_charter = open;
    }

    /// The end of the fifty years.
    pub(super) fn ending_window(&mut self, ctx: &egui::Context) {
        let Some(ending) = self.ending else { return };
        let (mut open, mut ok) = (true, false);
        let (wellbeing, value, total) = (self.wellbeing, self.company.value(&self.assets), self.compute.total);
        win95::window(ctx, "2050", &mut open, egui::pos2(320.0, 140.0), |ui| {
            ui.set_width(380.0);
            ui.label(egui::RichText::new(ending.title()).heading().color(win95::ink()));
            ui.label(ending.text());
            ui.separator();
            ui.label(format!("Money: {}", super::company_ui::money(value)));
            ui.label(format!("Wellbeing: {wellbeing:.0} / 100"));
            ui.label(format!("Compute made: {}", crate::compute::show(total)));
            ui.add_space(4.0);
            ok = ui.button("Keep playing").clicked();
        });
        if !open || ok {
            self.ending = None;
        }
    }
}
