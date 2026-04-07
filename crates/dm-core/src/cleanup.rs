use crate::model::FileTree;

/// A known space-hog pattern that can be safely cleaned up.
#[derive(Debug, Clone)]
pub struct CleanupPattern {
    /// Directory name to match.
    pub dir_name: &'static str,
    /// Human-readable description.
    pub description: &'static str,
    /// Category for grouping in the UI.
    pub category: CleanupCategory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CleanupCategory {
    BuildArtifacts,
    PackageCache,
    RuntimeCache,
    Logs,
}

impl CleanupCategory {
    pub fn label(&self) -> &'static str {
        match self {
            Self::BuildArtifacts => "Build Artifacts",
            Self::PackageCache => "Package Caches",
            Self::RuntimeCache => "Runtime Caches",
            Self::Logs => "Logs",
        }
    }
}

/// Known reclaimable space patterns.
const PATTERNS: &[CleanupPattern] = &[
    // Build artifacts
    CleanupPattern {
        dir_name: "node_modules",
        description: "Node.js dependencies (reinstallable with npm/yarn)",
        category: CleanupCategory::BuildArtifacts,
    },
    CleanupPattern {
        dir_name: "target",
        description: "Rust/Cargo build artifacts",
        category: CleanupCategory::BuildArtifacts,
    },
    CleanupPattern {
        dir_name: "__pycache__",
        description: "Python bytecode cache",
        category: CleanupCategory::BuildArtifacts,
    },
    CleanupPattern {
        dir_name: ".gradle",
        description: "Gradle build cache",
        category: CleanupCategory::BuildArtifacts,
    },
    CleanupPattern {
        dir_name: "build",
        description: "Build output directory",
        category: CleanupCategory::BuildArtifacts,
    },
    CleanupPattern {
        dir_name: "dist",
        description: "Distribution build output",
        category: CleanupCategory::BuildArtifacts,
    },
    CleanupPattern {
        dir_name: ".next",
        description: "Next.js build cache",
        category: CleanupCategory::BuildArtifacts,
    },
    CleanupPattern {
        dir_name: ".nuxt",
        description: "Nuxt.js build cache",
        category: CleanupCategory::BuildArtifacts,
    },
    // Package caches
    CleanupPattern {
        dir_name: ".cache",
        description: "Application cache directory",
        category: CleanupCategory::PackageCache,
    },
    CleanupPattern {
        dir_name: ".npm",
        description: "npm package cache",
        category: CleanupCategory::PackageCache,
    },
    CleanupPattern {
        dir_name: ".cargo",
        description: "Cargo registry and git cache",
        category: CleanupCategory::PackageCache,
    },
    CleanupPattern {
        dir_name: ".pub-cache",
        description: "Dart/Flutter package cache",
        category: CleanupCategory::PackageCache,
    },
    CleanupPattern {
        dir_name: ".m2",
        description: "Maven local repository",
        category: CleanupCategory::PackageCache,
    },
    // Runtime caches
    CleanupPattern {
        dir_name: ".Trash",
        description: "Trash / recycle bin",
        category: CleanupCategory::RuntimeCache,
    },
    CleanupPattern {
        dir_name: ".local/share/Trash",
        description: "Linux trash directory",
        category: CleanupCategory::RuntimeCache,
    },
    // Logs
    CleanupPattern {
        dir_name: "log",
        description: "Log files directory",
        category: CleanupCategory::Logs,
    },
    CleanupPattern {
        dir_name: "logs",
        description: "Log files directory",
        category: CleanupCategory::Logs,
    },
];

/// A reclaimable directory found during analysis.
#[derive(Debug, Clone)]
pub struct ReclaimableItem {
    pub node_index: u32,
    pub path: String,
    pub size: u64,
    pub modified: Option<i64>,
    pub pattern: &'static CleanupPattern,
}

/// Scan the tree for known reclaimable directories.
pub fn find_reclaimable(tree: &FileTree, root: u32) -> Vec<ReclaimableItem> {
    let mut results = Vec::new();
    find_reclaimable_inner(tree, root, &mut String::new(), &mut results);
    results.sort_by(|a, b| b.size.cmp(&a.size));
    results
}

fn find_reclaimable_inner(
    tree: &FileTree,
    node_index: u32,
    path: &mut String,
    out: &mut Vec<ReclaimableItem>,
) {
    let node = tree.node(node_index);
    let prev_len = path.len();

    if !path.is_empty() {
        path.push('/');
    }
    path.push_str(&node.name);

    if node.is_dir {
        // Check if this directory matches any cleanup pattern
        if let Some(pattern) = PATTERNS.iter().find(|p| p.dir_name == node.name) {
            out.push(ReclaimableItem {
                node_index,
                path: path.clone(),
                size: node.size,
                modified: node.modified,
                pattern,
            });
            // Don't recurse into matched dirs — we already counted the whole thing
            path.truncate(prev_len);
            return;
        }

        // Recurse into children
        for i in node.children.clone() {
            find_reclaimable_inner(tree, i, path, out);
        }
    }

    path.truncate(prev_len);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{FileNode, FileTree};

    #[test]
    fn finds_node_modules() {
        let nodes = vec![
            FileNode {
                name: "project".into(),
                size: 500,
                is_dir: true,
                extension: None,
                modified: None,
                children: 1..3,
                depth: 0,
                denied: false,
            },
            FileNode {
                name: "node_modules".into(),
                size: 400,
                is_dir: true,
                extension: None,
                modified: None,
                children: 0..0,
                depth: 1,
                denied: false,
            },
            FileNode {
                name: "src".into(),
                size: 100,
                is_dir: true,
                extension: None,
                modified: None,
                children: 0..0,
                depth: 1,
                denied: false,
            },
        ];

        let tree = FileTree {
            nodes,
            root: 0,
            total_size: 500,
            denied_paths: vec![],
        };

        let reclaimable = find_reclaimable(&tree, 0);
        assert_eq!(reclaimable.len(), 1);
        assert_eq!(reclaimable[0].size, 400);
        assert_eq!(reclaimable[0].pattern.dir_name, "node_modules");
    }

    #[test]
    fn does_not_recurse_into_matched_dirs() {
        // A node_modules inside another node_modules should not be double-counted
        let nodes = vec![
            FileNode {
                name: "root".into(),
                size: 600,
                is_dir: true,
                extension: None,
                modified: None,
                children: 1..2,
                depth: 0,
                denied: false,
            },
            FileNode {
                name: "node_modules".into(),
                size: 500,
                is_dir: true,
                extension: None,
                modified: None,
                children: 2..3,
                depth: 1,
                denied: false,
            },
            FileNode {
                name: "node_modules".into(),
                size: 200,
                is_dir: true,
                extension: None,
                modified: None,
                children: 0..0,
                depth: 2,
                denied: false,
            },
        ];

        let tree = FileTree {
            nodes,
            root: 0,
            total_size: 600,
            denied_paths: vec![],
        };

        let reclaimable = find_reclaimable(&tree, 0);
        // Should only find the outer node_modules, not the inner one
        assert_eq!(reclaimable.len(), 1);
        assert_eq!(reclaimable[0].size, 500);
    }
}
