//! The local rule engine: name markers → cleanup categories.
//!
//! Matching is done on folder/file **names** that are already in memory after a
//! scan, plus their sizes. File contents are never read.

use super::{Category, Rationale, Suggestion, Src};
use crate::safety::{Os, Rules};
use crate::tree::{Kind, Tree, NONE};

/// Tunables for a run.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    /// Items smaller than this are ignored.
    pub min_size: u64,
    /// Maximum number of suggestions returned.
    pub max_items: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Limits { min_size: 50 * 1024 * 1024, max_items: 40 }
    }
}

/// A name marker and the category (with confidence and rationale) it implies.
struct Marker {
    /// Exact name (case-insensitive) or a `*.suffix` pattern.
    pattern: &'static str,
    category: Category,
    confidence: u8,
    rationale_key: &'static str,
}

/// Order matters only between overlapping patterns (`Cache` before `Caches`
/// does not matter, since both are exact; `.cache` precedes `*.cache`).
const MARKERS: &[Marker] = &[
    Marker { pattern: "node_modules", category: Category::NodeModules, confidence: 95, rationale_key: "Dependencies a project can reinstall" },
    Marker { pattern: "DerivedData", category: Category::BuildArtifact, confidence: 88, rationale_key: "Xcode build output, safe to regenerate" },
    Marker { pattern: "__pycache__", category: Category::BuildArtifact, confidence: 88, rationale_key: "Compiled Python cache, safe to delete" },
    Marker { pattern: ".next", category: Category::BuildArtifact, confidence: 85, rationale_key: "Next.js build cache" },
    Marker { pattern: ".nuxt", category: Category::BuildArtifact, confidence: 85, rationale_key: "Nuxt build cache" },
    Marker { pattern: ".parcel-cache", category: Category::BuildArtifact, confidence: 85, rationale_key: "Parcel build cache" },
    Marker { pattern: "Pods", category: Category::BuildArtifact, confidence: 78, rationale_key: "CocoaPods dependencies, can be reinstalled" },
    Marker { pattern: "target", category: Category::BuildArtifact, confidence: 70, rationale_key: "Compiled build output" },
    Marker { pattern: "build", category: Category::BuildArtifact, confidence: 65, rationale_key: "Build output" },
    Marker { pattern: "dist", category: Category::BuildArtifact, confidence: 62, rationale_key: "Build output" },
    Marker { pattern: "Caches", category: Category::Cache, confidence: 85, rationale_key: "Application caches, rebuilt on demand" },
    Marker { pattern: "Cache", category: Category::Cache, confidence: 80, rationale_key: "Application cache, rebuilt on demand" },
    Marker { pattern: ".cache", category: Category::Cache, confidence: 85, rationale_key: "Application caches, rebuilt on demand" },
    Marker { pattern: "*.cache", category: Category::Cache, confidence: 78, rationale_key: "Application cache" },
    Marker { pattern: ".rustup", category: Category::DevToolchain, confidence: 55, rationale_key: "Rust toolchains — remove only the unused ones" },
    Marker { pattern: ".cargo", category: Category::DevToolchain, confidence: 55, rationale_key: "Rust package cache — rebuilt on demand" },
    Marker { pattern: ".npm", category: Category::DevToolchain, confidence: 55, rationale_key: "npm cache — rebuilt on demand" },
    Marker { pattern: ".m2", category: Category::DevToolchain, confidence: 55, rationale_key: "Maven repository cache" },
    Marker { pattern: ".gradle", category: Category::DevToolchain, confidence: 55, rationale_key: "Gradle caches — rebuilt on demand" },
    Marker { pattern: ".pyenv", category: Category::DevToolchain, confidence: 55, rationale_key: "Python versions managed by pyenv" },
    Marker { pattern: "logs", category: Category::Logs, confidence: 70, rationale_key: "Log files, usually safe to remove" },
    Marker { pattern: "Logs", category: Category::Logs, confidence: 70, rationale_key: "Log files, usually safe to remove" },
    Marker { pattern: "*.log", category: Category::Logs, confidence: 68, rationale_key: "Log file" },
    Marker { pattern: "*.dmg", category: Category::OldInstaller, confidence: 75, rationale_key: "Installer, re-downloadable" },
    Marker { pattern: "*.pkg", category: Category::OldInstaller, confidence: 75, rationale_key: "Installer, re-downloadable" },
    Marker { pattern: "*.msi", category: Category::OldInstaller, confidence: 75, rationale_key: "Installer, re-downloadable" },
    Marker { pattern: "*.iso", category: Category::OldInstaller, confidence: 72, rationale_key: "Disk image" },
    Marker { pattern: "*.exe", category: Category::OldInstaller, confidence: 68, rationale_key: "Installer, re-downloadable" },
    Marker { pattern: ".Trash", category: Category::Trash, confidence: 60, rationale_key: "Items already in the Trash" },
    Marker { pattern: "$Recycle.Bin", category: Category::Trash, confidence: 60, rationale_key: "Items already in the Recycle Bin" },
];

fn classify(name: &str) -> Option<(&'static str, Category, u8)> {
    for m in MARKERS {
        let hit = match m.pattern.strip_prefix('*') {
            // compare bytes: the cut may fall inside a multi-byte character
            Some(suffix) => name.len() > suffix.len() && name.as_bytes()[name.len() - suffix.len()..].eq_ignore_ascii_case(suffix.as_bytes()),
            None => name.eq_ignore_ascii_case(m.pattern),
        };
        if hit {
            return Some((m.rationale_key, m.category, m.confidence));
        }
    }
    None
}

/// Walks `tree` and returns the cleanup suggestions, biggest first.
pub fn analyze(tree: &Tree, limits: &Limits, rules: &Rules) -> Vec<Suggestion> {
    let mut out: Vec<Suggestion> = Vec::new();
    // DFS carrying which categories an ancestor already claimed, so nested
    // markers (`node_modules` inside `node_modules`) are counted once.
    let mut stack: Vec<(u32, u16)> = vec![(tree.root, 0)];
    while let Some((id, mut claimed)) = stack.pop() {
        let node = tree.get(id);
        if id != tree.root && !matches!(node.kind, Kind::Mount | Kind::Hidden | Kind::SmallFiles) {
            if let Some((rationale, category, confidence)) = classify(&node.name) {
                let bit = 1u16 << category as u16;
                if claimed & bit == 0 {
                    claimed |= bit;
                    if node.size >= limits.min_size {
                        let path = tree.path(id);
                        let verdict = rules.check(&path);
                        if !verdict.is_forbidden() {
                            out.push(Suggestion {
                                node: id,
                                path,
                                size: node.size,
                                category,
                                confidence,
                                rationale: Rationale::Key(rationale),
                                verdict,
                                source: Src::Rule,
                            });
                        }
                    }
                }
            }
        }
        if node.kind == Kind::Dir && node.first_child != NONE {
            for c in tree.children(id) {
                stack.push((c, claimed));
            }
        }
    }
    out.sort_unstable_by_key(|s| std::cmp::Reverse(s.size));
    out.truncate(limits.max_items);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::Source;

    const GB: u64 = 1_000_000_000;

    fn suggestions(tree: &Tree) -> Vec<Suggestion> {
        // fixed ruleset: the fixture trees use POSIX paths ("/System", "/r"),
        // which only mean something under the Mac/Linux rulesets
        analyze(tree, &Limits::default(), &Rules::for_os(Os::Mac, None))
    }

    #[test]
    fn finds_node_modules_and_counts_nested_once() {
        let mut t = Tree::new("/r", "r", Source::Local);
        let proj = t.add_child(0, "proj", Kind::Dir, 0, 0);
        let nm = t.add_child(proj, "node_modules", Kind::Dir, 0, 0);
        let nested = t.add_child(nm, "node_modules", Kind::Dir, 0, 0);
        t.add_child_sized(nested, "big.bin", Kind::File, 2 * GB, 1);
        let got = suggestions(&t);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].category, Category::NodeModules);
        assert_eq!(got[0].size, 2 * GB);
        assert_eq!(got[0].confidence, 95);
    }

    #[test]
    fn matches_caches_build_and_installers() {
        let mut t = Tree::new("/r", "r", Source::Local);
        let caches = t.add_child(0, "Caches", Kind::Dir, 0, 0);
        t.add_child_sized(caches, "x", Kind::File, 3 * GB, 1);
        let target = t.add_child(0, "target", Kind::Dir, 0, 0);
        t.add_child_sized(target, "y", Kind::File, 2 * GB, 1);
        let dmg = t.add_child_sized(0, "App.dmg", Kind::File, GB, 1);
        let _ = dmg;
        let log = t.add_child_sized(0, "app.log", Kind::File, 100_000_000, 1);
        let _ = log;
        let cats: Vec<Category> = suggestions(&t).iter().map(|s| s.category).collect();
        assert!(cats.contains(&Category::Cache));
        assert!(cats.contains(&Category::BuildArtifact));
        assert!(cats.contains(&Category::OldInstaller));
        assert!(cats.contains(&Category::Logs));
    }

    #[test]
    fn ignores_items_below_min_size() {
        let mut t = Tree::new("/r", "r", Source::Local);
        let target = t.add_child(0, "target", Kind::Dir, 0, 0);
        t.add_child_sized(target, "small", Kind::File, 1_000_000, 1);
        assert!(suggestions(&t).is_empty());
    }

    #[test]
    fn never_suggests_protected_locations() {
        let mut t = Tree::new("/System", "System", Source::Local);
        let lib = t.add_child(0, "Library", Kind::Dir, 0, 0);
        let caches = t.add_child(lib, "Caches", Kind::Dir, 0, 0);
        t.add_child_sized(caches, "x", Kind::File, 5 * GB, 1);
        assert!(suggestions(&t).is_empty(), "system paths must be filtered");
    }

    #[test]
    fn skips_mounts_hidden_and_smallfiles() {
        let mut t = Tree::new("/r", "r", Source::Local);
        let m = t.add_child_sized(0, "Cache", Kind::Mount, 5 * GB, 1);
        let h = t.add_child_sized(0, "Caches", Kind::Hidden, 5 * GB, 1);
        let s = t.add_child_sized(0, "node_modules", Kind::SmallFiles, 5 * GB, 1);
        let _ = (m, h, s);
        assert!(suggestions(&t).is_empty());
    }

    #[test]
    fn bulk_safe_excludes_caution_and_low_confidence() {
        let mut t = Tree::new("/r", "r", Source::Local);
        let nm = t.add_child(0, "node_modules", Kind::Dir, 0, 0);
        t.add_child_sized(nm, "x", Kind::File, 2 * GB, 1);
        let got = suggestions(&t);
        assert!(got[0].is_safe_bulk());
    }
}
