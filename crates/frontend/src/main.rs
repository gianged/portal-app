mod api;
mod app;
mod features;
mod primitives;
mod state;
mod theme;
mod util;

use leptos::mount;

fn main() {
    mount::mount_to_body(app::App);
}
