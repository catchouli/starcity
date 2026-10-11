mod app;
mod camera;
mod render;
mod terrain;
mod ui;

#[cfg(not(target_arch = "wasm32"))]
fn main() -> eframe::Result {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_title("Starcity"),
        ..Default::default()
    };
    eframe::run_native(
        "Starcity",
        options,
        Box::new(|cc| Ok(Box::new(app::StarcityApp::new(cc)))),
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

        let result = eframe::WebRunner::new()
            .start(
                canvas,
                eframe::WebOptions::default(),
                Box::new(|cc| Ok(Box::new(app::StarcityApp::new(cc)))),
            )
            .await;

        if let Some(loading) = document.get_element_by_id("loading") {
            match result {
                Ok(()) => loading.remove(),
                Err(e) => {
                    loading.set_inner_html(&format!("<p>Starcity failed to start: {e:?}</p>"))
                }
            }
        }
    });
}
