//! Headlines (GAME-DESIGN.md §10.9) arrive the way news did in each era: a web page in Internet
//! Explorer in the 2000s, a phone notification in the 2010s, a social feed card in the 2020s, and
//! a projected signal after that. Same events, different paper.

use super::StarcityApp;
use crate::events::{EVENTS, Event};
use crate::ui::win95;
use eframe::egui::{self, Color32};

/// How the news looks in a year.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NewsStyle {
    Browser,
    Phone,
    Feed,
    Signal,
}

pub fn news_style(year: i32) -> NewsStyle {
    match year {
        ..=2009 => NewsStyle::Browser,
        2010..=2019 => NewsStyle::Phone,
        2020..=2029 => NewsStyle::Feed,
        _ => NewsStyle::Signal,
    }
}

/// The news follows the interface: picking an older look brings its way of reading the news back too.
pub fn style_for(look: win95::Look) -> NewsStyle {
    news_style(look.from_year())
}

/// "Fuel price spike" -> "fuel-price-spike", for the address bar.
pub fn slug(title: &str) -> String {
    title
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

/// Made-up reaction counts that are always the same for the same story.
fn reactions(i: usize) -> (u32, u32) {
    let h = (i as u32 + 7).wrapping_mul(2654435761);
    (800 + h % 9000, 120 + (h >> 8) % 2400)
}

impl StarcityApp {
    /// The headline of the day, with what it does, until it is dismissed.
    pub(super) fn news_window(&mut self, ctx: &egui::Context) {
        let Some(&i) = self.news.first() else { return };
        let e = &EVENTS[i];
        let dismissed = match style_for(win95::look()) {
            NewsStyle::Browser => browser(ctx, e),
            NewsStyle::Phone => phone(ctx, e, &self.clock.date().date_string()),
            NewsStyle::Feed => feed(ctx, e, i),
            NewsStyle::Signal => signal(ctx, e),
        };
        if dismissed {
            self.news.remove(0);
        }
    }
}

/// 2000–2009: a page in Internet Explorer.
fn browser(ctx: &egui::Context, e: &Event) -> bool {
    let (mut open, mut ok) = (true, false);
    win95::window(ctx, "Microsoft Internet Explorer", &mut open, egui::pos2(330.0, 130.0), |ui| {
        ui.set_width(380.0);
        ui.horizontal(|ui| {
            for menu in ["File", "Edit", "View", "Favorites", "Tools", "Help"] {
                ui.label(menu);
            }
        });
        ui.horizontal(|ui| {
            for b in ["Back", "Forward", "Stop", "Refresh", "Home"] {
                ui.add_enabled(false, egui::Button::new(b).small());
            }
        });
        ui.horizontal(|ui| {
            ui.label("Address");
            let mut url = format!("http://www.starcity-news.com/headlines/{}", slug(e.title));
            ui.add(egui::TextEdit::singleline(&mut url).desired_width(f32::INFINITY));
        });
        egui::Frame::NONE
            .fill(Color32::WHITE)
            .stroke(egui::Stroke::new(1.0, Color32::from_gray(0x80)))
            .inner_margin(egui::Margin::same(8))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(
                    egui::RichText::new("STARCITY NEWS NETWORK")
                        .color(Color32::from_rgb(0x00, 0x33, 0x99))
                        .strong(),
                );
                ui.separator();
                ui.label(
                    egui::RichText::new(e.title)
                        .color(Color32::from_rgb(0x00, 0x00, 0xee))
                        .underline()
                        .size(20.0),
                );
                ui.label(egui::RichText::new(e.text).color(Color32::BLACK));
                ui.add_space(6.0);
                ui.label(egui::RichText::new("Best viewed in Internet Explorer 5 at 800x600").small().color(Color32::GRAY));
            });
        ui.horizontal(|ui| {
            ui.label("Done");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ok = ui.button("Close").clicked();
            });
        });
    });
    !open || ok
}

/// 2010–2019: a notification on a phone.
fn phone(ctx: &egui::Context, e: &Event, date: &str) -> bool {
    let mut dismiss = false;
    let screen = ctx.content_rect();
    egui::Area::new(egui::Id::new("news_phone"))
        .fixed_pos(egui::pos2(screen.right() - 290.0, screen.top() + 90.0))
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            egui::Frame::NONE
                .fill(Color32::from_rgb(0x12, 0x12, 0x14))
                .corner_radius(egui::CornerRadius::same(30))
                .inner_margin(egui::Margin::same(10))
                .shadow(egui::Shadow { offset: [0, 8], blur: 24, spread: 0, color: Color32::from_black_alpha(110) })
                .show(ui, |ui| {
                    egui::Frame::NONE
                        .fill(Color32::from_rgb(0x3a, 0x6e, 0xa8))
                        .corner_radius(egui::CornerRadius::same(20))
                        .inner_margin(egui::Margin::same(10))
                        .show(ui, |ui| {
                            ui.set_width(220.0);
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new("9:41").color(Color32::WHITE).small());
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    ui.label(egui::RichText::new("3G  100%").color(Color32::WHITE).small());
                                });
                            });
                            ui.label(egui::RichText::new(date).color(Color32::from_white_alpha(200)).small());
                            ui.add_space(24.0);
                            egui::Frame::NONE
                                .fill(Color32::from_rgba_unmultiplied(250, 250, 252, 235))
                                .corner_radius(egui::CornerRadius::same(12))
                                .inner_margin(egui::Margin::same(10))
                                .show(ui, |ui| {
                                    ui.set_width(ui.available_width());
                                    ui.label(
                                        egui::RichText::new("STARCITY NEWS  now").color(Color32::from_gray(0x55)).small(),
                                    );
                                    ui.label(egui::RichText::new(e.title).color(Color32::BLACK).strong());
                                    ui.label(egui::RichText::new(e.text).color(Color32::from_gray(0x22)).small());
                                    ui.add_space(4.0);
                                    dismiss = ui
                                        .add(egui::Button::new(egui::RichText::new("Swipe to dismiss").color(Color32::from_rgb(0x0a, 0x60, 0xd0))).frame(false))
                                        .clicked();
                                });
                            ui.add_space(150.0);
                        });
                });
        });
    dismiss
}

/// 2020–2029: a post in a feed.
fn feed(ctx: &egui::Context, e: &Event, i: usize) -> bool {
    let (likes, reposts) = reactions(i);
    let mut dismiss = false;
    let mut open = true;
    win95::window(ctx, "Feed", &mut open, egui::pos2(380.0, 130.0), |ui| {
        ui.set_width(330.0);
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(egui::vec2(34.0, 34.0), egui::Sense::hover());
            ui.painter().circle_filled(rect.center(), 17.0, Color32::from_rgb(0x0a, 0x84, 0xff));
            ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, "S", egui::FontId::proportional(16.0), Color32::WHITE);
            ui.vertical(|ui| {
                ui.label(egui::RichText::new("Starcity News  (verified)").strong());
                ui.label(egui::RichText::new("@starcitynews  ·  just now").small());
            });
        });
        ui.add_space(4.0);
        ui.label(egui::RichText::new(e.title).size(18.0).strong());
        ui.label(e.text);
        ui.add_space(4.0);
        ui.label(egui::RichText::new(format!("{likes} likes  ·  {reposts} reposts  ·  trending")).small());
        dismiss = ui.button("Mute this thread").clicked();
    });
    !open || dismiss
}

/// 2030 on: a signal in the air.
fn signal(ctx: &egui::Context, e: &Event) -> bool {
    let (mut open, mut ok) = (true, false);
    win95::window(ctx, "SIGNAL", &mut open, egui::pos2(380.0, 130.0), |ui| {
        ui.set_width(320.0);
        ui.label(egui::RichText::new(e.title).size(18.0).color(win95::ink()).strong());
        ui.label(e.text);
        ui.add_space(4.0);
        ok = ui.button("Acknowledge").clicked();
    });
    !open || ok
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_news_changes_hands_with_the_decades() {
        assert_eq!(news_style(2000), NewsStyle::Browser);
        assert_eq!(news_style(2009), NewsStyle::Browser);
        assert_eq!(news_style(2010), NewsStyle::Phone);
        assert_eq!(news_style(2025), NewsStyle::Feed);
        assert_eq!(news_style(2045), NewsStyle::Signal);
        // Picking an older look brings its news back with it.
        assert_eq!(style_for(win95::Look::Glass), NewsStyle::Phone);
        assert_eq!(style_for(win95::Look::Classic), NewsStyle::Browser);
    }

    #[test]
    fn addresses_are_tidy() {
        assert_eq!(slug("Fuel price spike"), "fuel-price-spike");
        assert_eq!(slug("Dot-com bust"), "dot-com-bust");
        assert_eq!(slug("\"Convergence\" announced!"), "convergence-announced");
        let (l, r) = reactions(3);
        assert_eq!(reactions(3), (l, r));
    }
}
