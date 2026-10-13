//! The Buy Train window: choose a locomotive, what the wagons carry and how many, and see what it
//! will cost, how fast it goes and what it holds (GAME-DESIGN.md §7.5).

use super::StarcityApp;
use crate::cargo::Cargo;
use crate::catalog::{self, LOCOS};
use crate::ui::win95;
use eframe::egui;

impl StarcityApp {
    /// Shown while the Place train tool is active; click plain track to put the train down.
    pub(super) fn train_buy_window(&mut self, ctx: &egui::Context) {
        if self.ui.tool != crate::ui::Tool::Train {
            return;
        }
        self.buy.fix(self.year);
        let year = self.year;
        let mut order = self.buy;
        let mut open = true;
        let money = super::company_ui::money;
        win95::window(ctx, "Buy a train", &mut open, egui::pos2(300.0, 130.0), |ui| {
            ui.set_width(330.0);
            ui.label(egui::RichText::new("Locomotive").strong());
            for (i, loco) in LOCOS.iter().enumerate() {
                if let Some(when) = loco.coming(year) {
                    ui.add_enabled(false, egui::Label::new(format!("{}: coming around {when}", loco.name)));
                    continue;
                }
                if !loco.available(year) {
                    continue;
                }
                let text = format!(
                    "{}  {}  {:.1} tiles/s  pulls {}",
                    loco.name,
                    money(loco.price as i64),
                    loco.speed,
                    loco.max_wagons
                );
                if ui
                    .selectable_label(order.loco == i, text)
                    .on_hover_text(loco.blurb)
                    .clicked()
                {
                    order.loco = i;
                }
            }
            ui.separator();
            ui.horizontal(|ui| {
                ui.label("Carries");
                egui::ComboBox::from_id_salt("buy cargo")
                    .selected_text(order.cargo.name())
                    .show_ui(ui, |ui| {
                        for c in Cargo::ALL {
                            ui.selectable_value(&mut order.cargo, c, c.name());
                        }
                    });
            });
            ui.horizontal(|ui| {
                ui.label(format!("{}s", catalog::wagon_name(order.cargo)));
                let most = order.loco().max_wagons;
                ui.add(egui::Slider::new(&mut order.wagons, 1..=most));
            });
            ui.separator();
            ui.label(egui::RichText::new(format!("Price {}", money(order.price() as i64))).strong());
            ui.label(format!(
                "Holds {} {} · {:.1} tiles/s · {} a year to run",
                order.capacity(),
                order.cargo.unit(),
                order.speed(),
                money(order.running() as i64)
            ));
            ui.weak("Click plain track to place it, then click the train to give it orders.");
        });
        order.fix(year);
        self.buy = order;
        if !open {
            self.ui.tool = crate::ui::Tool::Pointer;
        }
    }
}
