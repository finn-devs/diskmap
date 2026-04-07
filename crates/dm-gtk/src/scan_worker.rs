use dm_core::treemap::{self, TreemapRect};
use dm_scan::walker::{self, ScanProgress, ScanResult};
use gtk4::glib;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc;
use std::sync::Arc;

#[cfg(target_os = "linux")]
extern crate libc;

/// Pre-computed data ready for the UI to display.
pub struct ScanComplete {
    pub result: ScanResult,
    pub initial_rects: Vec<TreemapRect>,
}

/// Messages sent from the scan worker to the GTK main loop.
pub enum ScanMsg {
    Progress(ScanProgress),
    Complete(ScanComplete),
    Error(String),
}

/// Start an async single-directory scan on a background thread.
pub fn start_scan(path: String, on_message: impl Fn(ScanMsg) + 'static) {
    let (tx, rx) = mpsc::channel::<ScanMsg>();
    let cancel = Arc::new(AtomicBool::new(false));

    std::thread::Builder::new()
        .name("diskmap-scanner".into())
        .spawn(move || {
            #[cfg(target_os = "linux")]
            unsafe {
                libc::nice(10);
            }

            let tx_progress = tx.clone();
            match walker::scan_dir(&path, cancel, move |progress| {
                let _ = tx_progress.send(ScanMsg::Progress(progress));
            }) {
                Ok(result) => {
                    // Pre-compute layout — fast for single-level (tens of rects, not thousands)
                    let initial_rects = treemap::layout_treemap(
                        &result.tree,
                        result.tree.root,
                        1200.0,
                        800.0,
                        2, // Only 2 levels deep (root + children)
                    );

                    let _ = tx.send(ScanMsg::Complete(ScanComplete {
                        result,
                        initial_rects,
                    }));
                }
                Err(e) => {
                    let _ = tx.send(ScanMsg::Error(e.message));
                }
            }
        })
        .expect("failed to spawn scanner thread");

    // Poll at ~20fps, coalesce progress, deliver terminal immediately
    glib::timeout_add_local(std::time::Duration::from_millis(50), move || {
        let mut latest_progress: Option<ScanMsg> = None;

        loop {
            match rx.try_recv() {
                Ok(msg) => {
                    let is_terminal = matches!(msg, ScanMsg::Complete(_) | ScanMsg::Error(_));
                    if is_terminal {
                        if let Some(prog) = latest_progress.take() {
                            on_message(prog);
                        }
                        on_message(msg);
                        return glib::ControlFlow::Break;
                    }
                    latest_progress = Some(msg);
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    return glib::ControlFlow::Break;
                }
            }
        }

        if let Some(prog) = latest_progress {
            on_message(prog);
        }

        glib::ControlFlow::Continue
    });
}
