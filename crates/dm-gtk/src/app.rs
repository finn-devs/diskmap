use crate::window;
use gtk4::prelude::*;
use libadwaita as adw;

const APP_ID: &str = "com.diskmap.app";

pub fn run() {
    // Initialize libadwaita (also initializes GTK4)
    adw::init().expect("Failed to initialize libadwaita");

    let app = adw::Application::builder()
        .application_id(APP_ID)
        .build();

    app.connect_activate(|app| {
        let win = window::build_window(app);
        win.present();
    });

    // Pass empty args — CLI args already handled in main()
    app.run_with_args::<String>(&[]);
}
