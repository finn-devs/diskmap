use crate::model::FileTree;
use std::time::{SystemTime, UNIX_EPOCH};

/// A file entry returned by analysis queries.
#[derive(Debug, Clone)]
pub struct FileEntry {
    pub node_index: u32,
    pub path: String,
    pub size: u64,
    pub modified: Option<i64>,
    pub extension: Option<String>,
}

/// Find the N largest files in the tree.
pub fn largest_files(tree: &FileTree, root: u32, limit: usize) -> Vec<FileEntry> {
    let mut files = Vec::new();
    collect_files(tree, root, &mut String::new(), &mut files);
    files.sort_by(|a, b| b.size.cmp(&a.size));
    files.truncate(limit);
    files
}

/// Find files not modified in the last `months` months.
pub fn old_files(tree: &FileTree, root: u32, months: u32, limit: usize) -> Vec<FileEntry> {
    let cutoff = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64 - (months as i64 * 30 * 24 * 3600))
        .unwrap_or(0);

    let mut files = Vec::new();
    collect_files(tree, root, &mut String::new(), &mut files);
    files.retain(|f| f.modified.is_some_and(|m| m < cutoff));
    files.sort_by(|a, b| b.size.cmp(&a.size));
    files.truncate(limit);
    files
}

/// Aggregate file sizes by extension category.
#[derive(Debug, Clone)]
pub struct TypeBreakdown {
    pub category: String,
    pub total_size: u64,
    pub file_count: u64,
}

pub fn type_breakdown(tree: &FileTree, root: u32) -> Vec<TypeBreakdown> {
    let mut categories: std::collections::HashMap<String, (u64, u64)> =
        std::collections::HashMap::new();

    tree.walk(root, &mut |_, node| {
        if !node.is_dir {
            let cat = categorize_extension(node.extension.as_deref());
            let entry = categories.entry(cat).or_insert((0, 0));
            entry.0 += node.size;
            entry.1 += 1;
        }
    });

    let mut result: Vec<TypeBreakdown> = categories
        .into_iter()
        .map(|(category, (total_size, file_count))| TypeBreakdown {
            category,
            total_size,
            file_count,
        })
        .collect();

    result.sort_by(|a, b| b.total_size.cmp(&a.total_size));
    result
}

fn collect_files(tree: &FileTree, node_index: u32, path: &mut String, out: &mut Vec<FileEntry>) {
    let node = tree.node(node_index);
    let prev_len = path.len();

    if !path.is_empty() {
        path.push('/');
    }
    path.push_str(&node.name);

    if !node.is_dir {
        out.push(FileEntry {
            node_index,
            path: path.clone(),
            size: node.size,
            modified: node.modified,
            extension: node.extension.clone(),
        });
    }

    for i in node.children.clone() {
        collect_files(tree, i, path, out);
    }

    path.truncate(prev_len);
}

fn categorize_extension(ext: Option<&str>) -> String {
    let ext = match ext {
        Some(e) => e.to_ascii_lowercase(),
        None => return "Other".into(),
    };

    match ext.as_str() {
        "jpg" | "jpeg" | "png" | "gif" | "bmp" | "tiff" | "webp" | "svg" | "ico" | "heic"
        | "raw" | "cr2" | "nef" | "arw" => "Images".into(),

        "mp4" | "mkv" | "avi" | "mov" | "wmv" | "flv" | "webm" | "m4v" => "Video".into(),

        "mp3" | "flac" | "wav" | "aac" | "ogg" | "wma" | "m4a" | "opus" => "Audio".into(),

        "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "odt" | "ods" | "odp"
        | "txt" | "md" | "rtf" | "csv" | "tsv" => "Documents".into(),

        "rs" | "go" | "py" | "js" | "ts" | "jsx" | "tsx" | "c" | "cpp" | "h" | "hpp" | "java"
        | "kt" | "swift" | "rb" | "php" | "cs" | "scala" | "zig" | "html" | "css" | "scss"
        | "less" | "xml" | "json" | "yaml" | "yml" | "toml" => "Code".into(),

        "zip" | "tar" | "gz" | "bz2" | "xz" | "7z" | "rar" | "zst" | "lz4" | "deb" | "rpm"
        | "dmg" | "iso" | "img" => "Archives".into(),

        "exe" | "dll" | "so" | "dylib" | "app" | "bin" | "elf" => "Executables".into(),

        "db" | "sqlite" | "sqlite3" | "mdb" => "Databases".into(),

        "ttf" | "otf" | "woff" | "woff2" => "Fonts".into(),

        _ => "Other".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{FileNode, FileTree};

    fn make_test_tree() -> FileTree {
        let nodes = vec![
            FileNode {
                name: "root".into(),
                size: 300,
                is_dir: true,
                extension: None,
                modified: None,
                children: 1..4,
                depth: 0,
                denied: false,
            },
            FileNode {
                name: "big.mp4".into(),
                size: 200,
                is_dir: false,
                extension: Some("mp4".into()),
                modified: Some(1_000_000),
                children: 0..0,
                depth: 1,
                denied: false,
            },
            FileNode {
                name: "medium.jpg".into(),
                size: 80,
                is_dir: false,
                extension: Some("jpg".into()),
                modified: Some(i64::MAX / 2), // Far future — never "old"
                children: 0..0,
                depth: 1,
                denied: false,
            },
            FileNode {
                name: "small.txt".into(),
                size: 20,
                is_dir: false,
                extension: Some("txt".into()),
                modified: Some(i64::MAX / 2), // Far future — never "old"
                children: 0..0,
                depth: 1,
                denied: false,
            },
        ];

        FileTree {
            nodes,
            root: 0,
            total_size: 300,
            denied_paths: vec![],
        }
    }

    #[test]
    fn largest_files_sorted() {
        let tree = make_test_tree();
        let files = largest_files(&tree, 0, 10);
        assert_eq!(files.len(), 3);
        assert_eq!(files[0].size, 200);
        assert_eq!(files[1].size, 80);
        assert_eq!(files[2].size, 20);
    }

    #[test]
    fn old_files_filters_by_date() {
        let tree = make_test_tree();
        // big.mp4 has modified=1_000_000 (~1970), should be "old"
        let old = old_files(&tree, 0, 6, 10);
        assert_eq!(old.len(), 1);
        assert_eq!(old[0].size, 200);
    }

    #[test]
    fn type_breakdown_groups_correctly() {
        let tree = make_test_tree();
        let breakdown = type_breakdown(&tree, 0);
        assert!(breakdown.iter().any(|b| b.category == "Video" && b.total_size == 200));
        assert!(breakdown.iter().any(|b| b.category == "Images" && b.total_size == 80));
    }
}
