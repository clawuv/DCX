//! Opt-in cloud analysis client.
//!
//! Sends the anonymized [`Prepared`] summary (see [`super::payload`]) to a
//! remote service and turns its response back into [`Suggestion`]s, mapped to
//! tree nodes by the payload-local index. Cloud results pass the same
//! [`crate::safety`] gate as the local rules.
//!
//! This is the only place that talks to the analysis service; it is compiled
//! only in GUI builds and is only ever called after the user opts in.

use super::payload::Prepared;
use super::{Category, Rationale, Src, Suggestion};
use crate::safety::Rules;
use crate::tree::Tree;
use serde::Deserialize;

/// Where and how to reach the analysis service.
pub struct Endpoint {
    pub url: String,
    pub token: Option<String>,
}

#[derive(Deserialize)]
struct Response {
    #[serde(default)]
    suggestions: Vec<Item>,
}

#[derive(Deserialize)]
struct Item {
    idx: usize,
    category: String,
    #[serde(default = "default_confidence")]
    confidence: u8,
    #[serde(default)]
    rationale: String,
}

fn default_confidence() -> u8 {
    60
}

/// Runs a cloud analysis. Blocking; call from a worker thread.
pub fn analyze(tree: &Tree, prepared: &Prepared, ep: &Endpoint, rules: &Rules) -> Result<Vec<Suggestion>, String> {
    let url = ep.url.trim();
    if url.is_empty() {
        return Err("No analysis service is configured".to_string());
    }
    let body = serde_json::to_string(&prepared.payload).map_err(|e| e.to_string())?;
    let mut req = ureq::post(url)
        .header("Content-Type", "application/json")
        .header("User-Agent", concat!("SquirrelDisk/", env!("CARGO_PKG_VERSION")));
    if let Some(tok) = ep.token.as_deref().filter(|t| !t.is_empty()) {
        req = req.header("Authorization", &format!("Bearer {tok}"));
    }
    let mut resp = req.send(&body).map_err(readable)?;
    let parsed: Response = resp
        .body_mut()
        .read_json()
        .map_err(|e| format!("The analysis service sent an unexpected response ({e})"))?;
    Ok(map_response(tree, prepared, parsed, rules))
}

/// Turns a parsed response into suggestions, re-applying the safety gate.
fn map_response(tree: &Tree, prepared: &Prepared, resp: Response, rules: &Rules) -> Vec<Suggestion> {
    let mut out = Vec::new();
    for it in resp.suggestions {
        let Some(&node) = prepared.nodes.get(it.idx) else { continue };
        if !tree.is_alive(node) {
            continue;
        }
        let Some(category) = Category::from_wire(&it.category) else { continue };
        let path = tree.path(node);
        let verdict = rules.check(&path);
        if verdict.is_forbidden() {
            continue;
        }
        let rationale = if it.rationale.trim().is_empty() { Rationale::Key(category.label_key()) } else { Rationale::Text(it.rationale) };
        out.push(Suggestion {
            node,
            path,
            size: tree.get(node).size,
            category,
            confidence: it.confidence.min(100),
            rationale,
            verdict,
            source: Src::Cloud,
        });
    }
    out.sort_unstable_by_key(|s| std::cmp::Reverse(s.size));
    out
}

/// A message safe to show the user for a failed request.
fn readable(e: ureq::Error) -> String {
    match e {
        ureq::Error::StatusCode(code) => match code {
            401 | 403 => "The analysis service rejected this device".to_string(),
            402 => "Subscribe to use AI analysis".to_string(),
            429 => "Too many analyses for now — try again later".to_string(),
            _ => format!("The analysis service returned an error (HTTP {code})"),
        },
        other => format!("Can't reach the analysis service ({other})"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyze::payload::{self, Limits};
    use crate::safety::{Os, Rules};
    use crate::tree::{Kind, Source};

    fn sample() -> (Tree, Prepared, Rules) {
        let rules = Rules::for_os(Os::Mac, None);
        let mut t = Tree::new("/r", "r", Source::Local);
        let nm = t.add_child(0, "node_modules", Kind::Dir, 0, 0);
        t.add_child_sized(nm, "big.bin", Kind::File, 5_000_000, 1);
        let caches = t.add_child(0, "Caches", Kind::Dir, 0, 0);
        t.add_child_sized(caches, "c", Kind::File, 3_000_000, 1);
        let limits = Limits { min_size: 1, max_depth: 4, max_nodes: 100, max_exts: 10 };
        let prepared = payload::build(&t, &limits);
        (t, prepared, rules)
    }

    #[test]
    fn maps_cloud_response_to_suggestions() {
        let (t, prepared, rules) = sample();
        let resp: Response = serde_json::from_str(
            r#"{"suggestions":[
                {"idx":0,"category":"node_modules","confidence":92,"rationale":"Leftover dependencies"},
                {"idx":999,"category":"cache","confidence":80},
                {"idx":1,"category":"weird_category","confidence":80}
            ]}"#,
        )
        .unwrap();
        let got = map_response(&t, &prepared, resp, &rules);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].source, Src::Cloud);
        assert_eq!(got[0].category, Category::NodeModules);
        assert_eq!(got[0].confidence, 92);
        assert!(matches!(got[0].rationale, Rationale::Text(_)));
    }

    #[test]
    fn cloud_results_pass_the_safety_gate() {
        let rules = Rules::for_os(Os::Mac, None);
        let mut t = Tree::new("/System", "System", Source::Local);
        let lib = t.add_child(0, "Library", Kind::Dir, 0, 0);
        t.add_child_sized(lib, "x", Kind::File, 5_000_000, 1);
        let limits = Limits { min_size: 1, max_depth: 4, max_nodes: 100, max_exts: 10 };
        let prepared = payload::build(&t, &limits);
        let resp: Response = serde_json::from_str(r#"{"suggestions":[{"idx":0,"category":"cache","confidence":90}]}"#).unwrap();
        assert!(map_response(&t, &prepared, resp, &rules).is_empty(), "protected paths must be filtered");
    }

    #[test]
    fn empty_rationale_falls_back_to_the_category_label() {
        let (t, prepared, rules) = sample();
        let resp: Response = serde_json::from_str(r#"{"suggestions":[{"idx":0,"category":"cache","confidence":70}]}"#).unwrap();
        let got = map_response(&t, &prepared, resp, &rules);
        assert_eq!(got.len(), 1);
        assert!(matches!(got[0].rationale, Rationale::Key(_)));
    }
}
