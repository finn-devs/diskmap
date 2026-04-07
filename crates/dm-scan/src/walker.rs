use dm_core::model::{FileNode, FileTree, TreeFragment};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::UNIX_EPOCH;

/// Progress update emitted during scanning.
#[derive(Debug, Clone)]
pub struct ScanProgress {
    pub files_scanned: u64,
    pub dirs_scanned: u64,
    pub bytes_scanned: u64,
    pub current_path: String,
    pub phase: ScanPhase,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanPhase {
    UserScan,
    SizingDirs,
    EscalatedScan,
    Complete,
}

/// Result of scanning a single directory level.
#[derive(Debug)]
pub struct ScanResult {
    pub tree: FileTree,
    pub denied_paths: Vec<String>,
}

/// Error during scanning.
#[derive(Debug)]
pub struct ScanError {
    pub message: String,
}

impl std::fmt::Display for ScanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "scan error: {}", self.message)
    }
}

impl std::error::Error for ScanError {}

/// Scan a single directory level — its immediate children only.
///
/// For files, size is exact. For subdirectories, size is computed by
/// recursively summing file sizes (this is the expensive part, done
/// per-directory so the UI can show results progressively).
pub fn scan_dir(
    root: &str,
    cancel: Arc<AtomicBool>,
    on_progress: impl Fn(ScanProgress),
) -> Result<ScanResult, ScanError> {
    let root_path = Path::new(root).canonicalize().map_err(|e| ScanError {
        message: format!("cannot resolve path '{}': {}", root, e),
    })?;

    if !root_path.is_dir() {
        return Err(ScanError {
            message: format!("'{}' is not a directory", root),
        });
    }

    let mut denied_paths: Vec<String> = Vec::new();

    // Read immediate children
    let entries = match fs::read_dir(&root_path) {
        Ok(rd) => rd,
        Err(e) => {
            return Err(ScanError {
                message: format!("cannot read '{}': {}", root, e),
            });
        }
    };

    let root_name = root_path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| root_path.to_string_lossy().into_owned());

    // Collect children
    let mut children: Vec<(PathBuf, fs::Metadata)> = Vec::new();
    for entry in entries {
        if cancel.load(Ordering::Relaxed) {
            return Err(ScanError {
                message: "scan cancelled".into(),
            });
        }
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        let path = entry.path();
        // Use symlink_metadata to avoid following symlinks
        let metadata = match fs::symlink_metadata(&path) {
            Ok(m) => m,
            Err(_) => {
                denied_paths.push(path.to_string_lossy().into_owned());
                continue;
            }
        };
        // Skip symlinks
        if metadata.is_symlink() {
            continue;
        }
        // Skip virtual/pseudo filesystems
        if metadata.is_dir() && is_virtual_fs(&path) {
            continue;
        }
        children.push((path, metadata));
    }

    // Sort by name
    children.sort_by(|a, b| a.0.file_name().cmp(&b.0.file_name()));

    on_progress(ScanProgress {
        files_scanned: 0,
        dirs_scanned: 0,
        bytes_scanned: 0,
        current_path: root_path.to_string_lossy().into_owned(),
        phase: ScanPhase::UserScan,
    });

    // Build nodes: root + children
    let mut nodes: Vec<FileNode> = Vec::with_capacity(children.len() + 1);
    let child_start = 1u32;
    let child_end = child_start + children.len() as u32;

    // Root node (size filled in after children)
    nodes.push(FileNode {
        name: root_name,
        size: 0,
        is_dir: true,
        extension: None,
        modified: None,
        children: child_start..child_end,
        depth: 0,
        denied: false,
    });

    // Children — files get exact size, dirs get 0 initially
    let mut dirs_to_size: Vec<(u32, PathBuf)> = Vec::new();
    let mut total_file_size: u64 = 0;
    let mut file_count: u64 = 0;
    let mut dir_count: u64 = 0;

    for (path, metadata) in &children {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();

        let is_dir = metadata.is_dir();
        let size = if is_dir { 0 } else { metadata.len() };
        let modified = metadata
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64);

        let extension = if !is_dir {
            path.extension()
                .map(|e| e.to_string_lossy().into_owned())
        } else {
            None
        };

        let is_denied = denied_paths.iter().any(|d| d == &path.to_string_lossy().as_ref());

        let node_idx = nodes.len() as u32;
        nodes.push(FileNode {
            name,
            size,
            is_dir,
            extension,
            modified,
            children: 0..0, // Leaf — no children loaded yet
            depth: 1,
            denied: is_denied,
        });

        if is_dir {
            dir_count += 1;
            dirs_to_size.push((node_idx, path.clone()));
        } else {
            file_count += 1;
            total_file_size += size;
        }
    }

    on_progress(ScanProgress {
        files_scanned: file_count,
        dirs_scanned: dir_count,
        bytes_scanned: total_file_size,
        current_path: "Calculating directory sizes...".into(),
        phase: ScanPhase::SizingDirs,
    });

    // Now compute directory sizes (the expensive part)
    // Each dir size = sum of all files recursively inside it
    let _total_dirs = dirs_to_size.len();
    for (i, (node_idx, dir_path)) in dirs_to_size.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            return Err(ScanError {
                message: "scan cancelled".into(),
            });
        }

        let dir_size = compute_dir_size(dir_path, &cancel);
        nodes[*node_idx as usize].size = dir_size;

        on_progress(ScanProgress {
            files_scanned: file_count,
            dirs_scanned: (i + 1) as u64,
            bytes_scanned: total_file_size + dir_size,
            current_path: dir_path.to_string_lossy().into_owned(),
            phase: ScanPhase::SizingDirs,
        });
    }

    // Set root size
    let total_size: u64 = nodes[1..].iter().map(|n| n.size).sum();
    nodes[0].size = total_size;

    on_progress(ScanProgress {
        files_scanned: file_count,
        dirs_scanned: dir_count,
        bytes_scanned: total_size,
        current_path: String::new(),
        phase: ScanPhase::Complete,
    });

    Ok(ScanResult {
        tree: FileTree {
            nodes,
            root: 0,
            total_size,
            denied_paths: denied_paths.clone(),
        },
        denied_paths,
    })
}

/// Compute total size of a directory by walking all descendants.
/// Uses a simple iterative stack (no recursion) to avoid stack overflow.
/// Gives up after `timeout` to keep the UI responsive for huge dirs.
fn compute_dir_size(path: &Path, cancel: &Arc<AtomicBool>) -> u64 {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let mut total: u64 = 0;
    let mut stack: Vec<PathBuf> = vec![path.to_path_buf()];

    while let Some(dir) = stack.pop() {
        if cancel.load(Ordering::Relaxed) || std::time::Instant::now() > deadline {
            return total;
        }

        let entries = match fs::read_dir(&dir) {
            Ok(rd) => rd,
            Err(_) => continue,
        };

        for entry in entries {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };

            let metadata = match fs::symlink_metadata(entry.path()) {
                Ok(m) => m,
                Err(_) => continue,
            };

            if metadata.is_symlink() {
                continue;
            }

            if metadata.is_dir() {
                let entry_path = entry.path();
                if !is_virtual_fs(&entry_path) {
                    stack.push(entry_path);
                }
            } else {
                total += metadata.len();
            }
        }
    }

    total
}

/// Scan only specific denied paths (called after privilege escalation).
pub fn scan_denied_paths(
    paths: &[String],
    on_progress: impl Fn(ScanProgress),
) -> Result<Vec<TreeFragment>, ScanError> {
    let cancel = Arc::new(AtomicBool::new(false));
    let mut fragments = Vec::new();

    for path_str in paths {
        let path = Path::new(path_str);
        if !path.is_dir() {
            continue;
        }
        if is_system_critical(path) {
            continue;
        }

        let result = scan_dir(path_str, cancel.clone(), &on_progress)?;
        fragments.push(TreeFragment {
            path: path_str.clone(),
            nodes: result.tree.nodes,
            total_size: result.tree.total_size,
        });
    }

    Ok(fragments)
}

/// Merge escalated scan fragments into an existing tree.
pub fn merge_fragments(tree: &mut FileTree, fragments: Vec<TreeFragment>) {
    for fragment in fragments {
        let target_name = Path::new(&fragment.path)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();

        let target_index = tree
            .nodes
            .iter()
            .position(|n| n.denied && n.name == target_name);

        if let Some(idx) = target_index {
            let offset = tree.nodes.len() as u32;
            let child_count = fragment.nodes.len() as u32;

            for mut node in fragment.nodes {
                node.children =
                    (node.children.start + offset)..(node.children.end + offset);
                tree.nodes.push(node);
            }

            tree.nodes[idx].denied = false;
            tree.nodes[idx].size = fragment.total_size;
            tree.nodes[idx].children = offset..(offset + child_count);
            tree.denied_paths.retain(|p| p != &fragment.path);
        }
    }
    // Recalculate root size
    let root = tree.root as usize;
    let children = tree.nodes[root].children.clone();
    tree.nodes[root].size = children.map(|c| tree.nodes[c as usize].size).sum();
    tree.total_size = tree.nodes[root].size;
}

/// Check if a path is a virtual/pseudo filesystem that shouldn't be scanned.
/// These report fake sizes and can cause hangs.
fn is_virtual_fs(path: &Path) -> bool {
    let path_str = path.to_string_lossy();
    matches!(
        path_str.as_ref(),
        "/proc" | "/sys" | "/dev" | "/run" | "/snap"
        | "/dev/shm" | "/dev/pts"
        | "/sys/firmware" | "/sys/kernel"
    ) || path_str.starts_with("/proc/")
      || path_str.starts_with("/sys/")
      || path_str.starts_with("/dev/")
      || path_str.starts_with("/run/")
      || path_str.starts_with("/snap/")
}

fn is_system_critical(path: &Path) -> bool {
    let path_str = path.to_string_lossy();
    matches!(
        path_str.as_ref(),
        "/proc" | "/sys" | "/dev" | "/run" | "/snap"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_test_dir() -> TempDir {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("a.txt"), "hello").unwrap();
        fs::write(dir.path().join("b.mp4"), "x".repeat(1000)).unwrap();
        fs::create_dir(dir.path().join("sub")).unwrap();
        fs::write(dir.path().join("sub/c.rs"), "fn main() {}").unwrap();
        dir
    }

    #[test]
    fn scan_dir_counts_children() {
        let dir = make_test_dir();
        let cancel = Arc::new(AtomicBool::new(false));
        let result = scan_dir(dir.path().to_str().unwrap(), cancel, |_| {}).unwrap();

        // root + a.txt + b.mp4 + sub/ = 4 nodes (sub/c.rs NOT included — single level)
        assert_eq!(result.tree.nodes.len(), 4);
    }

    #[test]
    fn scan_dir_computes_dir_sizes() {
        let dir = make_test_dir();
        let cancel = Arc::new(AtomicBool::new(false));
        let result = scan_dir(dir.path().to_str().unwrap(), cancel, |_| {}).unwrap();

        // sub/ should have size = 12 (fn main() {})
        let sub = result.tree.nodes.iter().find(|n| n.name == "sub").unwrap();
        assert_eq!(sub.size, 12);
    }

    #[test]
    fn scan_dir_total_size() {
        let dir = make_test_dir();
        let cancel = Arc::new(AtomicBool::new(false));
        let result = scan_dir(dir.path().to_str().unwrap(), cancel, |_| {}).unwrap();

        // 5 (hello) + 1000 (x*1000) + 12 (fn main() {}) = 1017
        assert_eq!(result.tree.total_size, 1017);
    }

    #[test]
    fn scan_dir_empty() {
        let dir = TempDir::new().unwrap();
        let cancel = Arc::new(AtomicBool::new(false));
        let result = scan_dir(dir.path().to_str().unwrap(), cancel, |_| {}).unwrap();

        assert_eq!(result.tree.nodes.len(), 1); // Just root
        assert_eq!(result.tree.total_size, 0);
    }

    #[test]
    fn cancel_stops_scan() {
        let dir = make_test_dir();
        let cancel = Arc::new(AtomicBool::new(true));
        let result = scan_dir(dir.path().to_str().unwrap(), cancel, |_| {});
        assert!(result.is_err());
    }

    #[test]
    fn system_critical_paths_blocked() {
        assert!(is_system_critical(Path::new("/proc")));
        assert!(is_system_critical(Path::new("/sys")));
        assert!(!is_system_critical(Path::new("/home")));
    }

    #[test]
    fn compute_dir_size_works() {
        let dir = make_test_dir();
        let cancel = Arc::new(AtomicBool::new(false));
        let size = compute_dir_size(dir.path(), &cancel);
        assert_eq!(size, 1017);
    }
}
