#[cfg(not(target_arch = "wasm32"))]
fn main() -> eframe::Result<()> {
    dehancer_lite::run_native()
}

#[cfg(target_arch = "wasm32")]
fn main() {}
