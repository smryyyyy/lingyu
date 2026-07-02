#[macro_use]
mod log;
mod api;
mod audio;
mod config;
mod db;
mod input;
mod local_stt;
#[cfg(test)]
mod tests;
mod ui;

#[cfg(target_os = "macos")]
#[macro_use]
extern crate objc;

use gtk4::prelude::*;
use std::sync::Arc;

fn main() {
    // Create tokio runtime for reqwest async + spawn_blocking
    let _tokio_rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let args: Vec<String> = std::env::args().collect();
    let debug = args.iter().any(|a| a == "--debug");
    log::init(debug);

    let config = Arc::new(config::Config::load());

    let app = gtk4::Application::builder()
        .application_id("com.lingyu.app")
        .build();

    let config_c = Arc::clone(&config);
    app.connect_activate(move |app| {
        ui::build_ui(app, Arc::clone(&config_c));
    });

    let gtk_args: Vec<String> = args.into_iter().filter(|a| a != "--debug").collect();
    let gtk_args_ref: Vec<&str> = gtk_args.iter().map(|s| s.as_str()).collect();
    app.run_with_args(&gtk_args_ref);
}
