mod api;
mod app;
mod features;
mod primitives;
mod state;
mod theme;
mod util;

fn main() {
    leptos::mount::mount_to_body(app::App);
}
