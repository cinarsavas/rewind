#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

#[cfg(not(windows))]
compile_error!("Rewind uses Windows capture APIs and only builds for Windows targets (x86_64-pc-windows-msvc).");

mod engine;
mod download;
mod hotkeys;
mod overlay;
mod packs;
mod perf;
mod platform;
mod runtime;
mod thumbs;
mod tray;
mod voice;
mod ui;

fn main() -> eframe::Result<()> {
    // Used when building the installer: fetch the OBS runtime next to the exe, then exit.
    if std::env::args().any(|a| a == "--install-runtime") {
        match runtime::install_blocking() {
            Ok(()) => std::process::exit(0),
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(1);
            }
        }
    }
    if !platform::acquire_single_instance() {
        // Already running (maybe hidden in the tray): bring that window forward and stop.
        platform::focus_existing_window();
        return Ok(());
    }
    let start_hidden = std::env::args().any(|a| a == "--tray");
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("Rewind")
            .with_inner_size([1100.0, 740.0])
            .with_min_inner_size([900.0, 600.0]),
        ..Default::default()
    };
    eframe::run_native("Rewind", options, Box::new(move |cc| Ok(Box::new(ui::RewindApp::new(cc, start_hidden)))))
}
