//! Windows 95 look for egui: grey surfaces, bevelled edges, navy selection.

use eframe::egui::{self, Color32, CornerRadius, Painter, Rect, Response, Sense, Shadow, Stroke, Ui, Vec2, pos2, vec2};

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
        v.override_text_color = None;
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
        // bg_fill is used for slider grooves and checkbox boxes, which are white
        // in Win95; buttons use weak_bg_fill and stay grey.
        for wv in [&mut w.inactive, &mut w.hovered, &mut w.active] {
            wv.bg_fill = Color32::WHITE;
        }
        w.hovered.weak_bg_fill = LIGHT;
        w.active.weak_bg_fill = SHADOW;
        // Open menus and selected items: white on navy, like Win95 menus.
        w.open.weak_bg_fill = NAVY;
        w.open.bg_fill = NAVY;
        w.open.fg_stroke = Stroke::new(1.0, Color32::WHITE);
        v.weak_text_color = Some(SHADOW);

        style.spacing.window_margin = egui::Margin::same(6);
        style.spacing.button_padding = vec2(8.0, 3.0);
        style.spacing.menu_margin = egui::Margin::same(2);
    });
}

/// A Win95 window: bevelled grey frame, navy gradient title bar with white
/// bold text and a bevelled close button. Drag the title bar (or any empty
/// part of the window) to move it.
pub fn window<R>(
    ctx: &egui::Context,
    title: &str,
    open: &mut bool,
    default_pos: egui::Pos2,
    add_contents: impl FnOnce(&mut Ui) -> R,
) -> Option<R> {
    if !*open {
        return None;
    }
    let mut close = false;
    let frame = egui::Frame::NONE.fill(FACE).inner_margin(egui::Margin::same(4));
    let response = egui::Window::new(title)
        .title_bar(false)
        .frame(frame)
        .fade_in(false)
        .fade_out(false)
        .default_pos(default_pos)
        .show(ctx, |ui| {
            close = title_bar(ui, title);
            ui.add_space(4.0);
            egui::Frame::NONE
                .inner_margin(egui::Margin::symmetric(4, 0))
                .show(ui, add_contents)
                .inner
        });
    let response = response?;
    bevel(
        &ctx.layer_painter(response.response.layer_id),
        response.response.rect,
        true,
    );
    if close {
        *open = false;
    }
    response.inner
}

/// Paints a title bar across the top of a window. Returns true if the close button was clicked.
fn title_bar(ui: &mut Ui, title: &str) -> bool {
    let height = 20.0;
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width().max(160.0), height), Sense::hover());
    let p = ui.painter();
    // Navy to blue gradient, as drawn by Win95 Plus! / Win98 title bars.
    let mut mesh = egui::Mesh::default();
    let (left, right) = (NAVY, Color32::from_rgb(0x10, 0x84, 0xd0));
    mesh.colored_vertex(rect.left_top(), left);
    mesh.colored_vertex(rect.right_top(), right);
    mesh.colored_vertex(rect.right_bottom(), right);
    mesh.colored_vertex(rect.left_bottom(), left);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    p.add(egui::Shape::mesh(mesh));
    p.text(
        pos2(rect.min.x + 5.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        title,
        egui::FontId::proportional(13.0),
        Color32::WHITE,
    );
    // Bold by overdrawing one pixel to the right.
    p.text(
        pos2(rect.min.x + 6.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        title,
        egui::FontId::proportional(13.0),
        Color32::WHITE,
    );

    let button = Rect::from_min_size(pos2(rect.max.x - 18.0, rect.min.y + 2.0), vec2(16.0, 14.0));
    let response = ui.interact(button, ui.id().with("close"), Sense::click());
    let pressed = response.is_pointer_button_down_on();
    p.rect_filled(button, 0.0, FACE);
    bevel(p, button, !pressed);
    let c = button.center() + if pressed { vec2(1.0, 1.0) } else { Vec2::ZERO };
    let s = Stroke::new(1.5, Color32::BLACK);
    p.line_segment([c + vec2(-3.5, -3.0), c + vec2(3.5, 3.0)], s);
    p.line_segment([c + vec2(3.5, -3.0), c + vec2(-3.5, 3.0)], s);
    response.clicked()
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
    painter.rect_filled(Rect::from_min_max(r.min, pos2(r.max.x, r.min.y + px)), 0.0, tl);
    painter.rect_filled(Rect::from_min_max(r.min, pos2(r.min.x + px, r.max.y)), 0.0, tl);
    painter.rect_filled(Rect::from_min_max(pos2(r.min.x, r.max.y - px), r.max), 0.0, br);
    painter.rect_filled(Rect::from_min_max(pos2(r.max.x - px, r.min.y), r.max), 0.0, br);
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
pub fn tool_button(ui: &mut Ui, size: f32, selected: bool, icon: impl FnOnce(&Painter, Rect)) -> Response {
    tool_button_ex(ui, size, selected, true, false, icon)
}

/// A toolbar button that can be greyed out (`enabled` false: the headquarters
/// hasn't unlocked it) or outlined to point the player at it (`suggested`).
pub fn tool_button_ex(
    ui: &mut Ui,
    size: f32,
    selected: bool,
    enabled: bool,
    suggested: bool,
    icon: impl FnOnce(&Painter, Rect),
) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());
    let pressed = enabled && (selected || response.is_pointer_button_down_on());
    let p = ui.painter();
    p.rect_filled(rect, 0.0, FACE);
    if selected {
        // Win95 draws latched buttons with a light dither; a checkerboard of 1px dots.
        let mut y = rect.min.y + 2.0;
        while y < rect.max.y - 2.0 {
            let mut x = rect.min.x + 2.0 + ((y - rect.min.y) as i32 % 2) as f32;
            while x < rect.max.x - 2.0 {
                p.rect_filled(Rect::from_min_size(pos2(x, y), Vec2::splat(1.0)), 0.0, HIGHLIGHT);
                x += 2.0;
            }
            y += 1.0;
        }
    }
    bevel(p, rect, !pressed);
    let offset = if pressed { vec2(1.0, 1.0) } else { Vec2::ZERO };
    icon(p, rect.shrink(5.0).translate(offset));
    if !enabled {
        // Win95 greys things out with a dither of white over the icon.
        let mut y = rect.min.y + 3.0;
        while y < rect.max.y - 3.0 {
            let mut x = rect.min.x + 3.0 + ((y - rect.min.y) as i32 % 2) as f32;
            while x < rect.max.x - 3.0 {
                p.rect_filled(Rect::from_min_size(pos2(x, y), Vec2::splat(1.0)), 0.0, FACE);
                x += 2.0;
            }
            y += 1.0;
        }
    }
    if suggested {
        p.rect_stroke(
            rect.shrink(1.0),
            0.0,
            Stroke::new(2.0, Color32::from_rgb(0xf0, 0xb0, 0x00)),
            egui::StrokeKind::Inside,
        );
    }
    response
}

/// A sunken pane showing the company's cash; click it for the finance window.
pub fn money_pane(ui: &mut Ui, width: f32, text: &str, in_the_red: bool) -> Response {
    let height = ui.spacing().interact_size.y;
    let (rect, response) = ui.allocate_exact_size(vec2(width, height), Sense::click());
    let p = ui.painter();
    edge(p, rect, SHADOW, HIGHLIGHT);
    p.text(
        pos2(rect.max.x - 6.0, rect.center().y),
        egui::Align2::RIGHT_CENTER,
        text,
        egui::FontId::proportional(13.0),
        if in_the_red {
            Color32::from_rgb(0xc0, 0x10, 0x10)
        } else {
            Color32::BLACK
        },
    );
    response
}

/// The Compute meter: the running total, the rate, and a log-axis sparkline of the last months.
pub fn compute_pane(ui: &mut Ui, width: f32, total: &str, rate: &str, spark: &[f32]) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(width, 20.0), Sense::hover());
    let p = ui.painter();
    edge(p, rect, SHADOW, HIGHLIGHT);
    let ink = Color32::from_rgb(0x10, 0x40, 0x90);
    p.text(
        pos2(rect.min.x + 6.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        total,
        egui::FontId::proportional(13.0),
        ink,
    );
    // The sparkline: a straight line means exponential growth.
    let area = Rect::from_min_max(pos2(rect.max.x - 50.0, rect.min.y + 4.0), pos2(rect.max.x - 5.0, rect.max.y - 4.0));
    p.text(
        pos2(area.min.x - 6.0, rect.center().y),
        egui::Align2::RIGHT_CENTER,
        rate,
        egui::FontId::proportional(11.0),
        Color32::from_gray(0x50),
    );
    if spark.len() >= 2 {
        let top = spark.iter().copied().fold(3.0f32, f32::max);
        let points: Vec<_> = spark
            .iter()
            .enumerate()
            .map(|(i, &v)| {
                pos2(
                    area.min.x + area.width() * i as f32 / (spark.len() - 1) as f32,
                    area.max.y - area.height() * (v / top).clamp(0.0, 1.0),
                )
            })
            .collect();
        p.add(egui::Shape::line(points, Stroke::new(1.5, ink)));
    }
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

/// A sunken pane with the date, and a sun or moon showing the time of day.
pub fn clock_pane(ui: &mut Ui, width: f32, text: &str, daylight: f32) {
    let height = ui.spacing().interact_size.y;
    let (rect, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    let p = ui.painter();
    edge(p, rect, SHADOW, HIGHLIGHT);
    let c = pos2(rect.min.x + 12.0, rect.center().y);
    if daylight > 0.5 {
        // A sun: a disc with rays.
        let sun = Color32::from_rgb(0xf2, 0xb8, 0x10);
        p.circle_filled(c, 4.0, sun);
        for i in 0..8 {
            let a = i as f32 * std::f32::consts::FRAC_PI_4;
            let d = vec2(a.cos(), a.sin());
            p.line_segment([c + d * 6.0, c + d * 8.0], egui::Stroke::new(1.0, sun));
        }
    } else {
        // A crescent moon: a pale disc with a bite taken out of it.
        p.circle_filled(c, 6.0, Color32::from_rgb(0x30, 0x40, 0x80));
        p.circle_filled(c + vec2(2.5, -1.5), 5.0, FACE);
    }
    p.text(
        pos2(rect.min.x + 26.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        text,
        egui::FontId::proportional(12.0),
        Color32::BLACK,
    );
}
