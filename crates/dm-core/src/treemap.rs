use crate::color;
use crate::model::FileTree;

/// A single rectangle in the treemap output.
///
/// `#[repr(C)]` so this can cross the FFI boundary as a contiguous array.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct TreemapRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub r: f32,
    pub g: f32,
    pub b: f32,
    /// Back-reference into `FileTree::nodes`.
    pub node_index: u32,
}

/// Viewport bounds for layout computation.
#[derive(Debug, Clone, Copy)]
struct Rect {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

impl Rect {
    fn shorter_side(&self) -> f32 {
        self.w.min(self.h)
    }

    fn area(&self) -> f32 {
        self.w * self.h
    }
}

/// An item to be laid out: its area (proportional to file size) and index.
#[derive(Debug, Clone, Copy)]
struct LayoutItem {
    area: f32,
    node_index: u32,
}

/// Compute a squarified treemap layout.
///
/// Pure function: tree + viewport in, flat rects out.
/// Both frontends call this with the same inputs and get identical results.
pub fn layout_treemap(
    tree: &FileTree,
    root: u32,
    viewport_w: f32,
    viewport_h: f32,
    max_depth: u16,
) -> Vec<TreemapRect> {
    let mut rects = Vec::new();
    let bounds = Rect {
        x: 0.0,
        y: 0.0,
        w: viewport_w,
        h: viewport_h,
    };

    let root_node = tree.node(root);
    if root_node.size == 0 || bounds.area() < 1.0 {
        return rects;
    }

    layout_node(tree, root, bounds, 0, max_depth, &mut rects);

    // Post-pass: assign heat-map colors based on relative size
    // Red = largest, orange → yellow → green → teal → blue = smallest
    apply_size_heatmap(tree, &mut rects);

    rects
}

/// Assign heat-map colors to rects based on file size relative to the largest.
/// Red (urgent/large) → Orange → Yellow → Green → Teal → Blue (small).
fn apply_size_heatmap(tree: &FileTree, rects: &mut [TreemapRect]) {
    if rects.is_empty() {
        return;
    }

    // Find max size among leaf nodes (files, not dirs)
    let max_size = rects
        .iter()
        .map(|r| tree.node(r.node_index).size)
        .max()
        .unwrap_or(1) as f64;

    if max_size == 0.0 {
        return;
    }

    for rect in rects.iter_mut() {
        let node = tree.node(rect.node_index);

        if node.denied {
            let c = color::DENIED_COLOR;
            rect.r = c.r;
            rect.g = c.g;
            rect.b = c.b;
            continue;
        }

        if node.is_dir && node.children.is_empty() {
            let c = color::DIR_COLOR;
            rect.r = c.r;
            rect.g = c.g;
            rect.b = c.b;
            continue;
        }

        // Normalized size 0.0 (smallest) → 1.0 (largest)
        // Use log scale so mid-size files aren't all blue
        let ratio = (node.size as f64).ln_1p() / max_size.ln_1p();
        let t = ratio.clamp(0.0, 1.0) as f32;

        // Heat map: red (1.0) → orange → yellow → green → teal → blue (0.0)
        let (r, g, b) = heatmap_color(t);
        rect.r = r;
        rect.g = g;
        rect.b = b;
    }
}

/// Map t (0.0 = cold/small, 1.0 = hot/large) to an RGB color.
/// Blue → Teal → Green → Yellow → Orange → Red
fn heatmap_color(t: f32) -> (f32, f32, f32) {
    // 5-stop gradient
    let (r, g, b) = if t < 0.2 {
        // Blue → Teal
        let s = t / 0.2;
        lerp3((0.30, 0.50, 0.80), (0.15, 0.65, 0.65), s)
    } else if t < 0.4 {
        // Teal → Green
        let s = (t - 0.2) / 0.2;
        lerp3((0.15, 0.65, 0.65), (0.30, 0.75, 0.35), s)
    } else if t < 0.6 {
        // Green → Yellow
        let s = (t - 0.4) / 0.2;
        lerp3((0.30, 0.75, 0.35), (0.90, 0.80, 0.25), s)
    } else if t < 0.8 {
        // Yellow → Orange
        let s = (t - 0.6) / 0.2;
        lerp3((0.90, 0.80, 0.25), (0.95, 0.55, 0.20), s)
    } else {
        // Orange → Red
        let s = (t - 0.8) / 0.2;
        lerp3((0.95, 0.55, 0.20), (0.90, 0.25, 0.20), s)
    };

    (r, g, b)
}

fn lerp3(a: (f32, f32, f32), b: (f32, f32, f32), t: f32) -> (f32, f32, f32) {
    (
        a.0 + (b.0 - a.0) * t,
        a.1 + (b.1 - a.1) * t,
        a.2 + (b.2 - a.2) * t,
    )
}

fn layout_node(
    tree: &FileTree,
    node_index: u32,
    bounds: Rect,
    depth: u16,
    max_depth: u16,
    out: &mut Vec<TreemapRect>,
) {
    let node = tree.node(node_index);

    if !node.is_dir || depth >= max_depth || node.children.is_empty() {
        // Leaf: emit a rect (color assigned in post-pass)
        out.push(TreemapRect {
            x: bounds.x,
            y: bounds.y,
            w: bounds.w,
            h: bounds.h,
            r: 0.5,
            g: 0.5,
            b: 0.5,
            node_index,
        });
        return;
    }

    // Collect children with their proportional areas, sorted descending by size
    let total_size = node.size as f64;
    let total_area = bounds.area() as f64;

    let mut items: Vec<LayoutItem> = node
        .children
        .clone()
        .filter_map(|i| {
            let child = tree.node(i);
            if child.size == 0 {
                return None;
            }
            let area = (child.size as f64 / total_size * total_area) as f32;
            if area < 4.0 {
                return None; // Skip sub-pixel items
            }
            Some(LayoutItem {
                area,
                node_index: i,
            })
        })
        .collect();

    items.sort_by(|a, b| b.area.partial_cmp(&a.area).unwrap_or(std::cmp::Ordering::Equal));

    if items.is_empty() {
        return;
    }

    // Squarify: lay out items into rows
    let child_rects = squarify(&items, bounds);

    // For each child, either recurse (directory) or emit rect
    for (item, rect) in items.iter().zip(child_rects.iter()) {
        let child = tree.node(item.node_index);
        if child.is_dir && !child.children.is_empty() && depth + 1 < max_depth {
            // Recurse into directory with padding for label
            let padding = 2.0;
            let inner = Rect {
                x: rect.x + padding,
                y: rect.y + padding,
                w: (rect.w - 2.0 * padding).max(0.0),
                h: (rect.h - 2.0 * padding).max(0.0),
            };
            if inner.area() >= 4.0 {
                layout_node(tree, item.node_index, inner, depth + 1, max_depth, out);
            }
        } else {
            out.push(TreemapRect {
                x: rect.x,
                y: rect.y,
                w: rect.w,
                h: rect.h,
                r: 0.5,
                g: 0.5,
                b: 0.5,
                node_index: item.node_index,
            });
        }
    }
}

/// Squarified treemap algorithm (Bruls, Huizing, van Wijk 2000).
///
/// Given items sorted descending by area and a bounding rectangle,
/// produces a rectangle for each item tiling the bounds.
fn squarify(items: &[LayoutItem], bounds: Rect) -> Vec<Rect> {
    let mut result = vec![
        Rect {
            x: 0.0,
            y: 0.0,
            w: 0.0,
            h: 0.0,
        };
        items.len()
    ];
    let mut remaining = bounds;
    let mut i = 0;

    while i < items.len() {
        let shorter = remaining.shorter_side();
        if shorter <= 0.0 {
            break;
        }

        // Build a row: add items as long as worst aspect ratio improves
        let row_start = i;
        let mut row_area = 0.0f32;

        loop {
            let candidate_area = row_area + items[i].area;
            let old_worst = if i > row_start {
                worst_ratio(&items[row_start..i], row_area, shorter)
            } else {
                f32::MAX
            };
            let new_worst = worst_ratio(&items[row_start..=i], candidate_area, shorter);

            if new_worst > old_worst && i > row_start {
                // Adding this item made it worse — finalize row without it
                break;
            }

            row_area = candidate_area;
            i += 1;

            if i >= items.len() {
                break;
            }
        }

        // Lay out the row along the shorter side
        let row = &items[row_start..i];
        remaining = layout_row(row, row_area, remaining, &mut result[row_start..i]);
    }

    result
}

/// Compute the worst aspect ratio for items in a row strip of width `w`.
fn worst_ratio(row: &[LayoutItem], total_area: f32, w: f32) -> f32 {
    if w <= 0.0 || total_area <= 0.0 {
        return f32::MAX;
    }
    let w2 = w * w;
    let s2 = total_area * total_area;

    let mut worst = 0.0f32;
    for item in row {
        let r = (w2 * item.area / s2).max(s2 / (w2 * item.area));
        worst = worst.max(r);
    }
    worst
}

/// Lay out a row of items as a strip in the bounding rect.
/// Returns the remaining rect after consuming this row.
fn layout_row(
    row: &[LayoutItem],
    row_area: f32,
    bounds: Rect,
    out: &mut [Rect],
) -> Rect {
    let horizontal = bounds.w >= bounds.h;

    if horizontal {
        // Row is a vertical strip on the left side
        let strip_w = if bounds.h > 0.0 {
            row_area / bounds.h
        } else {
            0.0
        };
        let mut y = bounds.y;
        for (item, rect) in row.iter().zip(out.iter_mut()) {
            let h = if strip_w > 0.0 {
                item.area / strip_w
            } else {
                0.0
            };
            *rect = Rect {
                x: bounds.x,
                y,
                w: strip_w,
                h,
            };
            y += h;
        }
        Rect {
            x: bounds.x + strip_w,
            y: bounds.y,
            w: (bounds.w - strip_w).max(0.0),
            h: bounds.h,
        }
    } else {
        // Row is a horizontal strip on the top
        let strip_h = if bounds.w > 0.0 {
            row_area / bounds.w
        } else {
            0.0
        };
        let mut x = bounds.x;
        for (item, rect) in row.iter().zip(out.iter_mut()) {
            let w = if strip_h > 0.0 {
                item.area / strip_h
            } else {
                0.0
            };
            *rect = Rect {
                x,
                y: bounds.y,
                w,
                h: strip_h,
            };
            x += w;
        }
        Rect {
            x: bounds.x,
            y: bounds.y + strip_h,
            w: bounds.w,
            h: (bounds.h - strip_h).max(0.0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{FileNode, FileTree};

    fn make_tree(sizes: &[u64]) -> FileTree {
        let total: u64 = sizes.iter().sum();
        let child_start = 1u32;
        let child_end = 1 + sizes.len() as u32;

        let mut nodes = vec![FileNode {
            name: "root".into(),
            size: total,
            is_dir: true,
            extension: None,
            modified: None,
            children: child_start..child_end,
            depth: 0,
            denied: false,
        }];

        for (i, &size) in sizes.iter().enumerate() {
            nodes.push(FileNode {
                name: format!("file_{i}.dat"),
                size,
                is_dir: false,
                extension: Some("dat".into()),
                modified: None,
                children: 0..0,
                depth: 1,
                denied: false,
            });
        }

        FileTree {
            nodes,
            root: 0,
            total_size: total,
            denied_paths: vec![],
        }
    }

    #[test]
    fn rects_tile_viewport_exactly() {
        let tree = make_tree(&[6, 6, 4, 3, 2, 1]);
        let rects = layout_treemap(&tree, 0, 600.0, 400.0, 1);

        let viewport_area = 600.0 * 400.0;
        let rects_area: f32 = rects.iter().map(|r| r.w * r.h).sum();

        // Total rect area should equal viewport area (within float tolerance)
        assert!(
            (rects_area - viewport_area).abs() < viewport_area * 0.01,
            "rect area {rects_area} vs viewport {viewport_area}"
        );
    }

    #[test]
    fn no_overlapping_rects() {
        let tree = make_tree(&[100, 80, 60, 40, 20, 10, 5, 3, 2, 1]);
        let rects = layout_treemap(&tree, 0, 800.0, 600.0, 1);

        for (i, a) in rects.iter().enumerate() {
            for b in &rects[i + 1..] {
                let overlap_x = a.x < b.x + b.w && a.x + a.w > b.x;
                let overlap_y = a.y < b.y + b.h && a.y + a.h > b.y;
                // Allow tiny floating-point overlaps
                if overlap_x && overlap_y {
                    let ox = (a.x + a.w).min(b.x + b.w) - a.x.max(b.x);
                    let oy = (a.y + a.h).min(b.y + b.h) - a.y.max(b.y);
                    assert!(
                        ox * oy < 1.0,
                        "significant overlap between rect {i} and another"
                    );
                }
            }
        }
    }

    #[test]
    fn aspect_ratios_are_reasonable() {
        let tree = make_tree(&[6, 6, 4, 3, 2, 1]);
        let rects = layout_treemap(&tree, 0, 600.0, 400.0, 1);

        for r in &rects {
            if r.w > 1.0 && r.h > 1.0 {
                let aspect = r.w.max(r.h) / r.w.min(r.h);
                assert!(
                    aspect < 10.0,
                    "bad aspect ratio {aspect} for rect {}x{}",
                    r.w,
                    r.h
                );
            }
        }
    }

    #[test]
    fn empty_tree_returns_no_rects() {
        let tree = make_tree(&[]);
        let rects = layout_treemap(&tree, 0, 600.0, 400.0, 8);
        assert!(rects.is_empty());
    }

    #[test]
    fn single_file_fills_viewport() {
        let tree = make_tree(&[100]);
        let rects = layout_treemap(&tree, 0, 600.0, 400.0, 1);
        assert_eq!(rects.len(), 1);
        assert!((rects[0].w - 600.0).abs() < 1.0);
        assert!((rects[0].h - 400.0).abs() < 1.0);
    }
}
