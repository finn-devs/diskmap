use crate::scan_worker::{self, ScanMsg};
use crate::treemap_widget;
use dm_core::model::FileTree;
use dm_core::treemap::TreemapRect;


use gtk4::prelude::*;
use gtk4::{gdk, glib};
use libadwaita as adw;
use std::cell::RefCell;
use std::rc::Rc;

/// Shared application state.
pub(crate) struct AppState {
    pub tree: Option<FileTree>,
    pub rects: Vec<TreemapRect>,
    pub view_root: u32,
    pub view_stack: Vec<u32>,
    pub hovered_index: Option<u32>,
    pub selected_indices: Vec<u32>,
    /// The absolute path of the currently scanned directory.
    pub scan_root_path: String,
    /// Stack of previous states for back navigation.
    pub history: Vec<HistoryEntry>,
}

/// Saved state for back navigation.
pub(crate) struct HistoryEntry {
    pub tree: FileTree,
    pub rects: Vec<TreemapRect>,
    pub scan_root_path: String,
}

impl AppState {
    fn new() -> Self {
        Self {
            tree: None,
            rects: Vec::new(),
            view_root: 0,
            view_stack: Vec::new(),
            hovered_index: None,
            selected_indices: Vec::new(),
            scan_root_path: String::new(),
            history: Vec::new(),
        }
    }
}

pub fn build_window(app: &adw::Application) -> adw::ApplicationWindow {
    // Load custom CSS
    load_css();

    let state = Rc::new(RefCell::new(AppState::new()));

    // --- Header bar ---
    let header = adw::HeaderBar::new();
    header.add_css_class("flat");

    let title_label = gtk4::Label::new(Some("DiskMap"));
    title_label.add_css_class("heading");
    header.set_title_widget(Some(&title_label));

    let scan_button = gtk4::Button::new();
    let scan_btn_content = adw::ButtonContent::new();
    scan_btn_content.set_icon_name("folder-open-symbolic");
    scan_btn_content.set_label("Scan Folder");
    scan_button.set_child(Some(&scan_btn_content));
    scan_button.add_css_class("suggested-action");
    scan_button.add_css_class("pill");
    header.pack_end(&scan_button);

    let back_button = gtk4::Button::from_icon_name("go-previous-symbolic");
    back_button.set_tooltip_text(Some("Go back"));
    back_button.set_sensitive(false);
    back_button.add_css_class("flat");
    header.pack_start(&back_button);

    // --- Main content stack: welcome view vs treemap view ---
    let view_stack = gtk4::Stack::new();
    view_stack.set_transition_type(gtk4::StackTransitionType::Crossfade);
    view_stack.set_transition_duration(200);

    // --- Welcome screen ---
    let (welcome_page, welcome_buttons) = build_welcome_page();
    view_stack.add_named(&welcome_page, Some("welcome"));

    // Placeholder labels (no longer displayed, but kept to avoid removing all references)
    let loading_label = gtk4::Label::new(None);
    let loading_path_label = gtk4::Label::new(None);

    // --- Treemap view ---
    // Late-binding scan function — set after start_scan is defined
    let scan_fn: Rc<RefCell<Option<Rc<dyn Fn(String)>>>> = Rc::new(RefCell::new(None));
    let scan_fn_proxy: Rc<dyn Fn(String)> = {
        let scan_fn = scan_fn.clone();
        Rc::new(move |path: String| {
            if let Some(f) = scan_fn.borrow().as_ref() {
                f(path);
            }
        })
    };
    let treemap_area = treemap_widget::create_treemap_widget(state.clone(), back_button.clone(), scan_fn_proxy);

    // --- Left gutter: directory tree browser ---
    let dir_tree_box = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    dir_tree_box.set_width_request(250);
    dir_tree_box.add_css_class("dir-tree-panel");

    let dir_tree_header = gtk4::Label::new(Some("Filesystem"));
    dir_tree_header.set_halign(gtk4::Align::Start);
    dir_tree_header.add_css_class("heading");
    dir_tree_header.set_margin_start(12);
    dir_tree_header.set_margin_top(8);
    dir_tree_header.set_margin_bottom(4);
    dir_tree_box.append(&dir_tree_header);

    let dir_tree_list = gtk4::ListBox::new();
    dir_tree_list.add_css_class("navigation-sidebar");
    dir_tree_list.set_selection_mode(gtk4::SelectionMode::Single);

    let dir_tree_scroll = gtk4::ScrolledWindow::new();
    dir_tree_scroll.set_child(Some(&dir_tree_list));
    dir_tree_scroll.set_policy(gtk4::PolicyType::Never, gtk4::PolicyType::Automatic);
    dir_tree_scroll.set_vexpand(true);
    dir_tree_box.append(&dir_tree_scroll);

    // --- Right side: details panel ---
    let details_label = gtk4::Label::new(None);
    details_label.set_halign(gtk4::Align::Start);
    details_label.set_valign(gtk4::Align::Start);
    details_label.set_wrap(true);
    details_label.set_xalign(0.0);
    details_label.add_css_class("body");
    details_label.add_css_class("dim-label");
    details_label.set_margin_start(12);
    details_label.set_margin_end(12);
    details_label.set_margin_top(8);

    // Breadcrumb bar
    let breadcrumb_bar = gtk4::Box::new(gtk4::Orientation::Horizontal, 2);
    breadcrumb_bar.set_margin_start(12);
    breadcrumb_bar.set_margin_end(12);
    breadcrumb_bar.set_margin_top(6);
    breadcrumb_bar.set_margin_bottom(6);
    breadcrumb_bar.add_css_class("breadcrumb-bar");

    // Hidden label used by scan complete handler (we show dir name in breadcrumb instead)
    let sidebar_label = gtk4::Label::new(None);
    sidebar_label.set_visible(false);

    // Status bar
    let status_bar = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    status_bar.set_margin_start(12);
    status_bar.set_margin_end(12);
    status_bar.set_margin_top(4);
    status_bar.set_margin_bottom(6);
    let status_label = gtk4::Label::new(None);
    status_label.set_halign(gtk4::Align::Start);
    status_label.set_hexpand(true);
    status_label.add_css_class("caption");
    status_label.add_css_class("dim-label");
    status_bar.append(&status_label);

    // Progress bar
    let progress_bar = gtk4::ProgressBar::new();
    progress_bar.set_visible(false);
    progress_bar.set_show_text(true);
    progress_bar.add_css_class("osd");

    // --- Scan spinner overlay ---
    let scan_spinner_box = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
    scan_spinner_box.set_halign(gtk4::Align::Center);
    scan_spinner_box.set_valign(gtk4::Align::Center);
    let scan_spinner = gtk4::Spinner::new();
    scan_spinner.set_size_request(32, 32);
    scan_spinner_box.append(&scan_spinner);
    let scan_spinner_label = gtk4::Label::new(Some("Scanning..."));
    scan_spinner_label.add_css_class("dim-label");
    scan_spinner_box.append(&scan_spinner_label);
    scan_spinner_box.set_visible(false);

    // Overlay the spinner on top of the pie chart
    let chart_overlay = gtk4::Overlay::new();
    chart_overlay.set_child(Some(&treemap_area));
    chart_overlay.add_overlay(&scan_spinner_box);
    treemap_area.set_vexpand(true);
    chart_overlay.set_vexpand(true);

    // --- Right column: pie chart + details below ---
    let right_column = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    right_column.append(&breadcrumb_bar);
    right_column.append(&chart_overlay);

    // Details at the bottom of the right column
    let details_scroll = gtk4::ScrolledWindow::new();
    details_scroll.set_child(Some(&details_label));
    details_scroll.set_policy(gtk4::PolicyType::Never, gtk4::PolicyType::Automatic);
    details_scroll.set_max_content_height(150);
    details_scroll.set_propagate_natural_height(true);
    right_column.append(&details_scroll);

    // --- Main split: dir tree | pie chart ---
    let content_paned = gtk4::Paned::new(gtk4::Orientation::Horizontal);
    content_paned.set_position(250);
    content_paned.set_shrink_start_child(false);
    content_paned.set_shrink_end_child(false);
    content_paned.set_start_child(Some(&dir_tree_box));
    content_paned.set_end_child(Some(&right_column));

    let treemap_view = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    treemap_view.append(&content_paned);
    treemap_view.append(&status_bar);
    content_paned.set_vexpand(true);

    view_stack.add_named(&treemap_view, Some("treemap"));

    // --- Main layout ---
    let main_box = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    main_box.append(&header);
    main_box.append(&view_stack);

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("DiskMap")
        .default_width(1200)
        .default_height(800)
        .content(&main_box)
        .build();

    // --- Helper: start scan for a path ---
    let start_scan = {
        let state = state.clone();
        let treemap_area = treemap_area.clone();
        let status_label = status_label.clone();
        let _progress_bar = progress_bar.clone();
        let sidebar_label = sidebar_label.clone();
        let details_label = details_label.clone();
        let back_button = back_button.clone();
        let view_stack = view_stack.clone();
        let breadcrumb_bar = breadcrumb_bar.clone();
        let loading_label = loading_label.clone();
        let loading_path_label = loading_path_label.clone();
        let dir_tree_list = dir_tree_list.clone();
        let scan_spinner_box = scan_spinner_box.clone();
        let scan_spinner = scan_spinner.clone();
        let scan_spinner_label = scan_spinner_label.clone();

        Rc::new(move |path_str: String| {
            // Show spinner overlay
            scan_spinner_box.set_visible(true);
            scan_spinner.start();
            scan_spinner_label.set_label("Scanning...");

            // Switch to treemap view if on welcome screen
            view_stack.set_visible_child_name("treemap");

            let state_c = state.clone();
            let treemap_c = treemap_area.clone();
            let status_c = status_label.clone();
            let sidebar_c = sidebar_label.clone();
            let details_c = details_label.clone();
            let back_c = back_button.clone();
            let breadcrumb_c = breadcrumb_bar.clone();
            let view_stack_c = view_stack.clone();
            let loading_label_c = loading_label.clone();
            let loading_path_c = loading_path_label.clone();
            let dir_tree_c = dir_tree_list.clone();
            let spinner_box_c = scan_spinner_box.clone();
            let spinner_c = scan_spinner.clone();
            let spinner_label_c = scan_spinner_label.clone();

            scan_worker::start_scan(path_str.clone(), move |msg| match msg {
                ScanMsg::Progress(p) => {
                    let phase_text = match p.phase {
                        dm_scan::walker::ScanPhase::SizingDirs => "Calculating sizes",
                        _ => "Scanning",
                    };
                    let text = format!(
                        "{} files  \u{00b7}  {} dirs  \u{00b7}  {}",
                        p.files_scanned,
                        p.dirs_scanned,
                        format_size(p.bytes_scanned),
                    );
                    loading_label_c.set_label(&text);
                    loading_path_c.set_label(&format!("{phase_text}: {}", p.current_path));
                    spinner_label_c.set_label(&format!("{phase_text}..."));
                }
                ScanMsg::Complete(complete) => {
                    // Hide spinner
                    spinner_box_c.set_visible(false);
                    spinner_c.stop();

                    let root_name = complete.result.tree.root_node().name.clone();
                    let total_size = complete.result.tree.total_size;
                    let denied_count = complete.result.denied_paths.len();
                    let file_count = complete.result.tree.nodes.iter()
                        .filter(|n| !n.is_dir).count() as u64;
                    let dir_count = complete.result.tree.nodes.iter()
                        .filter(|n| n.is_dir && n.depth > 0).count() as u64;

                    // Populate directory tree sidebar
                    populate_dir_tree(&dir_tree_c, &complete.result.tree, &path_str);

                    // Store tree and pre-computed rects
                    {
                        let mut s = state_c.borrow_mut();
                        s.view_root = complete.result.tree.root;
                        s.view_stack.clear();
                        s.rects = complete.initial_rects;
                        s.scan_root_path = path_str.clone();
                        s.tree = Some(complete.result.tree);
                    }
                    back_c.set_sensitive(!state_c.borrow().history.is_empty());

                    // Status bar
                    let denied_text = if denied_count > 0 {
                        format!("  \u{00b7}  {} restricted", denied_count)
                    } else {
                        String::new()
                    };
                    status_c.set_label(&format!(
                        "{} files  \u{00b7}  {} dirs  \u{00b7}  {}{}",
                        file_count, dir_count,
                        format_size(total_size), denied_text,
                    ));
                    sidebar_c.set_label(&root_name);
                    update_breadcrumb(&breadcrumb_c, &path_str);

                    // Lazy-load analysis details
                    details_c.set_label(&format!("Total: {}", format_size(total_size)));

                    // Switch to treemap immediately
                    view_stack_c.set_visible_child_name("treemap");
                    treemap_c.queue_draw();
                }
                ScanMsg::Error(msg) => {
                    spinner_box_c.set_visible(false);
                    spinner_c.stop();
                    status_c.set_label(&format!("Error: {msg}"));
                    view_stack_c.set_visible_child_name("treemap");
                }
            });
        })
    };

    // --- Welcome page: wire up the quick-scan buttons ---
    *scan_fn.borrow_mut() = Some(start_scan.clone());

    for btn in welcome_buttons {
        let start_scan = start_scan.clone();
        let path: String = btn.widget_name().into();
        btn.connect_clicked(move |_| {
            start_scan(path.clone());
        });
    }

    // --- Scan button handler (file chooser) ---
    {
        let start_scan = start_scan.clone();
        let window_weak = window.downgrade();

        scan_button.connect_clicked(move |_| {
            let Some(win) = window_weak.upgrade() else {
                return;
            };

            let dialog = gtk4::FileChooserDialog::new(
                Some("Select directory to scan"),
                Some(&win),
                gtk4::FileChooserAction::SelectFolder,
                &[
                    ("Cancel", gtk4::ResponseType::Cancel),
                    ("Scan", gtk4::ResponseType::Accept),
                ],
            );
            dialog.set_modal(true);

            let start_scan = start_scan.clone();
            dialog.connect_response(move |dialog, response| {
                if response == gtk4::ResponseType::Accept {
                    if let Some(file) = dialog.file() {
                        if let Some(path) = file.path() {
                            start_scan(path.to_string_lossy().into_owned());
                        }
                    }
                }
                dialog.close();
            });

            dialog.show();
        });
    }

    // --- Back button handler ---
    {
        let state = state.clone();
        let treemap_area = treemap_area.clone();
        let back_button_ref = back_button.clone();
        let status_label = status_label.clone();
        let sidebar_label = sidebar_label.clone();
        let breadcrumb_bar = breadcrumb_bar.clone();

        back_button.connect_clicked(move |_| {
            let mut s = state.borrow_mut();
            if let Some(entry) = s.history.pop() {
                s.tree = Some(entry.tree);
                s.rects = entry.rects;
                s.scan_root_path = entry.scan_root_path;
                s.view_root = 0;
                s.selected_indices.clear();
                s.hovered_index = None;
                back_button_ref.set_sensitive(!s.history.is_empty());

                let root_name = s.tree.as_ref()
                    .map(|t| t.root_node().name.clone())
                    .unwrap_or_default();
                let total_size = s.tree.as_ref().map(|t| t.total_size).unwrap_or(0);
                let file_count = s.tree.as_ref()
                    .map(|t| t.nodes.iter().filter(|n| !n.is_dir).count())
                    .unwrap_or(0);
                let dir_count = s.tree.as_ref()
                    .map(|t| t.nodes.iter().filter(|n| n.is_dir && n.depth > 0).count())
                    .unwrap_or(0);

                drop(s);

                status_label.set_label(&format!(
                    "{} files  \u{00b7}  {} dirs  \u{00b7}  {}",
                    file_count, dir_count, format_size(total_size),
                ));
                sidebar_label.set_label(&root_name);
                update_breadcrumb(&breadcrumb_bar, &root_name);
                treemap_area.queue_draw();
            }
        });
    }

    // --- Dir tree click handler ---
    {
        let start_scan = start_scan.clone();
        dir_tree_list.connect_row_activated(move |_, row| {
            let path = row.widget_name().to_string();
            if !path.is_empty() {
                start_scan(path);
            }
        });
    }

    // --- Populate initial filesystem roots in dir tree ---
    populate_dir_tree_roots(&dir_tree_list);

    // Show welcome screen on launch — user picks a directory from the
    // left sidebar or the welcome cards to start scanning.
    view_stack.set_visible_child_name("welcome");

    window
}

/// Build the welcome/landing page with quick-scan cards.
fn build_welcome_page() -> (gtk4::Box, Vec<gtk4::Button>) {
    let page = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    page.set_valign(gtk4::Align::Center);
    page.set_halign(gtk4::Align::Center);
    page.set_margin_top(48);
    page.set_margin_bottom(48);

    // App title + subtitle
    let title = gtk4::Label::new(Some("DiskMap"));
    title.add_css_class("title-1");
    title.add_css_class("accent-text");
    page.append(&title);

    let subtitle = gtk4::Label::new(Some("See where your disk space goes"));
    subtitle.add_css_class("title-4");
    subtitle.add_css_class("dim-label");
    subtitle.set_margin_bottom(24);
    page.append(&subtitle);

    // Quick scan cards
    let cards_label = gtk4::Label::new(Some("Select a location to scan"));
    cards_label.add_css_class("title-4");
    cards_label.add_css_class("dim-label");
    cards_label.set_margin_bottom(16);
    page.append(&cards_label);

    let cards_grid = gtk4::Grid::new();
    cards_grid.set_column_spacing(16);
    cards_grid.set_row_spacing(16);
    cards_grid.set_halign(gtk4::Align::Center);

    let home_dir = std::env::var("HOME").unwrap_or_else(|_| "/home".into());
    let quick_dirs: Vec<(&str, &str, String)> = vec![
        ("Home", "user-home-symbolic", home_dir.clone()),
        ("Root", "drive-harddisk-symbolic", "/".into()),
        ("Documents", "folder-documents-symbolic", format!("{home_dir}/Documents")),
        ("Downloads", "folder-download-symbolic", format!("{home_dir}/Downloads")),
        ("Projects", "folder-symbolic", format!("{home_dir}/Projects")),
        ("Temp", "folder-temp-symbolic", "/tmp".into()),
    ];

    let mut buttons: Vec<gtk4::Button> = Vec::new();
    let mut col = 0;
    let mut row = 0;
    for (label, icon, path) in &quick_dirs {
        if std::path::Path::new(path).exists() {
            let btn = build_scan_button(label, icon, path);
            cards_grid.attach(&btn, col, row, 1, 1);
            buttons.push(btn);
            col += 1;
            if col >= 3 {
                col = 0;
                row += 1;
            }
        }
    }

    page.append(&cards_grid);

    let hint = gtk4::Label::new(Some("Or use the sidebar to browse directories"));
    hint.add_css_class("dim-label");
    hint.add_css_class("caption");
    hint.set_margin_top(20);
    page.append(&hint);

    (page, buttons)
}

#[allow(dead_code)]
fn build_loading_page() -> (gtk4::Box, gtk4::Label, gtk4::Label) {
    let page = gtk4::Box::new(gtk4::Orientation::Vertical, 16);
    page.set_valign(gtk4::Align::Center);
    page.set_halign(gtk4::Align::Center);

    // Animated hard drive with legs — drawn on a canvas
    let canvas = gtk4::DrawingArea::new();
    canvas.set_size_request(200, 160);

    let frame_counter = Rc::new(RefCell::new(0u32));

    {
        let frame_counter = frame_counter.clone();
        canvas.set_draw_func(move |_, cr, width, height| {
            let frame = *frame_counter.borrow();
            let cx = width as f64 / 2.0;
            let cy = height as f64 / 2.0 - 10.0;

            // Bounce offset
            let bounce = (frame as f64 * 0.3).sin().abs() * 8.0;
            let tilt = (frame as f64 * 0.3).sin() * 0.08;

            let _ = cr.save();
            cr.translate(cx, cy - bounce);
            cr.rotate(tilt);

            // --- Hard drive body ---
            // Shadow
            cr.set_source_rgba(0.0, 0.0, 0.0, 0.2);
            let shadow_y = 20.0 + bounce;
            cr.rectangle(-40.0, shadow_y, 80.0, 8.0);
            let _ = cr.fill();

            // Main body (rounded rect)
            rounded_rect(cr, -38.0, -25.0, 76.0, 50.0, 8.0);
            cr.set_source_rgb(0.35, 0.40, 0.50); // Metal gray-blue
            let _ = cr.fill_preserve();
            cr.set_source_rgb(0.25, 0.30, 0.38);
            cr.set_line_width(2.0);
            let _ = cr.stroke();

            // Drive label stripe
            cr.set_source_rgb(0.42, 0.53, 0.70); // Accent blue
            rounded_rect(cr, -30.0, -18.0, 60.0, 14.0, 3.0);
            let _ = cr.fill();

            // LED indicator (blinks)
            if frame % 8 < 5 {
                cr.set_source_rgb(0.3, 0.9, 0.4); // Green blink
            } else {
                cr.set_source_rgb(0.15, 0.35, 0.2); // Dim green
            }
            cr.arc(28.0, 12.0, 3.0, 0.0, std::f64::consts::TAU);
            let _ = cr.fill();

            // Ventilation lines
            cr.set_source_rgba(0.2, 0.24, 0.30, 0.8);
            for i in 0..3 {
                let y = 2.0 + i as f64 * 5.0;
                cr.rectangle(-20.0, y, 30.0, 1.5);
                let _ = cr.fill();
            }

            // --- Legs! ---
            let leg_phase = frame as f64 * 0.4;

            // Left leg
            let left_foot_x = -18.0 + (leg_phase).sin() * 12.0;
            let left_knee_y = 25.0 + 5.0;
            let left_foot_y = 25.0 + 18.0 + (leg_phase).cos().abs() * 4.0;
            cr.set_source_rgb(0.42, 0.53, 0.70);
            cr.set_line_width(3.5);
            cr.set_line_cap(gtk4::cairo::LineCap::Round);
            cr.move_to(-12.0, 25.0);
            cr.line_to(left_foot_x - 3.0, left_knee_y);
            cr.line_to(left_foot_x, left_foot_y);
            let _ = cr.stroke();
            // Shoe
            cr.set_source_rgb(0.9, 0.35, 0.3); // Red shoes!
            cr.arc(left_foot_x + 2.0, left_foot_y, 4.0, 0.0, std::f64::consts::TAU);
            let _ = cr.fill();

            // Right leg (opposite phase)
            let right_foot_x = 18.0 + (leg_phase + std::f64::consts::PI).sin() * 12.0;
            let right_knee_y = 25.0 + 5.0;
            let right_foot_y =
                25.0 + 18.0 + (leg_phase + std::f64::consts::PI).cos().abs() * 4.0;
            cr.set_source_rgb(0.42, 0.53, 0.70);
            cr.set_line_width(3.5);
            cr.move_to(12.0, 25.0);
            cr.line_to(right_foot_x + 3.0, right_knee_y);
            cr.line_to(right_foot_x, right_foot_y);
            let _ = cr.stroke();
            // Shoe
            cr.set_source_rgb(0.9, 0.35, 0.3);
            cr.arc(right_foot_x - 2.0, right_foot_y, 4.0, 0.0, std::f64::consts::TAU);
            let _ = cr.fill();

            // --- Eyes ---
            // Left eye
            cr.set_source_rgb(1.0, 1.0, 1.0);
            cr.arc(-12.0, -12.0, 5.0, 0.0, std::f64::consts::TAU);
            let _ = cr.fill();
            // Pupil (looks in running direction)
            let look_x = (leg_phase).sin() * 2.0;
            cr.set_source_rgb(0.1, 0.1, 0.15);
            cr.arc(-12.0 + look_x, -12.0, 2.5, 0.0, std::f64::consts::TAU);
            let _ = cr.fill();

            // Right eye
            cr.set_source_rgb(1.0, 1.0, 1.0);
            cr.arc(12.0, -12.0, 5.0, 0.0, std::f64::consts::TAU);
            let _ = cr.fill();
            cr.set_source_rgb(0.1, 0.1, 0.15);
            cr.arc(12.0 + look_x, -12.0, 2.5, 0.0, std::f64::consts::TAU);
            let _ = cr.fill();

            let _ = cr.restore();

            // --- Dust puffs behind the running drive ---
            let puff_x = cx - 50.0 - (frame as f64 * 0.2).sin() * 10.0;
            let puff_alpha = 0.15 + (frame as f64 * 0.15).sin().abs() * 0.1;
            cr.set_source_rgba(0.6, 0.6, 0.6, puff_alpha);
            cr.arc(puff_x, cy + 30.0 - bounce / 2.0, 6.0 + (frame % 10) as f64 * 0.5, 0.0, std::f64::consts::TAU);
            let _ = cr.fill();
            cr.arc(puff_x - 15.0, cy + 28.0, 4.0 + (frame % 7) as f64 * 0.3, 0.0, std::f64::consts::TAU);
            let _ = cr.fill();
        });
    }

    // Animation timer
    {
        let frame_counter = frame_counter.clone();
        let canvas_weak = canvas.downgrade();
        glib::timeout_add_local(std::time::Duration::from_millis(50), move || {
            *frame_counter.borrow_mut() += 1;
            if let Some(canvas) = canvas_weak.upgrade() {
                canvas.queue_draw();
                glib::ControlFlow::Continue
            } else {
                glib::ControlFlow::Break
            }
        });
    }

    page.append(&canvas);

    // Progress text (counts)
    let progress_label = gtk4::Label::new(Some("Getting ready..."));
    progress_label.add_css_class("title-4");
    progress_label.add_css_class("dim-label");
    page.append(&progress_label);

    // Current file path label
    let path_label = gtk4::Label::new(None);
    path_label.add_css_class("caption");
    path_label.add_css_class("dim-label");
    path_label.set_ellipsize(gtk4::pango::EllipsizeMode::Middle);
    path_label.set_max_width_chars(80);
    path_label.set_opacity(0.5);
    page.append(&path_label);

    // Spinner as backup visual
    let spinner = gtk4::Spinner::new();
    spinner.start();
    spinner.set_size_request(24, 24);
    page.append(&spinner);

    (page, progress_label, path_label)
}

/// Helper to draw a rounded rectangle with Cairo.
#[allow(dead_code)]
fn rounded_rect(cr: &gtk4::cairo::Context, x: f64, y: f64, w: f64, h: f64, r: f64) {
    cr.new_sub_path();
    cr.arc(x + w - r, y + r, r, -std::f64::consts::FRAC_PI_2, 0.0);
    cr.arc(x + w - r, y + h - r, r, 0.0, std::f64::consts::FRAC_PI_2);
    cr.arc(x + r, y + h - r, r, std::f64::consts::FRAC_PI_2, std::f64::consts::PI);
    cr.arc(x + r, y + r, r, std::f64::consts::PI, 3.0 * std::f64::consts::FRAC_PI_2);
    cr.close_path();
}

/// Build a scan button for the welcome page.
fn build_scan_button(label: &str, icon_name: &str, path: &str) -> gtk4::Button {
    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
    content.set_margin_top(16);
    content.set_margin_bottom(12);
    content.set_margin_start(20);
    content.set_margin_end(20);
    content.set_halign(gtk4::Align::Center);

    let icon = gtk4::Image::from_icon_name(icon_name);
    icon.set_pixel_size(32);
    content.append(&icon);

    let name_label = gtk4::Label::new(Some(label));
    name_label.add_css_class("heading");
    content.append(&name_label);

    let path_label = gtk4::Label::new(Some(path));
    path_label.add_css_class("caption");
    path_label.add_css_class("dim-label");
    path_label.set_ellipsize(gtk4::pango::EllipsizeMode::Middle);
    path_label.set_max_width_chars(20);
    content.append(&path_label);

    let btn = gtk4::Button::new();
    btn.set_child(Some(&content));
    btn.add_css_class("flat");
    btn.add_css_class("scan-card");
    btn.set_widget_name(path);
    btn
}

/// Wire up click handlers on welcome page cards.

/// Update the breadcrumb bar.
fn update_breadcrumb(breadcrumb_bar: &gtk4::Box, root_name: &str) {
    // Clear existing children
    while let Some(child) = breadcrumb_bar.first_child() {
        breadcrumb_bar.remove(&child);
    }

    let label = gtk4::Label::new(Some(root_name));
    label.add_css_class("heading");
    label.add_css_class("accent-text");
    breadcrumb_bar.append(&label);
}

/// Populate the dir tree sidebar with filesystem root locations.
fn populate_dir_tree_roots(list: &gtk4::ListBox) {
    // Clear existing
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }

    let home = std::env::var("HOME").unwrap_or_else(|_| "/home".into());

    // Root entries: system + common user dirs
    let entries: Vec<(&str, &str, String)> = vec![
        ("Root", "drive-harddisk-symbolic", "/".into()),
        ("Home", "user-home-symbolic", home.clone()),
        ("Documents", "folder-documents-symbolic", format!("{home}/Documents")),
        ("Downloads", "folder-download-symbolic", format!("{home}/Downloads")),
        ("Desktop", "user-desktop-symbolic", format!("{home}/Desktop")),
        ("Pictures", "folder-pictures-symbolic", format!("{home}/Pictures")),
        ("Music", "folder-music-symbolic", format!("{home}/Music")),
        ("Videos", "folder-videos-symbolic", format!("{home}/Videos")),
        ("Projects", "folder-symbolic", format!("{home}/Projects")),
        ("/tmp", "folder-temp-symbolic", "/tmp".into()),
        ("/var", "folder-symbolic", "/var".into()),
        ("/usr", "folder-symbolic", "/usr".into()),
    ];

    for (name, icon, path) in &entries {
        if std::path::Path::new(path).exists() {
            let row = build_dir_tree_row(name, icon, path);
            list.append(&row);
        }
    }
}

/// Populate the dir tree with quick-access roots + current directory contents.
fn populate_dir_tree(list: &gtk4::ListBox, _tree: &dm_core::model::FileTree, _scan_path: &str) {
    // Clear existing
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }

    // Always show quick-access roots at the top
    populate_dir_tree_roots(list);

    // Only show roots — subdirectories are in the pie chart
}

/// Build a clickable directory row for the tree.
fn build_dir_tree_row(name: &str, icon_name: &str, path: &str) -> gtk4::ListBoxRow {
    let hbox = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    hbox.set_margin_start(8);
    hbox.set_margin_end(8);
    hbox.set_margin_top(4);
    hbox.set_margin_bottom(4);

    let icon = gtk4::Image::from_icon_name(icon_name);
    icon.set_pixel_size(16);
    hbox.append(&icon);

    let label = gtk4::Label::new(Some(name));
    label.set_halign(gtk4::Align::Start);
    label.set_hexpand(true);
    label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    hbox.append(&label);

    let row = gtk4::ListBoxRow::new();
    row.set_child(Some(&hbox));
    row.set_widget_name(path);
    row
}

/// Build a directory row with size info.
#[allow(dead_code)]
fn build_dir_tree_row_with_size(name: &str, icon_name: &str, path: &str, size: &str) -> gtk4::ListBoxRow {
    let hbox = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    hbox.set_margin_start(8);
    hbox.set_margin_end(8);
    hbox.set_margin_top(3);
    hbox.set_margin_bottom(3);

    let icon = gtk4::Image::from_icon_name(icon_name);
    icon.set_pixel_size(16);
    hbox.append(&icon);

    let label = gtk4::Label::new(Some(name));
    label.set_halign(gtk4::Align::Start);
    label.set_hexpand(true);
    label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    hbox.append(&label);

    let size_label = gtk4::Label::new(Some(size));
    size_label.add_css_class("caption");
    size_label.add_css_class("dim-label");
    hbox.append(&size_label);

    let row = gtk4::ListBoxRow::new();
    row.set_child(Some(&hbox));
    row.set_widget_name(path);
    row
}

/// Build a non-clickable file row (display only).
#[allow(dead_code)]
fn build_file_tree_row(name: &str, _icon_name: &str, size: &str) -> gtk4::Label {
    let text = format!("  {}  {}", name, size);
    let label = gtk4::Label::new(Some(&text));
    label.set_halign(gtk4::Align::Start);
    label.set_margin_start(28);
    label.set_margin_top(1);
    label.set_margin_bottom(1);
    label.add_css_class("caption");
    label.add_css_class("dim-label");
    label.set_ellipsize(gtk4::pango::EllipsizeMode::Middle);
    label
}

/// Load custom CSS for the modern look.
fn load_css() {
    let css = gtk4::CssProvider::new();
    css.load_from_data(
        r#"
        /* Accent colors */
        .accent-text {
            color: #6C9FFF;
        }
        .accent-icon {
            color: #6C9FFF;
        }

        /* Scan card buttons */
        .scan-card {
            border-radius: 12px;
            min-width: 140px;
            transition: all 200ms ease;
            background: alpha(white, 0.04);
            border: 1px solid alpha(white, 0.08);
        }
        .scan-card:hover {
            background: alpha(@accent_color, 0.12);
            border-color: alpha(@accent_color, 0.3);
        }

        /* Breadcrumb bar */
        .breadcrumb-bar {
            background: alpha(white, 0.04);
            border-radius: 6px;
            padding: 4px 8px;
        }

        /* Progress bar styling */
        progressbar.osd trough {
            min-height: 4px;
        }
        progressbar.osd progress {
            min-height: 4px;
            background: #6C9FFF;
            border-radius: 2px;
        }

        /* Dir tree panel */
        .dir-tree-panel {
            background: alpha(white, 0.02);
            border-right: 1px solid alpha(white, 0.06);
        }

        /* Status bar */
        .status-bar {
            background: alpha(white, 0.02);
            border-top: 1px solid alpha(white, 0.06);
        }
        "#,
    );

    gtk4::style_context_add_provider_for_display(
        &gdk::Display::default().expect("display"),
        &css,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}

pub(crate) fn format_size(bytes: u64) -> String {
    const KIB: u64 = 1024;
    const MIB: u64 = 1024 * KIB;
    const GIB: u64 = 1024 * MIB;
    const TIB: u64 = 1024 * GIB;

    if bytes >= TIB {
        format!("{:.1} TiB", bytes as f64 / TIB as f64)
    } else if bytes >= GIB {
        format!("{:.1} GiB", bytes as f64 / GIB as f64)
    } else if bytes >= MIB {
        format!("{:.1} MiB", bytes as f64 / MIB as f64)
    } else if bytes >= KIB {
        format!("{:.1} KiB", bytes as f64 / KIB as f64)
    } else {
        format!("{bytes} B")
    }
}
