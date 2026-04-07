use serde::{Deserialize, Serialize};
use std::ops::Range;

/// Arena-stored file tree node.
///
/// Children are stored contiguously in `FileTree::nodes` and referenced
/// by an index range, making the tree cache-friendly and FFI-safe.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileNode {
    pub name: String,
    /// Byte size. For directories this is the sum of all descendants.
    pub size: u64,
    pub is_dir: bool,
    pub extension: Option<String>,
    /// Unix timestamp (seconds since epoch), if available.
    pub modified: Option<i64>,
    /// Index range into `FileTree::nodes` for this node's children.
    /// Empty range for files or empty directories.
    pub children: Range<u32>,
    /// Depth from the scan root (root = 0).
    pub depth: u16,
    /// `true` when the scanner could not read this directory.
    pub denied: bool,
}

impl FileNode {
    pub fn child_count(&self) -> u32 {
        self.children.end - self.children.start
    }

    pub fn is_leaf(&self) -> bool {
        self.children.is_empty()
    }
}

/// A complete scanned file tree stored as a flat arena.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileTree {
    pub nodes: Vec<FileNode>,
    /// Index of the root node in `nodes`.
    pub root: u32,
    pub total_size: u64,
    /// Absolute paths the scanner could not read due to permissions.
    pub denied_paths: Vec<String>,
}

impl FileTree {
    pub fn root_node(&self) -> &FileNode {
        &self.nodes[self.root as usize]
    }

    pub fn children(&self, node: &FileNode) -> &[FileNode] {
        let start = node.children.start as usize;
        let end = node.children.end as usize;
        &self.nodes[start..end]
    }

    pub fn node(&self, index: u32) -> &FileNode {
        &self.nodes[index as usize]
    }

    /// Walk all descendants of `node` (inclusive), calling `f` for each.
    pub fn walk(&self, node_index: u32, f: &mut impl FnMut(u32, &FileNode)) {
        let node = self.node(node_index);
        f(node_index, node);
        for i in node.children.clone() {
            self.walk(i, f);
        }
    }
}

/// A fragment of a tree returned by an escalated scan of denied paths.
/// Merged into the main `FileTree` after privilege escalation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreeFragment {
    /// The absolute path that was scanned.
    pub path: String,
    /// Nodes rooted at this path.
    pub nodes: Vec<FileNode>,
    pub total_size: u64,
}
