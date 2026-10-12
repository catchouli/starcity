mod app;
mod boats;
mod camera;
mod cargo;
mod clock;
mod clouds;
mod company;
mod daylight;
#[cfg(not(target_arch = "wasm32"))]
mod devtools;
mod era;
mod exchange;
mod hq;
mod input;
mod models;
mod network;
mod picking;
mod planes;
mod props;
mod rails;
mod render;
mod roads;
mod scenario;
mod sea;
#[cfg(test)]
mod shader_test;
mod sim;
mod stations;
mod terminals;
mod terrain;
mod towns;
mod trains;
mod ui;
mod vehicles;

#[cfg(not(target_arch = "wasm32"))]
fn main() -> eframe::Result {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_title("Starcity"),
        ..Default::default()
    };
    let demo = std::env::args().any(|a| a == "--demo");
    eframe::run_native(
        "Starcity",
        options,
        Box::new(move |cc| Ok(Box::new(app::StarcityApp::new(cc, demo)))),
    )
}

#[cfg(target_arch = "wasm32")]
fn main() {
    use eframe::wasm_bindgen::JsCast as _;
    eframe::WebLogger::init(log::LevelFilter::Info).ok();

    wasm_bindgen_futures::spawn_local(async {
        let document = eframe::web_sys::window()
            .expect("no window")
            .document()
            .expect("no document");
        let canvas = document
            .get_element_by_id("starcity")
            .expect("no #starcity canvas")
            .dyn_into::<eframe::web_sys::HtmlCanvasElement>()
            .expect("#starcity is not a canvas");

        let demo = eframe::web_sys::window()
            .and_then(|w| w.location().search().ok())
            .is_some_and(|q| q.contains("demo"));
        let result = eframe::WebRunner::new()
            .start(
                canvas,
                eframe::WebOptions::default(),
                Box::new(move |cc| Ok(Box::new(app::StarcityApp::new(cc, demo)))),
            )
            .await;

        match result {
            Ok(()) => {
                if let Some(loading) = document.get_element_by_id("loading") {
                    loading.remove();
                }
            }
            Err(e) => {
                if let Some(label) = document.get_element_by_id("loading-label") {
                    label.set_text_content(Some(&format!("Starcity failed to start: {e:?}")));
                }
            }
        }
    });
}
