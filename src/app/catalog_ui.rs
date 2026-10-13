//! The Buy Train window: choose a locomotive, what the wagons carry and how many, and see what it
//! will cost, how fast it goes and what it holds (GAME-DESIGN.md §7.5).

use super::StarcityApp;
use crate::cargo::Cargo;
use crate::catalog::{self, LOCOS};
use crate::ui::win95;
use eframe::egui;

impl StarcityApp {
    /// Shown while the Place train tool is active; click plain track to put the train down.
    /// The Buy a boat / Buy a plane window, while the Ferry or Plane tool is active.
    pub(super) fn craft_buy_window(&mut self, ctx: &egui::Context) {
        let (list, mut chosen, title): (&[crate::catalog::Craft], usize, &str) = match self.ui.tool {
            crate::ui::Tool::Boat => (&crate::catalog::BOATS, self.buy_boat, "Buy a boat"),
            crate::ui::Tool::Plane => (&crate::catalog::PLANES, self.buy_plane, "Buy a plane"),
            _ => return,
        };
        let year = self.tech_year();
        if !list[chosen].available(year) {
            chosen = crate::catalog::default_craft(list, year);
        }
        let money = super::company_ui::money;
        let mut open = true;
        win95::window(ctx, title, &mut open, egui::pos2(300.0, 130.0), |ui| {
            ui.set_width(330.0);
            for (i, c) in list.iter().enumerate() {
                if year < c.from {
                    ui.add_enabled(false, egui::Label::new(format!("{}: coming around {}", c.name, c.from)));
                    continue;
                }
                if !c.available(year) {
                    continue;
                }
                let text = format!(
                    "{}  {}  {} seats  {:.1} tiles/s  {} a year",
                    c.name,
                    money(c.price as i64),
                    c.capacity,
                    c.speed,
                    money(c.running as i64)
                );
                if ui.selectable_label(chosen == i, text).clicked() {
                    chosen = i;
                }
            }
            ui.label(egui::RichText::new("Then click water (boats) or an airport (planes) to place it.").small());
        });
        match self.ui.tool {
            crate::ui::Tool::Boat => self.buy_boat = chosen,
            _ => self.buy_plane = chosen,
        }
        if !open {
            self.ui.tool = crate::ui::Tool::Pointer;
        }
    }

    pub(super) fn train_buy_window(&mut self, ctx: &egui::Context) {
        if self.ui.tool != crate::ui::Tool::Train {
            return;
        }
        let year = self.tech_year();
        self.buy.fix(year);
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
