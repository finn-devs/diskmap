mod app;
mod scan_worker;
mod treemap_widget;
mod window;

use dm_scan::escalate;

fn main() {
    // Check if running in --scan-paths mode (elevated helper)
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--scan-paths") {
        if let Err(e) = escalate::handle_scan_paths_cli(&args) {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
        return;
    }

    // Normal GUI mode
    app::run();
}
