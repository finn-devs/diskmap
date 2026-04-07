use crate::window::AppState;
use gtk4::prelude::*;
use gtk4::{self, gdk, gio};
use std::cell::RefCell;
use std::rc::Rc;

/// A slice in the pie chart for hit-testing.
#[derive(Clone)]
struct PieSlice {
    node_index: u32,
    start_angle: f64, // radians
    end_angle: f64,
    r: f64,
    g: f64,
    b: f64,
    name: String,
    size: u64,
    is_dir: bool,
}

/// Create the pie chart drawing area with all event handlers.
pub fn create_treemap_widget(
    state: Rc<RefCell<AppState>>,
    _back_button: gtk4::Button,
    start_scan: Rc<dyn Fn(String)>,
) -> gtk4::DrawingArea {
    let area = gtk4::DrawingArea::new();
    area.set_hexpand(true);
    area.set_vexpand(true);

    let slices: Rc<RefCell<Vec<PieSlice>>> = Rc::new(RefCell::new(Vec::new()));

    // --- Draw function ---
    {
        let state = state.clone();
        let slices = slices.clone();

        area.set_draw_func(move |_area, cr, width, height| {
            let s = state.borrow();

            // Dark background
            cr.set_source_rgb(0.12, 0.12, 0.14);
            cr.rectangle(0.0, 0.0, width as f64, height as f64);
            let _ = cr.fill();

            let tree = match &s.tree {
                Some(t) => t,
                None => return,
            };

            let root = tree.root_node();
            if root.size == 0 || root.children.is_empty() {
                cr.set_source_rgba(1.0, 1.0, 1.0, 0.5);
                cr.set_font_size(16.0);
                let _ = cr.move_to(width as f64 / 2.0 - 50.0, height as f64 / 2.0);
                let _ = cr.show_text("Empty directory");
                return;
            }

            // Compute pie geometry
            let cx = width as f64 / 2.0;
            let cy = height as f64 / 2.0;
            let radius = (cx.min(cy) - 80.0).max(50.0);
            let inner_radius = radius * 0.35; // Donut hole
            let total_size = root.size as f64;

            // Build slices from children
            let children: Vec<_> = root.children.clone()
                .filter_map(|i| tree.nodes.get(i as usize).map(|n| (i, n)))
                .collect();

            // Sort by size descending for visual clarity
            let mut sorted: Vec<_> = children.iter()
                .filter(|(_, n)| n.size > 0)
                .collect();
            sorted.sort_by(|a, b| b.1.size.cmp(&a.1.size));

            // Find max size for heat-map coloring
            let max_size = sorted.first().map(|(_, n)| n.size).unwrap_or(1) as f64;

            let mut new_slices: Vec<PieSlice> = Vec::new();
            let mut angle = -std::f64::consts::FRAC_PI_2; // Start at top (12 o'clock)

            // Use square root scale for slice sizes so small items remain visible.
            // This spreads the visual space much more than linear:
            // 4GB vs 50MB: linear = 80:1, sqrt = 9:1, so small dirs are clearly visible.
            // Each item also gets a minimum fraction so nothing disappears entirely.
            // Use sqrt scale + minimum floor, then renormalize to sum to 1.0
            let item_count = sorted.len() as f64;
            let min_visual = if item_count > 0.0 { 0.5 / item_count } else { 0.0 };
            let sqrt_sizes: Vec<f64> = sorted.iter()
                .map(|(_, n)| (n.size as f64).sqrt())
                .collect();
            let sqrt_total: f64 = sqrt_sizes.iter().sum();
            let raw_fractions: Vec<f64> = sqrt_sizes.iter()
                .map(|s| if sqrt_total > 0.0 { (s / sqrt_total).max(min_visual) } else { 0.0 })
                .collect();
            let adjusted_total: f64 = raw_fractions.iter().sum();

            for (i, &(idx, node)) in sorted.iter().enumerate() {
                let fraction = if adjusted_total > 0.0 { raw_fractions[i] / adjusted_total } else { 0.0 };
                let sweep = fraction * std::f64::consts::TAU;
                let end_angle = angle + sweep;

                // Heat-map color based on relative size (log scale)
                let ratio = (node.size as f64).ln_1p() / max_size.ln_1p();
                let t = ratio.clamp(0.0, 1.0);
                let (r, g, b) = heatmap_color(t);

                new_slices.push(PieSlice {
                    node_index: *idx,
                    start_angle: angle,
                    end_angle,
                    r: r as f64,
                    g: g as f64,
                    b: b as f64,
                    name: node.name.clone(),
                    size: node.size,
                    is_dir: node.is_dir,
                });

                angle = end_angle;
            }

            // Draw slices
            for slice in new_slices.iter() {
                let is_hovered = s.hovered_index == Some(slice.node_index);
                let is_selected = s.selected_indices.contains(&slice.node_index);

                // Explode hovered slice outward slightly
                let explode = if is_hovered { 8.0 } else { 0.0 };
                let mid_angle = (slice.start_angle + slice.end_angle) / 2.0;
                let ex = explode * mid_angle.cos();
                let ey = explode * mid_angle.sin();

                // Draw filled arc (donut slice)
                cr.new_path();
                cr.arc(cx + ex, cy + ey, radius, slice.start_angle, slice.end_angle);
                cr.arc_negative(cx + ex, cy + ey, inner_radius, slice.end_angle, slice.start_angle);
                cr.close_path();

                // Fill
                if is_selected {
                    cr.set_source_rgba(slice.r, slice.g, slice.b, 1.0);
                } else if is_hovered {
                    cr.set_source_rgba(
                        (slice.r + 0.15).min(1.0),
                        (slice.g + 0.15).min(1.0),
                        (slice.b + 0.15).min(1.0),
                        1.0,
                    );
                } else {
                    cr.set_source_rgba(slice.r, slice.g, slice.b, 0.85);
                }
                let _ = cr.fill_preserve();

                // Border between slices
                cr.set_source_rgba(0.12, 0.12, 0.14, 1.0);
                cr.set_line_width(2.0);
                let _ = cr.stroke();

                // Selection ring
                if is_selected {
                    cr.new_path();
                    cr.arc(cx + ex, cy + ey, radius + 3.0, slice.start_angle, slice.end_angle);
                    cr.set_source_rgba(0.4, 0.7, 1.0, 0.9);
                    cr.set_line_width(3.0);
                    let _ = cr.stroke();
                }

                // Label line + text for slices big enough
                let fraction = slice.size as f64 / total_size;
                if fraction > 0.02 {
                    let label_radius = radius + 20.0;
                    let lx = cx + label_radius * mid_angle.cos() + ex;
                    let ly = cy + label_radius * mid_angle.sin() + ey;

                    // Label text
                    let icon = if slice.is_dir { "\u{1f4c1} " } else { "" };
                    // Show real (linear) percentage, not the log-scaled visual fraction
                    let real_pct = (slice.size as f64 / total_size * 100.0) as u32;
                    let label = format!("{}{} ({}%)", icon, slice.name, real_pct);
                    let size_label = format_size_short(slice.size);

                    cr.set_source_rgba(1.0, 1.0, 1.0, 0.9);
                    cr.set_font_size(11.0);

                    // Align text based on which side of the pie
                    if mid_angle.cos() >= 0.0 {
                        let _ = cr.move_to(lx + 4.0, ly);
                    } else {
                        // Approximate text width for right-alignment
                        let text_width = label.len() as f64 * 6.5;
                        let _ = cr.move_to(lx - text_width - 4.0, ly);
                    }
                    let _ = cr.show_text(&label);

                    // Size below name
                    cr.set_source_rgba(1.0, 1.0, 1.0, 0.5);
                    cr.set_font_size(9.0);
                    if mid_angle.cos() >= 0.0 {
                        let _ = cr.move_to(lx + 4.0, ly + 14.0);
                    } else {
                        let text_width = size_label.len() as f64 * 5.5;
                        let _ = cr.move_to(lx - text_width - 4.0, ly + 14.0);
                    }
                    let _ = cr.show_text(&size_label);

                    // Line from slice to label
                    let line_start_r = radius + 4.0;
                    let lsx = cx + line_start_r * mid_angle.cos() + ex;
                    let lsy = cy + line_start_r * mid_angle.sin() + ey;
                    cr.set_source_rgba(1.0, 1.0, 1.0, 0.2);
                    cr.set_line_width(1.0);
                    cr.move_to(lsx, lsy);
                    cr.line_to(lx, ly);
                    let _ = cr.stroke();
                }
            }

            // Center text — total size
            cr.set_source_rgba(1.0, 1.0, 1.0, 0.9);
            cr.set_font_size(18.0);
            let total_text = format_size_short(root.size);
            let te = cr.text_extents(&total_text).unwrap();
            let _ = cr.move_to(cx - te.width() / 2.0, cy + 6.0);
            let _ = cr.show_text(&total_text);

            // Center sub-text — item count
            let count = sorted.len();
            let count_text = format!("{} items", count);
            cr.set_source_rgba(1.0, 1.0, 1.0, 0.4);
            cr.set_font_size(11.0);
            let te2 = cr.text_extents(&count_text).unwrap();
            let _ = cr.move_to(cx - te2.width() / 2.0, cy + 22.0);
            let _ = cr.show_text(&count_text);

            // Store slices for hit-testing
            *slices.borrow_mut() = new_slices;
        });
    }

    // --- Mouse hover ---
    {
        let state = state.clone();
        let slices = slices.clone();
        let area_weak = area.downgrade();
        let motion = gtk4::EventControllerMotion::new();
        motion.connect_motion(move |_, x, y| {
            let area = match area_weak.upgrade() {
                Some(a) => a,
                None => return,
            };
            let cx = area.width() as f64 / 2.0;
            let cy = area.height() as f64 / 2.0;
            let hit = pie_hit_test(&slices.borrow(), x, y, cx, cy);

            let mut s = state.borrow_mut();
            if s.hovered_index != hit {
                s.hovered_index = hit;
                area.queue_draw();
            }
        });
        area.add_controller(motion);
    }

    // --- Click to drill down / select ---
    {
        let state = state.clone();
        let slices = slices.clone();
        let area_weak = area.downgrade();
        let start_scan = start_scan.clone();
        let click = gtk4::GestureClick::new();
        click.set_button(gdk::BUTTON_PRIMARY);
        click.connect_released(move |gesture, n_press, x, y| {
            let area = match area_weak.upgrade() {
                Some(a) => a,
                None => return,
            };
            let cx = area.width() as f64 / 2.0;
            let cy = area.height() as f64 / 2.0;

            let hit_idx = match pie_hit_test(&slices.borrow(), x, y, cx, cy) {
                Some(idx) => idx,
                None => return,
            };

            let mut s = state.borrow_mut();
            let modifiers = gesture.current_event_state();
            let is_ctrl = modifiers.contains(gdk::ModifierType::CONTROL_MASK);
            let is_shift = modifiers.contains(gdk::ModifierType::SHIFT_MASK);

            if is_ctrl || is_shift {
                if let Some(pos) = s.selected_indices.iter().position(|&i| i == hit_idx) {
                    s.selected_indices.remove(pos);
                } else {
                    s.selected_indices.push(hit_idx);
                }
                area.queue_draw();
            } else if n_press == 2 {
                // Double-click: drill into directory
                let is_dir = s.tree.as_ref()
                    .map(|t| t.node(hit_idx).is_dir)
                    .unwrap_or(false);
                if is_dir {
                    let node_name = s.tree.as_ref()
                        .map(|t| t.node(hit_idx).name.clone())
                        .unwrap_or_default();
                    let dir_path = format!("{}/{}", s.scan_root_path, node_name);

                    // Save current state for back navigation
                    if let Some(tree) = s.tree.take() {
                        let rects = std::mem::take(&mut s.rects);
                        let path = std::mem::take(&mut s.scan_root_path);
                        s.history.push(crate::window::HistoryEntry {
                            tree,
                            rects,
                            scan_root_path: path,
                        });
                    }
                    s.selected_indices.clear();
                    drop(s);
                    start_scan(dir_path);
                }
            } else {
                // Single click: select
                s.selected_indices = vec![hit_idx];
                area.queue_draw();
            }
        });
        area.add_controller(click);
    }

    // --- Right-click context menu ---
    {
        let state = state.clone();
        let slices = slices.clone();
        let area_ref = area.clone();

        let right_click = gtk4::GestureClick::new();
        right_click.set_button(gdk::BUTTON_SECONDARY);
        right_click.connect_released(move |_, _, x, y| {
            let cx = area_ref.width() as f64 / 2.0;
            let cy = area_ref.height() as f64 / 2.0;

            let hit_idx = match pie_hit_test(&slices.borrow(), x, y, cx, cy) {
                Some(idx) => idx,
                None => return,
            };

            let s = state.borrow();
            let (node_path, node_is_dir): (String, bool) = match &s.tree {
                Some(tree) => {
                    let node = tree.node(hit_idx);
                    (
                        format!("{}/{}", s.scan_root_path, node.name),
                        node.is_dir,
                    )
                }
                None => return,
            };
            drop(s);

            let menu = gio::Menu::new();
            if node_is_dir {
                menu.append(Some("Open in File Manager"), Some("ctx.open-fm"));
            }
            menu.append(Some("Copy Path"), Some("ctx.copy-path"));
            menu.append(Some("Move to Trash"), Some("ctx.trash"));

            let popover = gtk4::PopoverMenu::from_model(Some(&menu));
            popover.set_parent(&area_ref);
            popover.set_pointing_to(Some(&gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
            popover.set_has_arrow(true);

            let action_group = gio::SimpleActionGroup::new();

            let path_for_fm = node_path.clone();
            let open_fm = gio::SimpleAction::new("open-fm", None);
            open_fm.connect_activate(move |_, _| {
                let file = gio::File::for_path(&path_for_fm);
                let uri = file.uri();
                if gio::AppInfo::launch_default_for_uri(&uri, gio::AppLaunchContext::NONE).is_err() {
                    let managers = ["xdg-open", "nautilus", "thunar", "dolphin", "pcmanfm", "nemo", "caja"];
                    for fm in &managers {
                        if std::process::Command::new(fm).arg(&path_for_fm).spawn().is_ok() {
                            break;
                        }
                    }
                }
            });
            action_group.add_action(&open_fm);

            let path_for_copy = node_path.clone();
            let area_for_clip = area_ref.clone();
            let copy_path = gio::SimpleAction::new("copy-path", None);
            copy_path.connect_activate(move |_, _| {
                if let Some(display) = area_for_clip.display().into() {
                    let clipboard: gdk::Clipboard = display.clipboard();
                    clipboard.set_text(&path_for_copy);
                }
            });
            action_group.add_action(&copy_path);

            let path_for_trash = node_path.clone();
            let trash = gio::SimpleAction::new("trash", None);
            trash.connect_activate(move |_, _| {
                let file = gio::File::for_path(&path_for_trash);
                if let Err(e) = file.trash(gio::Cancellable::NONE) {
                    eprintln!("Failed to trash {}: {}", path_for_trash, e);
                }
            });
            action_group.add_action(&trash);

            popover.insert_action_group("ctx", Some(&action_group));
            popover.popup();
        });
        area.add_controller(right_click);
    }

    // --- Mouse back/forward buttons (8=back, 9=forward) ---
    {
        let state = state.clone();
        let area_weak = area.downgrade();
        let back_click = gtk4::GestureClick::new();
        back_click.set_button(8);
        back_click.connect_released(move |_, _, _, _| {
            let mut s = state.borrow_mut();
            if let Some(entry) = s.history.pop() {
                s.tree = Some(entry.tree);
                s.rects = entry.rects;
                s.scan_root_path = entry.scan_root_path;
                s.view_root = 0;
                s.selected_indices.clear();
                s.hovered_index = None;
                drop(s);
                if let Some(area) = area_weak.upgrade() {
                    area.queue_draw();
                }
            }
        });
        area.add_controller(back_click);
    }

    area
}

// --- Hit testing ---

fn pie_hit_test(slices: &[PieSlice], x: f64, y: f64, cx: f64, cy: f64) -> Option<u32> {
    let dx = x - cx;
    let dy = y - cy;
    let dist = (dx * dx + dy * dy).sqrt();

    // Check if within the donut ring
    let radius = (cx.min(cy) - 80.0).max(50.0);
    let inner_radius = radius * 0.35;

    if dist < inner_radius || dist > radius + 10.0 {
        return None;
    }

    // Compute angle
    let angle = dy.atan2(dx);
    // Normalize to match our start at -PI/2
    // atan2 returns -PI..PI, we need to compare with our slice angles

    for slice in slices {
        // Normalize the test angle to be within [start_angle, start_angle + TAU)
        let mut test = angle;
        while test < slice.start_angle {
            test += std::f64::consts::TAU;
        }
        while test > slice.start_angle + std::f64::consts::TAU {
            test -= std::f64::consts::TAU;
        }

        if test >= slice.start_angle && test < slice.end_angle {
            return Some(slice.node_index);
        }
    }

    None
}

// --- Color ---

fn heatmap_color(t: f64) -> (f32, f32, f32) {
    let (r, g, b) = if t < 0.2 {
        let s = t / 0.2;
        lerp3((0.30, 0.50, 0.80), (0.15, 0.65, 0.65), s)
    } else if t < 0.4 {
        let s = (t - 0.2) / 0.2;
        lerp3((0.15, 0.65, 0.65), (0.30, 0.75, 0.35), s)
    } else if t < 0.6 {
        let s = (t - 0.4) / 0.2;
        lerp3((0.30, 0.75, 0.35), (0.90, 0.80, 0.25), s)
    } else if t < 0.8 {
        let s = (t - 0.6) / 0.2;
        lerp3((0.90, 0.80, 0.25), (0.95, 0.55, 0.20), s)
    } else {
        let s = (t - 0.8) / 0.2;
        lerp3((0.95, 0.55, 0.20), (0.90, 0.25, 0.20), s)
    };
    (r, g, b)
}

fn lerp3(a: (f32, f32, f32), b: (f32, f32, f32), t: f64) -> (f32, f32, f32) {
    let t = t as f32;
    (
        a.0 + (b.0 - a.0) * t,
        a.1 + (b.1 - a.1) * t,
        a.2 + (b.2 - a.2) * t,
    )
}

fn format_size_short(bytes: u64) -> String {
    const KIB: u64 = 1024;
    const MIB: u64 = 1024 * KIB;
    const GIB: u64 = 1024 * MIB;

    if bytes >= GIB {
        format!("{:.1}G", bytes as f64 / GIB as f64)
    } else if bytes >= MIB {
        format!("{:.1}M", bytes as f64 / MIB as f64)
    } else if bytes >= KIB {
        format!("{:.0}K", bytes as f64 / KIB as f64)
    } else {
        format!("{bytes}B")
    }
}
