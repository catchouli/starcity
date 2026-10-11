//! Windows 95 look for egui: grey surfaces, bevelled edges, navy selection.

use eframe::egui::{
    self, Color32, CornerRadius, Painter, Rect, Response, Sense, Shadow, Stroke, Ui, Vec2, pos2,
    vec2,
};

pub const FACE: Color32 = Color32::from_rgb(0xc0, 0xc0, 0xc0);
pub const LIGHT: Color32 = Color32::from_rgb(0xdf, 0xdf, 0xdf);
pub const HIGHLIGHT: Color32 = Color32::WHITE;
pub const SHADOW: Color32 = Color32::from_rgb(0x80, 0x80, 0x80);
pub const DARK: Color32 = Color32::from_rgb(0x0a, 0x0a, 0x0a);
pub const NAVY: Color32 = Color32::from_rgb(0x00, 0x00, 0x80);

pub fn apply(ctx: &egui::Context) {
    ctx.all_styles_mut(|style| {
        let v = &mut style.visuals;
        *v = egui::Visuals::light();
        v.override_text_color = Some(Color32::BLACK);
        v.panel_fill = FACE;
        v.window_fill = FACE;
        v.window_stroke = Stroke::new(1.0, DARK);
        v.window_shadow = Shadow::NONE;
        v.popup_shadow = Shadow::NONE;
        v.window_corner_radius = CornerRadius::ZERO;
        v.menu_corner_radius = CornerRadius::ZERO;
        v.extreme_bg_color = Color32::WHITE;
        v.faint_bg_color = LIGHT;
        v.selection.bg_fill = NAVY;
        v.selection.stroke = Stroke::new(1.0, Color32::WHITE);
        v.hyperlink_color = NAVY;
        v.slider_trailing_fill = false;
        v.handle_shape = egui::style::HandleShape::Rect { aspect_ratio: 0.5 };

        let w = &mut v.widgets;
        for (wv, stroke) in [
            (&mut w.noninteractive, SHADOW),
            (&mut w.inactive, DARK),
            (&mut w.hovered, DARK),
            (&mut w.active, DARK),
            (&mut w.open, DARK),
        ] {
            wv.corner_radius = CornerRadius::ZERO;
            wv.bg_fill = FACE;
            wv.weak_bg_fill = FACE;
            wv.bg_stroke = Stroke::new(1.0, stroke);
            wv.fg_stroke = Stroke::new(1.0, Color32::BLACK);
            wv.expansion = 0.0;
        }
        w.noninteractive.bg_stroke = Stroke::new(1.0, SHADOW);
        w.hovered.weak_bg_fill = LIGHT;
        w.active.weak_bg_fill = SHADOW;
        w.open.weak_bg_fill = NAVY;
        w.open.fg_stroke = Stroke::new(1.0, Color32::WHITE);

        style.spacing.window_margin = egui::Margin::same(6);
        style.spacing.button_padding = vec2(8.0, 3.0);
        style.spacing.menu_margin = egui::Margin::same(2);
    });
}

/// Draws a two-pixel Win95 bevel inside `rect`. Raised for buttons and windows,
/// sunken for pressed buttons, fields and status bar panes.
pub fn bevel(painter: &Painter, rect: Rect, raised: bool) {
    let (outer_tl, outer_br, inner_tl, inner_br) = if raised {
        (HIGHLIGHT, DARK, LIGHT, SHADOW)
    } else {
        (SHADOW, HIGHLIGHT, DARK, LIGHT)
    };
    edge(painter, rect, outer_tl, outer_br);
    edge(painter, rect.shrink(1.0), inner_tl, inner_br);
}

fn edge(painter: &Painter, r: Rect, tl: Color32, br: Color32) {
    let px = 1.0;
    painter.rect_filled(
        Rect::from_min_max(r.min, pos2(r.max.x, r.min.y + px)),
        0.0,
        tl,
    );
    painter.rect_filled(
        Rect::from_min_max(r.min, pos2(r.min.x + px, r.max.y)),
        0.0,
        tl,
    );
    painter.rect_filled(
        Rect::from_min_max(pos2(r.min.x, r.max.y - px), r.max),
        0.0,
        br,
    );
    painter.rect_filled(
        Rect::from_min_max(pos2(r.max.x - px, r.min.y), r.max),
        0.0,
        br,
    );
}

/// Etched vertical separator, as between toolbar groups.
pub fn separator(ui: &mut Ui, height: f32) {
    let (rect, _) = ui.allocate_exact_size(vec2(6.0, height), Sense::hover());
    let x = rect.center().x.round();
    let p = ui.painter();
    p.rect_filled(
        Rect::from_min_max(pos2(x - 1.0, rect.min.y), pos2(x, rect.max.y)),
        0.0,
        SHADOW,
    );
    p.rect_filled(
        Rect::from_min_max(pos2(x, rect.min.y), pos2(x + 1.0, rect.max.y)),
        0.0,
        HIGHLIGHT,
    );
}

/// A square toolbar button with a custom-painted icon. Stays pressed while `selected`.
pub fn tool_button(
    ui: &mut Ui,
    size: f32,
    selected: bool,
    icon: impl FnOnce(&Painter, Rect),
) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());
    let pressed = selected || response.is_pointer_button_down_on();
    let p = ui.painter();
    p.rect_filled(rect, 0.0, FACE);
    if selected {
        // Win95 draws latched buttons with a light dither; a checkerboard of 1px dots.
        let mut y = rect.min.y + 2.0;
        while y < rect.max.y - 2.0 {
            let mut x = rect.min.x + 2.0 + ((y - rect.min.y) as i32 % 2) as f32;
            while x < rect.max.x - 2.0 {
                p.rect_filled(
                    Rect::from_min_size(pos2(x, y), Vec2::splat(1.0)),
                    0.0,
                    HIGHLIGHT,
                );
                x += 2.0;
            }
            y += 1.0;
        }
    }
    bevel(p, rect, !pressed);
    let offset = if pressed { vec2(1.0, 1.0) } else { Vec2::ZERO };
    icon(p, rect.shrink(5.0).translate(offset));
    response
}

/// A sunken status bar pane holding some text.
pub fn status_pane(ui: &mut Ui, width: f32, text: &str) {
    let height = ui.spacing().interact_size.y;
    let (rect, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    let p = ui.painter();
    edge(p, rect, SHADOW, HIGHLIGHT);
    p.text(
        pos2(rect.min.x + 4.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        text,
        egui::FontId::proportional(12.0),
        Color32::BLACK,
    );
}
