//! C-ABI FFI layer for the DiskMap Swift frontend.
//!
//! All public functions are `extern "C"` with `#[unsafe(no_mangle)]`.
//! Memory ownership: Rust allocates, Rust frees. Every allocation
//! has a paired `_free` function.

use dm_core::model::FileTree;
use dm_core::treemap::{self, TreemapRect};
use dm_scan::walker::{self, ScanPhase, ScanProgress, ScanResult};
use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::Arc;
use std::thread::JoinHandle;

/// Opaque handle to an in-progress or completed scan.
pub struct DmScanHandle {
    tree: Option<FileTree>,
    denied_paths: Vec<String>,
    progress_rx: Option<Receiver<ScanProgress>>,
    join_handle: Option<JoinHandle<Result<ScanResult, walker::ScanError>>>,
    cancel: Arc<AtomicBool>,
    latest_progress: DmScanProgress,
}

/// Scan progress reported to the frontend.
#[repr(C)]
pub struct DmScanProgress {
    pub files_scanned: u64,
    pub dirs_scanned: u64,
    pub bytes_scanned: u64,
    pub is_complete: bool,
    pub has_error: bool,
}

/// Result of a treemap layout computation.
#[repr(C)]
pub struct DmTreemapResult {
    pub rects: *const TreemapRect,
    pub count: u64,
}

/// A C-compatible array of strings.
#[repr(C)]
pub struct DmStringArray {
    pub strings: *const *const c_char,
    pub count: u64,
}

// --- Scan lifecycle ---

/// Start an async scan. Returns an opaque handle.
///
/// # Safety
/// `path` must be a valid null-terminated UTF-8 C string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dm_scan_start(path: *const c_char) -> *mut DmScanHandle {
    let path_str = unsafe { CStr::from_ptr(path) }
        .to_str()
        .unwrap_or("")
        .to_owned();

    let (tx, rx) = mpsc::channel();
    let cancel = Arc::new(AtomicBool::new(false));
    let cancel_clone = cancel.clone();

    let join_handle = std::thread::spawn(move || {
        walker::scan_dir(&path_str, cancel_clone, move |progress| {
            let _ = tx.send(progress);
        })
    });

    Box::into_raw(Box::new(DmScanHandle {
        tree: None,
        denied_paths: Vec::new(),
        progress_rx: Some(rx),
        join_handle: Some(join_handle),
        cancel,
        latest_progress: DmScanProgress {
            files_scanned: 0,
            dirs_scanned: 0,
            bytes_scanned: 0,
            is_complete: false,
            has_error: false,
        },
    }))
}

/// Poll for scan progress. Returns the latest progress.
///
/// # Safety
/// `handle` must be a valid pointer from `dm_scan_start`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dm_scan_poll(handle: *mut DmScanHandle) -> DmScanProgress {
    let handle = unsafe { &mut *handle };

    // Drain progress channel
    if let Some(rx) = &handle.progress_rx {
        loop {
            match rx.try_recv() {
                Ok(p) => {
                    handle.latest_progress = DmScanProgress {
                        files_scanned: p.files_scanned,
                        dirs_scanned: p.dirs_scanned,
                        bytes_scanned: p.bytes_scanned,
                        is_complete: p.phase == ScanPhase::Complete,
                        has_error: false,
                    };
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    // Sender dropped — scan thread finished
                    break;
                }
            }
        }
    }

    // Check if the scan thread has completed
    if handle.tree.is_none() {
        if let Some(jh) = handle.join_handle.take() {
            if jh.is_finished() {
                match jh.join() {
                    Ok(Ok(result)) => {
                        handle.denied_paths = result.denied_paths.clone();
                        handle.latest_progress.is_complete = true;
                        handle.tree = Some(result.tree);
                    }
                    Ok(Err(_)) => {
                        handle.latest_progress.is_complete = true;
                        handle.latest_progress.has_error = true;
                    }
                    Err(_) => {
                        handle.latest_progress.is_complete = true;
                        handle.latest_progress.has_error = true;
                    }
                }
                handle.progress_rx = None;
            } else {
                // Not finished yet, put it back
                handle.join_handle = Some(jh);
            }
        }
    }

    DmScanProgress { ..handle.latest_progress }
}

/// Cancel an in-progress scan.
///
/// # Safety
/// `handle` must be a valid pointer from `dm_scan_start`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dm_scan_cancel(handle: *mut DmScanHandle) {
    let handle = unsafe { &*handle };
    handle.cancel.store(true, Ordering::Relaxed);
}

// --- Denied paths ---

/// Get the list of paths that need elevated access.
///
/// # Safety
/// `handle` must be a valid pointer from `dm_scan_start`.
/// Caller must free the result with `dm_string_array_free`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dm_scan_denied_paths(handle: *const DmScanHandle) -> DmStringArray {
    let handle = unsafe { &*handle };

    let c_strings: Vec<*const c_char> = handle
        .denied_paths
        .iter()
        .filter_map(|s| CString::new(s.as_str()).ok())
        .map(|cs| cs.into_raw() as *const c_char)
        .collect();

    let count = c_strings.len() as u64;
    let ptr = if c_strings.is_empty() {
        std::ptr::null()
    } else {
        let boxed = c_strings.into_boxed_slice();
        Box::into_raw(boxed) as *const *const c_char
    };

    DmStringArray {
        strings: ptr,
        count,
    }
}

// --- Treemap layout ---

/// Compute treemap layout for the current scan data.
///
/// # Safety
/// `handle` must be a valid pointer with a completed scan.
/// Caller must free the result with `dm_treemap_free`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dm_treemap_layout(
    handle: *const DmScanHandle,
    root_index: u32,
    viewport_w: f32,
    viewport_h: f32,
    max_depth: u16,
) -> DmTreemapResult {
    let handle = unsafe { &*handle };

    let tree = match &handle.tree {
        Some(t) => t,
        None => {
            return DmTreemapResult {
                rects: std::ptr::null(),
                count: 0,
            };
        }
    };

    let rects = treemap::layout_treemap(tree, root_index, viewport_w, viewport_h, max_depth);

    let result = DmTreemapResult {
        count: rects.len() as u64,
        rects: if rects.is_empty() {
            std::ptr::null()
        } else {
            let boxed = rects.into_boxed_slice();
            Box::into_raw(boxed) as *const TreemapRect
        },
    };

    result
}

/// Free a treemap layout result.
///
/// # Safety
/// `result` must be from `dm_treemap_layout`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dm_treemap_free(result: DmTreemapResult) {
    if !result.rects.is_null() && result.count > 0 {
        unsafe {
            let _ = Box::from_raw(std::ptr::slice_from_raw_parts_mut(
                result.rects as *mut TreemapRect,
                result.count as usize,
            ));
        }
    }
}

// --- Node queries ---

/// Get the display name for a node.
///
/// # Safety
/// Caller must free the result with `dm_string_free`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dm_node_name(
    handle: *const DmScanHandle,
    node_index: u32,
) -> *mut c_char {
    let handle = unsafe { &*handle };
    let name = handle
        .tree
        .as_ref()
        .map(|t| t.node(node_index).name.as_str())
        .unwrap_or("");
    CString::new(name).unwrap_or_default().into_raw()
}

/// Get the size of a node in bytes.
///
/// # Safety
/// `handle` must be a valid pointer with a completed scan.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dm_node_size(handle: *const DmScanHandle, node_index: u32) -> u64 {
    let handle = unsafe { &*handle };
    handle
        .tree
        .as_ref()
        .map(|t| t.node(node_index).size)
        .unwrap_or(0)
}

/// Check if a node is a denied/restricted directory.
///
/// # Safety
/// `handle` must be a valid pointer with a completed scan.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dm_node_is_denied(handle: *const DmScanHandle, node_index: u32) -> bool {
    let handle = unsafe { &*handle };
    handle
        .tree
        .as_ref()
        .map(|t| t.node(node_index).denied)
        .unwrap_or(false)
}

/// Check if a node is a directory.
///
/// # Safety
/// `handle` must be a valid pointer with a completed scan.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dm_node_is_dir(handle: *const DmScanHandle, node_index: u32) -> bool {
    let handle = unsafe { &*handle };
    handle
        .tree
        .as_ref()
        .map(|t| t.node(node_index).is_dir)
        .unwrap_or(false)
}

// --- Child iteration ---

/// Get the root node index.
///
/// # Safety
/// `handle` must be a valid pointer with a completed scan.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dm_root_index(handle: *const DmScanHandle) -> u32 {
    let handle = unsafe { &*handle };
    handle.tree.as_ref().map(|t| t.root).unwrap_or(0)
}

/// Get the number of children for a node.
///
/// # Safety
/// `handle` must be a valid pointer with a completed scan.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dm_node_child_count(handle: *const DmScanHandle, node_index: u32) -> u32 {
    let handle = unsafe { &*handle };
    handle
        .tree
        .as_ref()
        .map(|t| t.node(node_index).child_count())
        .unwrap_or(0)
}

/// Get the node index of the Nth child of a node.
///
/// # Safety
/// `handle` must be a valid pointer with a completed scan.
/// `child_offset` must be less than `dm_node_child_count`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dm_node_child_at(
    handle: *const DmScanHandle,
    node_index: u32,
    child_offset: u32,
) -> u32 {
    let handle = unsafe { &*handle };
    handle
        .tree
        .as_ref()
        .map(|t| {
            let node = t.node(node_index);
            node.children.start + child_offset
        })
        .unwrap_or(0)
}

/// Get the total size of the scanned tree.
///
/// # Safety
/// `handle` must be a valid pointer with a completed scan.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dm_total_size(handle: *const DmScanHandle) -> u64 {
    let handle = unsafe { &*handle };
    handle.tree.as_ref().map(|t| t.total_size).unwrap_or(0)
}

// --- Memory management ---

/// Free a C string allocated by dm-ffi.
///
/// # Safety
/// `s` must be from a dm-ffi function that returns `*mut c_char`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dm_string_free(s: *mut c_char) {
    if !s.is_null() {
        unsafe {
            let _ = CString::from_raw(s);
        }
    }
}

/// Free a string array allocated by dm-ffi.
///
/// # Safety
/// `arr` must be from `dm_scan_denied_paths`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dm_string_array_free(arr: DmStringArray) {
    if !arr.strings.is_null() && arr.count > 0 {
        unsafe {
            let slice = std::slice::from_raw_parts(arr.strings, arr.count as usize);
            for &s in slice {
                if !s.is_null() {
                    let _ = CString::from_raw(s as *mut c_char);
                }
            }
            let _ = Box::from_raw(std::ptr::slice_from_raw_parts_mut(
                arr.strings as *mut *const c_char,
                arr.count as usize,
            ));
        }
    }
}

/// Free a scan handle and all associated memory.
///
/// # Safety
/// `handle` must be from `dm_scan_start`. Do not use after freeing.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dm_scan_free(handle: *mut DmScanHandle) {
    if !handle.is_null() {
        unsafe {
            let mut handle = Box::from_raw(handle);
            // Cancel if still running
            handle.cancel.store(true, Ordering::Relaxed);
            // Wait for thread to finish
            if let Some(jh) = handle.join_handle.take() {
                let _ = jh.join();
            }
            // Box drop handles the rest
        }
    }
}
