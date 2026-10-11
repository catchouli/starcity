//! Native-only development hooks, driven by environment variables:
//!
//! - `STARCITY_SCREENSHOT=out.png`: render a few frames, save a screenshot, exit.
//! - `STARCITY_DEBUG=1`: open the debug window at startup.
//! - `STARCITY_DEMO=roads`: lay out some sample roads around the camera.
//! - `STARCITY_ZOOM=30`: initial zoom (visible height in tiles at the focus).
//! - `STARCITY_FOCUS=dx,dz`: move the camera focus from the map centre.

use eframe::egui;

pub struct DevTools {
    screenshot_to: Option<String>,
    frames: u32,
    requested: bool,
}

impl DevTools {
    pub fn from_env() -> DevTools {
        DevTools {
            screenshot_to: std::env::var("STARCITY_SCREENSHOT").ok(),
            frames: 0,
            requested: false,
        }
    }

    pub fn demo() -> Option<String> {
        std::env::var("STARCITY_DEMO").ok()
    }

    pub fn zoom() -> Option<f32> {
        std::env::var("STARCITY_ZOOM").ok()?.parse().ok()
    }

    pub fn show_debug() -> bool {
        std::env::var_os("STARCITY_DEBUG").is_some()
    }

    /// Call once per frame.
    pub fn update(&mut self, ctx: &egui::Context) {
        let Some(path) = &self.screenshot_to else {
            return;
        };
        self.frames += 1;
        ctx.request_repaint();
        // Give the UI a few frames to lay out before capturing.
        if self.frames == 10 && !self.requested {
            self.requested = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
        }
        let image = ctx.input(|i| {
            i.events.iter().find_map(|e| match e {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if let Some(image) = image {
            save_png(path, &image);
            eprintln!("saved screenshot to {path}");
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
}

fn save_png(path: &str, image: &egui::ColorImage) {
    let file = std::fs::File::create(path).expect("create screenshot file");
    let mut encoder = png::Encoder::new(
        std::io::BufWriter::new(file),
        image.size[0] as u32,
        image.size[1] as u32,
    );
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().expect("png header");
    writer
        .write_image_data(bytemuck::cast_slice(&image.pixels))
        .expect("png data");
}
