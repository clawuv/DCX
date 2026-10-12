//! On-device cleanup suggestions, plus an optional cloud pass.
//!
//! After a scan finishes, [`rules::analyze`] walks the in-memory [`Tree`] and
//! classifies folders and files into broad cleanup categories (caches, build
//! artifacts, `node_modules`, logs, old installers, …). Everything there runs
//! locally: only names and sizes already in memory are looked at, never file
//! contents, and nothing leaves the device.
//!
//! [`payload`] can build an **anonymized** structural summary, and [`cloud`]
//! (gui builds) can send it to a remote service for a deeper pass — strictly
//! opt-in, and only after the user sees exactly what would be sent.
//!
//! Every suggestion carries a confidence and a human-readable rationale, and is
//! gated by [`crate::safety`], so protected locations can never be suggested.

pub mod payload;
pub mod rules;

#[cfg(feature = "gui")]
pub mod cloud;

use crate::safety::Verdict;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// Broad cleanup category of a suggestion.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Category {
    NodeModules,
    BuildArtifact,
    Cache,
    DevToolchain,
    Logs,
    OldInstaller,
    Trash,
    /// Anything the cloud pass flags that has no local bucket.
    Other,
}

impl Category {
    /// English source string, used as the i18n key (see [`crate::i18n`]).
    pub fn label_key(self) -> &'static str {
        match self {
            Category::NodeModules => "Node modules",
            Category::BuildArtifact => "Build artifacts",
            Category::Cache => "Caches",
            Category::DevToolchain => "Dev toolchains",
            Category::Logs => "Logs",
            Category::OldInstaller => "Old installers",
            Category::Trash => "Trash",
            Category::Other => "Other",
        }
    }

    /// Parses a category name from the cloud service.
    pub fn from_wire(s: &str) -> Option<Category> {
        Some(match s {
            "node_modules" => Category::NodeModules,
            "build_artifact" | "build" => Category::BuildArtifact,
            "cache" | "caches" => Category::Cache,
            "dev_toolchain" | "toolchain" => Category::DevToolchain,
            "logs" | "log" => Category::Logs,
            "old_installer" | "installer" => Category::OldInstaller,
            "trash" => Category::Trash,
            "other" => Category::Other,
            _ => return None,
        })
    }
}

/// Where a suggestion came from.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Src {
    /// Local rule engine (on-device).
    Rule,
    /// Cloud analysis service (opt-in).
    Cloud,
}

/// A suggestion's explanation.
#[derive(Clone, Debug)]
pub enum Rationale {
    /// i18n key (English source string), translated for display.
    Key(&'static str),
    /// Ready-to-show text (e.g. from the cloud service).
    Text(String),
}

/// One cleanup suggestion, tied to a node of the scanned tree.
#[derive(Clone, Debug)]
pub struct Suggestion {
    /// Node in the tree; feeds straight into `Session::collect`.
    pub node: u32,
    /// Full path, for display.
    pub path: String,
    pub size: u64,
    pub category: Category,
    /// 0–100; low-confidence items are shown but not bulk-collected.
    pub confidence: u8,
    pub rationale: Rationale,
    /// Safety verdict from [`crate::safety::Rules`].
    pub verdict: Verdict,
    pub source: Src,
}

impl Suggestion {
    /// Safe to add with "Collect all": confident enough and not merely a
    /// caution (caution items are still shown, but one by one).
    pub fn is_safe_bulk(&self) -> bool {
        self.confidence >= 70 && !matches!(self.verdict, Verdict::Caution(_))
    }
}

/// Live state of an analysis run, shared between the worker threads and the UI.
#[derive(Default, Clone)]
pub struct Analysis {
    // local rules
    running: Arc<AtomicBool>,
    suggestions: Arc<Mutex<Vec<Suggestion>>>,
    /// `Tree::version` the suggestions were computed from; the UI marks them
    /// stale when the tree changes underneath (watcher events, deletions).
    at_version: Arc<Mutex<u64>>,
    // cloud pass (opt-in)
    cloud_running: Arc<AtomicBool>,
    cloud: Arc<Mutex<Vec<Suggestion>>>,
    cloud_error: Arc<Mutex<Option<String>>>,
}

impl Analysis {
    // -- local rules ---------------------------------------------------------

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }

    /// Marks a rule run as started; returns false if one is already running.
    pub fn begin(&self) -> bool {
        !self.running.swap(true, Ordering::Relaxed)
    }

    /// Publishes a finished rule run.
    pub fn finish(&self, list: Vec<Suggestion>, tree_version: u64) {
        *self.suggestions.lock().unwrap() = list;
        *self.at_version.lock().unwrap() = tree_version;
        self.running.store(false, Ordering::Relaxed);
    }

    // -- cloud pass ----------------------------------------------------------

    pub fn is_cloud_running(&self) -> bool {
        self.cloud_running.load(Ordering::Relaxed)
    }

    /// Marks a cloud run as started; returns false if one is already running.
    pub fn cloud_begin(&self) -> bool {
        !self.cloud_running.swap(true, Ordering::Relaxed)
    }

    /// Publishes a finished cloud run.
    pub fn cloud_finish(&self, list: Vec<Suggestion>) {
        *self.cloud.lock().unwrap() = list;
        *self.cloud_error.lock().unwrap() = None;
        self.cloud_running.store(false, Ordering::Relaxed);
    }

    /// Records a failed cloud run (offline, rejected, bad response…).
    pub fn cloud_fail(&self, err: impl Into<String>) {
        *self.cloud_error.lock().unwrap() = Some(err.into());
        self.cloud_running.store(false, Ordering::Relaxed);
    }

    pub fn cloud_error(&self) -> Option<String> {
        self.cloud_error.lock().unwrap().clone()
    }

    // -- shared --------------------------------------------------------------

    pub fn len(&self) -> usize {
        self.suggestions.lock().unwrap().len() + self.cloud.lock().unwrap().len()
    }

    /// Suggestions for display: rule + cloud, merged (cloud wins for the same
    /// node), biggest first.
    pub fn snapshot(&self) -> Vec<Suggestion> {
        let mut out = self.suggestions.lock().unwrap().clone();
        let cloud = self.cloud.lock().unwrap().clone();
        let overridden: std::collections::HashSet<u32> = cloud.iter().map(|s| s.node).collect();
        out.retain(|s| !overridden.contains(&s.node));
        out.extend(cloud);
        out.sort_unstable_by_key(|s| std::cmp::Reverse(s.size));
        out
    }

    /// True when the suggestions no longer match the tree they came from.
    pub fn is_stale(&self, tree_version: u64) -> bool {
        let at = *self.at_version.lock().unwrap();
        at != 0 && at != tree_version
    }

    /// Drops the current results and forgets the version.
    pub fn clear(&self) {
        self.suggestions.lock().unwrap().clear();
        self.cloud.lock().unwrap().clear();
        *self.cloud_error.lock().unwrap() = None;
        *self.at_version.lock().unwrap() = 0;
    }
}
