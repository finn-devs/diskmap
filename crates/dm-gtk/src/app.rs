use crate::window;
use gtk4::prelude::*;
use gtk4::glib;
use libadwaita as adw;
use libadwaita::prelude::*;

const APP_ID: &str = "com.finndevs.diskmap";

pub fn run() {
    adw::init().expect("Failed to initialize libadwaita");

    let app = adw::Application::builder()
        .application_id(APP_ID)
        .build();

    app.connect_startup(|_| {
        gtk4::Window::set_default_icon_name("com.finndevs.diskmap");
    });

    app.connect_activate(|app| {
        show_splash(app);
    });

    app.run_with_args::<String>(&[]);
}

fn show_splash(app: &adw::Application) {
    let splash_win = adw::ApplicationWindow::builder()
        .application(app)
        .title("DiskMap")
        .default_width(1200)
        .default_height(800)
        .decorated(false)
        .build();

    let canvas = gtk4::DrawingArea::new();
    canvas.set_hexpand(true);
    canvas.set_vexpand(true);

    let frame_counter = std::rc::Rc::new(std::cell::RefCell::new(0u32));

    {
        let frame_counter = frame_counter.clone();
        canvas.set_draw_func(move |_area, cr, width, height| {
            let frame = *frame_counter.borrow();
            let w = width as f64;
            let h = height as f64;
            let t = (frame as f64 / 75.0).min(1.0); // 0→1 over ~2.5s

            // Background — same dark as the app
            cr.set_source_rgb(0.12, 0.12, 0.14);
            cr.rectangle(0.0, 0.0, w, h);
            let _ = cr.fill();

            // Animated pie chart slices spinning in from center
            let cx = w / 2.0;
            let cy = h / 2.0 - 30.0;
            let max_radius = 140.0;
            let inner_radius = max_radius * 0.35;

            // Slices grow outward and fade in
            let radius = inner_radius + (max_radius - inner_radius) * ease_out_cubic(t);
            let spin = (1.0 - t) * std::f64::consts::PI; // Spins from 180° to 0°

            let slice_colors: [(f64, f64, f64); 6] = [
                (0.90, 0.25, 0.20), // Red
                (0.95, 0.55, 0.20), // Orange
                (0.90, 0.80, 0.25), // Yellow
                (0.30, 0.75, 0.35), // Green
                (0.15, 0.65, 0.65), // Teal
                (0.30, 0.50, 0.80), // Blue
            ];

            let slice_fractions = [0.30, 0.22, 0.18, 0.13, 0.10, 0.07];
            let mut angle = -std::f64::consts::FRAC_PI_2 + spin;
            let alpha = ease_out_cubic(t);

            for i in 0..6 {
                let sweep = slice_fractions[i] * std::f64::consts::TAU;
                let end = angle + sweep;
                let (r, g, b) = slice_colors[i];

                cr.new_path();
                cr.arc(cx, cy, radius, angle, end);
                cr.arc_negative(cx, cy, inner_radius, end, angle);
                cr.close_path();
                cr.set_source_rgba(r, g, b, alpha * 0.85);
                let _ = cr.fill_preserve();

                // Slice border
                cr.set_source_rgba(0.12, 0.12, 0.14, alpha);
                cr.set_line_width(2.0);
                let _ = cr.stroke();

                angle = end;
            }

            // Inner circle (donut hole) — always dark
            cr.arc(cx, cy, inner_radius, 0.0, std::f64::consts::TAU);
            cr.set_source_rgb(0.12, 0.12, 0.14);
            let _ = cr.fill();

            // "DiskMap" inside the donut — fades in
            let text_alpha = ease_out_cubic((t - 0.2).max(0.0) / 0.8);
            cr.set_source_rgba(1.0, 1.0, 1.0, text_alpha * 0.9);
            cr.select_font_face("sans-serif", gtk4::cairo::FontSlant::Normal, gtk4::cairo::FontWeight::Bold);
            cr.set_font_size(22.0);
            let te = cr.text_extents("DiskMap").unwrap();
            let _ = cr.move_to(cx - te.width() / 2.0, cy + 7.0);
            let _ = cr.show_text("DiskMap");

            // Company name below the chart — slides up and fades in
            let company_t = ((t - 0.4) * 2.5).clamp(0.0, 1.0);
            let company_y = h / 2.0 + max_radius + 40.0 + (1.0 - ease_out_cubic(company_t)) * 20.0;

            cr.set_source_rgba(1.0, 1.0, 1.0, ease_out_cubic(company_t) * 0.9);
            cr.select_font_face("sans-serif", gtk4::cairo::FontSlant::Normal, gtk4::cairo::FontWeight::Bold);
            cr.set_font_size(28.0);
            let te2 = cr.text_extents("Finn Devs").unwrap();
            let _ = cr.move_to(cx - te2.width() / 2.0, company_y);
            let _ = cr.show_text("Finn Devs");

            // "LLC"
            cr.set_source_rgba(0.42, 0.62, 1.0, ease_out_cubic(company_t) * 0.8);
            cr.set_font_size(14.0);
            cr.select_font_face("sans-serif", gtk4::cairo::FontSlant::Normal, gtk4::cairo::FontWeight::Normal);
            let te3 = cr.text_extents("LLC").unwrap();
            let _ = cr.move_to(cx - te3.width() / 2.0, company_y + 22.0);
            let _ = cr.show_text("LLC");

            // URL — last to appear
            let url_t = ((t - 0.6) * 2.5).clamp(0.0, 1.0);
            cr.set_source_rgba(0.5, 0.6, 0.8, ease_out_cubic(url_t) * 0.5);
            cr.set_font_size(12.0);
            let te4 = cr.text_extents("finndevs.com").unwrap();
            let _ = cr.move_to(cx - te4.width() / 2.0, company_y + 44.0);
            let _ = cr.show_text("finndevs.com");

            // Thin accent line under the chart — grows from center
            let line_t = ((t - 0.3) * 3.0).clamp(0.0, 1.0);
            let line_half_w = ease_out_cubic(line_t) * 80.0;
            cr.set_source_rgba(0.42, 0.62, 1.0, 0.4 * ease_out_cubic(line_t));
            cr.set_line_width(1.5);
            cr.move_to(cx - line_half_w, h / 2.0 + max_radius + 10.0);
            cr.line_to(cx + line_half_w, h / 2.0 + max_radius + 10.0);
            let _ = cr.stroke();
        });
    }

    // Animation timer
    {
        let frame_counter = frame_counter.clone();
        let canvas_weak = canvas.downgrade();
        glib::timeout_add_local(std::time::Duration::from_millis(33), move || {
            *frame_counter.borrow_mut() += 1;
            if let Some(canvas) = canvas_weak.upgrade() {
                canvas.queue_draw();
                glib::ControlFlow::Continue
            } else {
                glib::ControlFlow::Break
            }
        });
    }

    // Click opens website
    let click = gtk4::GestureClick::new();
    click.connect_released(move |_, _, _, _| {
        let _ = gtk4::gio::AppInfo::launch_default_for_uri(
            "https://finndevs.com",
            gtk4::gio::AppLaunchContext::NONE,
        );
    });
    canvas.add_controller(click);

    splash_win.set_content(Some(&canvas));
    splash_win.present();

    // Transition to main app
    let app_clone = app.clone();
    let splash_weak = splash_win.downgrade();
    glib::timeout_add_local_once(std::time::Duration::from_millis(3000), move || {
        if let Some(splash) = splash_weak.upgrade() {
            splash.close();
        }
        let win = window::build_window(&app_clone);
        win.present();
    });
}

fn ease_out_cubic(t: f64) -> f64 {
    1.0 - (1.0 - t).powi(3)
}
