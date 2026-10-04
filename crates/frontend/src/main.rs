#[cfg(not(target_arch = "wasm32"))]
fn main() {}
#[cfg(target_arch = "wasm32")]
mod browser;
#[cfg(target_arch = "wasm32")]
fn main() {
    leptos::mount::mount_to_body(browser::App);
}
