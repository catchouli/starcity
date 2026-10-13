//! The Compute window: what the company's compute is spent on (GAME-DESIGN.md §5.8).

use super::StarcityApp;
use crate::compute::{self, PROGRAMS};
use crate::ui::win95;
use eframe::egui;

impl StarcityApp {
    /// The compute made last month, in CU.
    fn monthly_cu(&self) -> f64 {
        self.compute.rate * crate::company::MONTH_SECONDS as f64
    }

    /// Re-works what the programs are doing from the allocation and last month's compute.
    pub(super) fn refresh_effects(&mut self) {
        self.company.effects = compute::effects(&self.compute_alloc, self.year, self.monthly_cu(), self.assets.trains);
    }

    pub(super) fn compute_window(&mut self, ctx: &egui::Context) {
        let mut open = self.ui.show_compute;
        let year = self.year;
        let fleet = self.assets.trains;
        let monthly = self.monthly_cu();
        let mut alloc = self.compute_alloc;
        let total = self.compute.total;
        let tier = self.compute.tier;
        win95::window(ctx, "Compute", &mut open, egui::pos2(260.0, 120.0), |ui| {
            ui.set_width(380.0);
            ui.label(egui::RichText::new(format!("{} made so far", compute::show(total))).strong());
            let tier_text = match compute::TIERS.get(tier) {
                Some((next, at)) => format!("Next milestone: {next} at {}", compute::show(*at)),
                None => "Every milestone reached".to_string(),
            };
            ui.label(tier_text);
            ui.label(format!("This month's compute: {}", compute::show(monthly)));
            ui.separator();
            ui.label("Share it out. Whatever you don't give a program is sold as hosting for money.");
            for p in PROGRAMS {
                ui.add_space(4.0);
                if year < p.from_year() {
                    ui.label(
                        egui::RichText::new(format!("{}: available from {}", p.name(), p.from_year()))
                            .color(win95::text().gamma_multiply(0.5)),
                    );
                    continue;
                }
                let mut pct = alloc.share[p.index()] * 100.0;
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(p.name()).strong());
                    if ui
                        .add(egui::Slider::new(&mut pct, 0.0..=100.0).suffix("%").integer())
                        .changed()
                    {
                        alloc.set(p, pct / 100.0);
                    }
                });
                let assigned = monthly * alloc.share[p.index()] as f64;
                let now = p.effect(year, assigned, fleet);
                let ceiling = p.strongest(year);
                let what = match p {
                    compute::Program::Analytics => "fares",
                    compute::Program::Optimiser => "running costs",
                    compute::Program::Design => "construction",
                };
                let sign = if p == compute::Program::Analytics { "+" } else { "-" };
                ui.label(format!(
                    "{} Now {}{:.1}% on {what}, at most {sign}{:.0}% this year. A company your size needs {} a month to run it fully.",
                    p.blurb(),
                    if now > 0.0 { sign } else { "" },
                    now * 100.0,
                    ceiling * 100.0,
                    compute::show(p.need(year, fleet)),
                ));
            }
            ui.separator();
            ui.label(format!("Hosting: {:.0}% of your compute", alloc.hosting() * 100.0));
        });
        self.compute_alloc = alloc;
        self.ui.show_compute = open;
        self.refresh_effects();
    }
}
