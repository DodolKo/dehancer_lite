pub mod app;
pub mod color;
pub mod effects;
pub mod gpu;
pub mod halation;
pub mod pipeline;

#[cfg(not(target_arch = "wasm32"))]
use eframe::egui;

#[cfg(not(target_arch = "wasm32"))]
pub fn run_native() -> eframe::Result<()> {
    let mut options = eframe::NativeOptions::default();
    options.viewport = egui::ViewportBuilder::default().with_inner_size([1440.0, 900.0]);
    options.renderer = eframe::Renderer::Wgpu;

    if let eframe::egui_wgpu::WgpuSetup::CreateNew(ref mut create_new) =
        options.wgpu_options.wgpu_setup
    {
        create_new.instance_descriptor.backends = eframe::wgpu::Backends::PRIMARY;
    }

    eframe::run_native(
        "Dehancer Lite · Film Stack MVP",
        options,
        Box::new(|cc| Ok(Box::new(app::HalationApp::new(cc)))),
    )
}

#[cfg(target_arch = "wasm32")]
use std::cell::RefCell;

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsCast;

#[cfg(target_arch = "wasm32")]
thread_local! {
    static WEB_RUNNER: RefCell<Option<eframe::WebRunner>> = const { RefCell::new(None) };
}

#[cfg(target_arch = "wasm32")]
fn set_runner(runner: eframe::WebRunner) {
    WEB_RUNNER.with(|cell| {
        *cell.borrow_mut() = Some(runner);
    });
}

#[cfg(target_arch = "wasm32")]
fn set_page_status(message: &str) {
    if let Some(element) = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.get_element_by_id("pageStatus"))
    {
        element.set_text_content(Some(message));
    }
}

#[cfg(target_arch = "wasm32")]
fn dispatch_app_ready() {
    let Some(window) = web_sys::window() else {
        return;
    };

    match web_sys::Event::new("DehancerAppReady") {
        Ok(event) => {
            let _ = window.dispatch_event(&event);
        }
        Err(err) => {
            log::warn!("Could not dispatch DehancerAppReady event: {err:?}");
        }
    }
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub fn load_web_image(name: String, bytes: &[u8]) -> Result<(), JsValue> {
    let runner = WEB_RUNNER
        .with(|cell| cell.borrow().as_ref().cloned())
        .ok_or_else(|| JsValue::from_str("Web runner is not initialized"))?;

    let mut app = runner
        .app_mut::<app::HalationApp>()
        .ok_or_else(|| JsValue::from_str("Web runner is not ready"))?;

    app.load_image_from_web(&name, bytes)
        .map_err(|err| JsValue::from_str(&format!("{err:#}")))
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(start)]
pub fn wasm_start() {
    eframe::WebLogger::init(log::LevelFilter::Info).ok();
    console_error_panic_hook::set_once();

    let Some(canvas) = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.get_element_by_id("halation-canvas"))
        .and_then(|element| element.dyn_into::<web_sys::HtmlCanvasElement>().ok())
    else {
        log::error!("Could not find canvas element with id `halation-canvas`");
        return;
    };

    let runner = eframe::WebRunner::new();
    set_runner(runner.clone());

    let mut options = eframe::WebOptions::default();
    options.renderer = eframe::Renderer::Wgpu;

    if let eframe::egui_wgpu::WgpuSetup::CreateNew(ref mut create_new) =
        options.wgpu_options.wgpu_setup
    {
        create_new.instance_descriptor.backends = eframe::wgpu::Backends::BROWSER_WEBGPU;
    }

    wasm_bindgen_futures::spawn_local(async move {
        match runner
            .start(
                canvas,
                options,
                Box::new(|cc| Ok(Box::new(app::HalationApp::new(cc)))),
            )
            .await
        {
            Ok(()) => {
                set_page_status(
                    "Renderer ready. Load an image, then tune the film stack preset and effect modules.",
                );
                dispatch_app_ready();
            }
            Err(err) => {
                let message = format!(
                    "WebGPU startup failed. Enable WebGPU or use a supported browser. Error: {err:?}"
                );
                set_page_status(&message);
                log::error!(
                    "WebGPU runtime startup failed. This MVP requires browser WebGPU support. Error: {err:?}"
                );
            }
        }
    });
}
