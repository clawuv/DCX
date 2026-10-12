//! Builds the **anonymized** structural summary sent to the cloud service.
//!
//! Everything that ever leaves the device is produced here, so the privacy
//! contract is auditable in one file (the same discipline as the sponsor
//! `wire` module). What is sent:
//!
//! * base names of folders and files (never a full or absolute path),
//! * their sizes and a coarse depth,
//! * a file-type (extension) summary,
//! * totals and the OS.
//!
//! What is never sent: the scan root, the home/username, absolute paths, file
//! contents, timestamps, or any account identifier.

use crate::tree::{Kind, Tree, NONE};
use serde::Serialize;

/// Caps that keep the payload small and predictable.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    /// Only items at least this large are considered.
    pub min_size: u64,
    /// Maximum folder/file levels below the root.
    pub max_depth: u8,
    /// Maximum number of items included.
    pub max_nodes: usize,
    /// Maximum number of distinct extensions in the summary.
    pub max_exts: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Limits { min_size: 1024 * 1024, max_depth: 4, max_nodes: 400, max_exts: 30 }
    }
}

#[derive(Serialize)]
pub struct Payload {
    /// Schema version.
    pub v: u8,
    pub os: &'static str,
    pub total_size: u64,
    pub files: u32,
    /// How deep the names below the root go.
    pub depth_cap: u8,
    pub nodes: Vec<PayloadNode>,
    pub extensions: Vec<ExtCount>,
}

#[derive(Serialize)]
pub struct PayloadNode {
    /// Stable index local to this payload; the service echoes it back.
    pub idx: usize,
    /// Base name only (no path, no separator).
    pub name: String,
    /// "dir" or "file".
    pub kind: &'static str,
    pub size: u64,
    pub depth: u8,
}

#[derive(Serialize)]
pub struct ExtCount {
    pub ext: String,
    pub count: u64,
    pub bytes: u64,
}

/// A payload plus the mapping from `idx` back to tree nodes (kept local; the
/// mapping itself is never sent).
pub struct Prepared {
    pub payload: Payload,
    pub nodes: Vec<u32>,
}

fn os_name() -> &'static str {
    if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(windows) {
        "windows"
    } else {
        "linux"
    }
}

/// The lower-case extension of `name`, or `"(none)"`.
fn extension_of(name: &str) -> String {
    match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() && !ext.is_empty() && !ext.contains(['/', '\\']) => ext.to_ascii_lowercase(),
        _ => "(none)".to_string(),
    }
}

/// Walks the tree and produces the anonymized summary plus the local index map.
pub fn build(tree: &Tree, limits: &Limits) -> Prepared {
    // (node id, depth)
    let mut candidates: Vec<(u32, u8)> = Vec::new();
    let mut stack: Vec<(u32, u8)> = vec![(tree.root, 0)];
    let mut exts: std::collections::HashMap<String, (u64, u64)> = std::collections::HashMap::new();
    while let Some((id, depth)) = stack.pop() {
        let node = tree.get(id);
        if id != tree.root && !matches!(node.kind, Kind::Mount | Kind::Hidden | Kind::SmallFiles | Kind::Link) {
            if node.size >= limits.min_size && depth <= limits.max_depth {
                candidates.push((id, depth));
            }
            if node.kind == Kind::File {
                let e = extension_of(&node.name);
                let slot = exts.entry(e).or_insert((0, 0));
                slot.0 += 1;
                slot.1 += node.size;
            }
        }
        if node.kind == Kind::Dir && node.first_child != NONE && depth < limits.max_depth {
            for c in tree.children(id) {
                stack.push((c, depth + 1));
            }
        }
    }

    // biggest items first, capped
    candidates.sort_unstable_by_key(|&(id, _)| std::cmp::Reverse(tree.get(id).size));
    candidates.truncate(limits.max_nodes);

    let mut nodes = Vec::with_capacity(candidates.len());
    let payload_nodes: Vec<PayloadNode> = candidates
        .iter()
        .enumerate()
        .map(|(idx, &(id, depth))| {
            nodes.push(id);
            let n = tree.get(id);
            PayloadNode {
                idx,
                name: n.name.to_string(),
                kind: if n.kind == Kind::Dir { "dir" } else { "file" },
                size: n.size,
                depth,
            }
        })
        .collect();

    let mut extensions: Vec<ExtCount> = exts
        .into_iter()
        .map(|(ext, (count, bytes))| ExtCount { ext, count, bytes })
        .collect();
    extensions.sort_unstable_by_key(|e| std::cmp::Reverse(e.bytes));
    extensions.truncate(limits.max_exts);

    let root = tree.get(tree.root);
    Prepared {
        payload: Payload {
            v: 1,
            os: os_name(),
            total_size: root.size,
            files: root.files,
            depth_cap: limits.max_depth,
            nodes: payload_nodes,
            extensions,
        },
        nodes,
    }
}

/// Human-readable summary of exactly what [`build`] produced, for the consent
/// dialog.
pub fn preview(p: &Payload) -> String {
    format!(
        "{} item names and sizes (folders and files, up to {} levels deep), a summary of {} file types, the total size and your OS.",
        p.nodes.len(),
        p.depth_cap,
        p.extensions.len(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::Source;

    const MB: u64 = 1_000_000;

    fn tree() -> Tree {
        let mut t = Tree::new("/Users/alice/secret", "secret", Source::Local);
        let lib = t.add_child(0, "Library", Kind::Dir, 0, 0);
        let caches = t.add_child(lib, "Caches", Kind::Dir, 0, 0);
        t.add_child_sized(caches, "big.bin", Kind::File, 20 * MB, 1);
        let nm = t.add_child(0, "node_modules", Kind::Dir, 0, 0);
        t.add_child_sized(nm, "a.jpg", Kind::File, 5 * MB, 1);
        t.add_child_sized(nm, "b.jpg", Kind::File, 5 * MB, 1);
        t.add_child_sized(nm, "c.txt", Kind::File, 2 * MB, 1);
        t
    }

    #[test]
    fn never_leaks_paths_or_names_with_separators() {
        let t = tree();
        let prepared = build(&t, &Limits::default());
        let json = serde_json::to_string(&prepared.payload).unwrap();
        assert!(!json.contains("alice"), "username leaked: {json}");
        assert!(!json.contains("/Users"), "root path leaked: {json}");
        assert!(!json.contains("secret"), "root name leaked: {json}");
        for n in &prepared.payload.nodes {
            assert!(!n.name.contains('/') && !n.name.contains('\\'), "separator in name: {}", n.name);
        }
    }

    #[test]
    fn maps_indices_back_to_nodes() {
        let t = tree();
        let prepared = build(&t, &Limits::default());
        assert_eq!(prepared.payload.nodes.len(), prepared.nodes.len());
        for (i, n) in prepared.payload.nodes.iter().enumerate() {
            assert_eq!(n.idx, i);
            assert_eq!(t.get(prepared.nodes[i]).name.as_ref(), n.name.as_str());
        }
    }

    #[test]
    fn respects_caps() {
        let mut t = Tree::new("/r", "r", Source::Local);
        for i in 0..1000 {
            t.add_child_sized(0, &format!("f{i}.dat"), Kind::File, 2 * MB, 1);
        }
        let limits = Limits { max_nodes: 50, max_depth: 4, min_size: 1, max_exts: 5 };
        let prepared = build(&t, &limits);
        assert!(prepared.payload.nodes.len() <= 50);
    }

    #[test]
    fn aggregates_extensions() {
        let t = tree();
        let prepared = build(&t, &Limits::default());
        let jpg = prepared.payload.extensions.iter().find(|e| e.ext == "jpg").expect("jpg summary");
        assert_eq!(jpg.count, 2);
        assert_eq!(jpg.bytes, 10 * MB);
    }

    #[test]
    fn ignores_small_items_and_mounts() {
        let mut t = Tree::new("/r", "r", Source::Local);
        t.add_child_sized(0, "tiny.bin", Kind::File, 1000, 1);
        t.add_child_sized(0, "Cache", Kind::Mount, 5 * MB, 1);
        let prepared = build(&t, &Limits::default());
        assert!(prepared.payload.nodes.is_empty());
    }
}
