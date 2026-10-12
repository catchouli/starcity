//! The company inside the app: paying for things, income from passengers,
//! the headquarters, and the finance, headquarters and next-step windows
//! (GAME-DESIGN.md §2, §9.1, §9.2).

use super::StarcityApp;
use crate::company::{self, Assets, Company, Facts, Item};
use crate::hq;
use crate::ui::win95;
use eframe::egui;
use glam::Vec3;

/// 12345 -> "12,345"; negative numbers keep their sign.
pub fn money(n: i64) -> String {
    let digits = n.unsigned_abs().to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    format!("{}£{out}", if n < 0 { "-" } else { "" })
}

impl StarcityApp {
    /// A fresh company with its garage in the company town. Keeps the sandbox setting.
    pub(super) fn new_company(&mut self) {
        let sandbox = self.company.sandbox;
        self.company = Company::new(self.year);
        self.company.sandbox = sandbox;
        self.player_roads = 0;
        self.hq = hq::choose_town(&self.towns)
            .and_then(|town| hq::place(&self.world, &mut self.towns, &self.roads, &self.rails, town));
        self.static_changed = true;
        self.refresh_assets();
    }

    /// Points the camera at the headquarters.
    pub(super) fn look_at_hq(&mut self) {
        if let Some(hq) = self.hq {
            let (x, z) = hq.centre();
            self.camera.focus = Vec3::new(x, self.world.height_at(x, z), z);
            self.scene_dirty = true;
        }
    }

    /// Counts what the company owns (cheap; once a second).
    pub(super) fn refresh_assets(&mut self) {
        self.assets = Assets {
            trains: self.trains.list.len() as u32,
            ferries: self.boats.list.len() as u32,
            planes: self.planes.list.len() as u32,
            road_pieces: self.player_roads,
            rail_pieces: if self.assets_version == self.track_version {
                self.assets.rail_pieces
            } else {
                self.rails.count()
            },
            station_tiles: self.stations.tile_count() as u32,
            ports: self.terminals.of_kind(crate::terminals::Kind::Port).count() as u32,
            airports: self.terminals.of_kind(crate::terminals::Kind::Airport).count() as u32,
        };
        self.assets_version = self.track_version;
        let town_served = self
            .stations
            .list
            .values()
            .map(|s| self.towns.population_near(&s.tiles))
            .chain(
                self.terminals
                    .list
                    .values()
                    .map(|t| self.towns.population_near(&t.tiles())),
            )
            .max()
            .unwrap_or(0);
        self.facts = Facts {
            rail_pieces: self.assets.rail_pieces,
            stations: self.stations.list.len() as u32,
            trains: self.assets.trains,
            town_served,
            vehicles: self.assets.trains + self.assets.ferries + self.assets.planes,
        };
    }

    /// Charges for something; says so if the money isn't there.
    pub(super) fn pay(&mut self, item: Item, cost: u32) -> bool {
        if self.company.spend(item, cost) {
            return true;
        }
        let now = self.last_time;
        self.show_message(&format!("Not enough money: {} needed", money(cost as i64)), now);
        false
    }

    /// Whether another vehicle fits under the headquarters' limit and the
    /// money is there. Says why not.
    pub(super) fn can_add_vehicle(&mut self, price: u32) -> bool {
        let now = self.last_time;
        if self.facts.vehicles >= self.company.vehicle_limit() {
            let limit = self.company.vehicle_limit();
            self.show_message(
                &format!("Your headquarters only has room for {limit} vehicles. Upgrade it to run more."),
                now,
            );
            return false;
        }
        if !self.company.can_afford(price) {
            self.show_message(&format!("Not enough money: {} needed", money(price as i64)), now);
            return false;
        }
        true
    }

    /// Passengers got off: pay the fare for the trip from where they boarded.
    pub(super) fn pay_fare(&mut self, passengers: u32, boarded: Option<(f32, f32, f64)>, at: (f32, f32)) {
        let Some((x, z, since)) = boarded else { return };
        if passengers == 0 {
            return;
        }
        let tiles = ((at.0 - x).powi(2) + (at.1 - z).powi(2)).sqrt();
        let seconds = (self.clock.days() - since).max(0.0) as f32;
        self.company
            .earn(Item::Passengers, company::passenger_fare(passengers, tiles, seconds));
    }

    /// Once per sim step: the monthly books.
    pub(super) fn tick_company(&mut self, dt: f32) {
        let messages = self.company.tick(dt, self.year, &self.assets);
        let now = self.last_time;
        for m in messages {
            self.show_message(&m, now);
        }
    }

    pub(super) fn finance_window(&mut self, ctx: &egui::Context) {
        let mut open = self.ui.show_finance;
        let value = self.company.value(&self.assets);
        let (mut borrow, mut repay) = (false, false);
        let c = &self.company;
        win95::window(ctx, "Finances", &mut open, egui::pos2(200.0, 110.0), |ui| {
            ui.set_width(380.0);
            egui::Grid::new("balance").num_columns(2).show(ui, |ui| {
                ui.label("Cash");
                ui.label(egui::RichText::new(money(c.cash)).strong());
                ui.end_row();
                ui.label("Loan");
                ui.label(money(c.loan));
                ui.end_row();
                ui.label("Company value");
                ui.label(money(value));
                ui.end_row();
            });
            ui.horizontal(|ui| {
                let limit = c.loan_limit();
                borrow = ui
                    .add_enabled(
                        c.loan + company::LOAN_STEP <= limit,
                        egui::Button::new("Borrow £10,000"),
                    )
                    .clicked();
                repay = ui
                    .add_enabled(c.loan >= company::LOAN_STEP, egui::Button::new("Repay £10,000"))
                    .clicked();
                ui.label(if limit == 0 {
                    "Build a Depot office to open a loan".to_string()
                } else {
                    format!("up to {}", money(limit))
                });
            });
            ui.separator();
            // This year and the three before, newest first.
            let mut years: Vec<(i32, company::Ledger)> = vec![(c.year, c.ledger)];
            years.extend(c.history.iter().copied());
            egui::Grid::new("books")
                .num_columns(years.len() + 1)
                .striped(true)
                .show(ui, |ui| {
                    ui.label("");
                    for (y, _) in &years {
                        ui.label(egui::RichText::new(y.to_string()).strong());
                    }
                    ui.end_row();
                    for item in Item::ALL {
                        ui.label(item.name());
                        for (_, l) in &years {
                            ui.label(money(l.get(item)));
                        }
                        ui.end_row();
                    }
                    ui.label(egui::RichText::new("Profit").strong());
                    for (_, l) in &years {
                        ui.label(egui::RichText::new(money(l.income() - l.expenses())).strong());
                    }
                    ui.end_row();
                });
        });
        self.ui.show_finance = open;
        if borrow {
            self.company.borrow();
        }
        if repay {
            self.company.repay();
        }
    }

    pub(super) fn hq_window(&mut self, ctx: &egui::Context) {
        let Some(hq) = self.hq else {
            self.ui.show_hq = false;
            return;
        };
        let mut open = self.ui.show_hq;
        let level = self.company.hq_level;
        let reqs = self.company.requirements(&self.assets, &self.facts);
        let can_upgrade = self.company.can_upgrade(&self.assets, &self.facts);
        let town = self
            .towns
            .list
            .get(hq.town as usize)
            .map(|t| t.name.clone())
            .unwrap_or_default();
        let (mut upgrade, mut look) = (false, false);
        win95::window(ctx, "Headquarters", &mut open, egui::pos2(220.0, 130.0), |ui| {
            ui.set_width(330.0);
            let current = company::level(level);
            ui.label(egui::RichText::new(format!("{} — {town}", current.name)).strong());
            ui.label(format!("Level {level}. Unlocks: {}.", current.unlocks.to_lowercase()));
            ui.label(format!(
                "Room for {} vehicles; loan up to {}.",
                self.company.vehicle_limit(),
                money(self.company.loan_limit())
            ));
            ui.separator();
            match company::LEVELS.get(level as usize + 1) {
                Some(next) => {
                    ui.label(egui::RichText::new(format!("Next: {}", next.name)).strong());
                    ui.label(format!("Unlocks: {}.", next.unlocks.to_lowercase()));
                    for r in &reqs {
                        ui.label(format!("{} {}", if r.met { "[x]" } else { "[ ]" }, r.text));
                    }
                    upgrade = ui
                        .add_enabled(
                            can_upgrade,
                            egui::Button::new(format!("Build {} ({})", next.name, money(next.cost as i64))),
                        )
                        .clicked();
                }
                None => {
                    ui.label(company::LATER_LEVELS);
                }
            }
            ui.add_space(4.0);
            look = ui.button("Look at it").clicked();
        });
        self.ui.show_hq = open;
        if look {
            self.look_at_hq();
        }
        if upgrade && self.company.upgrade(&self.assets, &self.facts) {
            self.static_changed = true;
            let now = self.last_time;
            let name = company::level(self.company.hq_level).name;
            self.show_message(&format!("Your headquarters is now a {name}"), now);
        }
    }

    /// The panel that always has one thing for the player to do.
    pub(super) fn next_step_window(&mut self, ctx: &egui::Context) {
        let step = self.company.next_step(&self.assets, &self.facts);
        let hint = self.company.stuck_hint(&self.facts);
        let mut look = false;
        win95::window(
            ctx,
            "Next step",
            &mut self.ui.show_next_step,
            egui::pos2(12.0, 98.0),
            |ui| {
                ui.set_width(250.0);
                ui.label(egui::RichText::new(&step.title).strong());
                ui.label(&step.detail);
                if let Some(h) = hint {
                    ui.add_space(2.0);
                    ui.label(egui::RichText::new(h).color(egui::Color32::from_rgb(0x90, 0x30, 0x10)));
                }
                if step.hq {
                    look = ui.button("Show my headquarters").clicked();
                }
            },
        );
        if look {
            self.look_at_hq();
            self.ui.show_hq = true;
        }
    }

    /// Returns true when the player asks to start again.
    pub(super) fn bankrupt_window(&mut self, ctx: &egui::Context) -> bool {
        if !self.company.bankrupt {
            return false;
        }
        let mut open = true;
        let mut restart = false;
        win95::window(ctx, "Bankrupt", &mut open, egui::pos2(260.0, 160.0), |ui| {
            ui.set_width(280.0);
            ui.label("Your company has been in the red for a year and the bank has closed it down.");
            ui.label("Sell what you can next time, and keep an eye on the finance window.");
            restart = ui.button("Start a new company").clicked();
        });
        restart
    }
}
