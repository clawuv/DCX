# P1 — Local cleanup suggestions (engine + panel)

Offline, on-device analysis of a finished scan. After a scan completes, a rule
engine walks the in-memory `Tree`, classifies folders/files into broad cleanup
categories, and shows them in the side panel. One click adds a suggestion to the
existing Collector, which runs the existing delete flow.

Non-goals (later phases): cloud LLM, accounts/billing, duplicate hashing,
permanent deletion, automatic cleanup.

## Decisions

1. **Panel form**: side-panel segmented toggle `Contents | Suggestions (N)`.
2. **Minimum size**: 50 MB (configurable in Settings).
3. **Trash category**: included, forced to `Verdict::Caution` by `safety`.
4. **Trigger**: run in the background as soon as a scan finishes.

## Data model (`src/analyze/mod.rs`)

- `Category`: `NodeModules`, `BuildArtifact`, `Cache`, `DevToolchain`, `Logs`,
  `OldInstaller`, `Trash`.
- `Src`: `Rule` (cloud added later).
- `Suggestion { node, path, size, category, confidence, rationale_key,
  rationale_arg, verdict, source }`.
- `Analysis { running, done, suggestions, at_version }` — shared with the UI.

## Rules (`src/analyze/rules.rs`)

Name markers (case-insensitive; `*.suffix` supported), mirroring the pattern in
`src/sponsor/interests.rs`.

| Category | Markers | Confidence |
|---|---|---|
| NodeModules | `node_modules` | 95 |
| BuildArtifact | `DerivedData`, `__pycache__`, `.next`, `.nuxt`, `.parcel-cache`, `Pods`, `target`, `build`, `dist` | 62–88 |
| Cache | `Caches`, `Cache`, `.cache`, `*.cache` | 78–85 |
| DevToolchain | `.rustup`, `.cargo`, `.npm`, `.m2`, `.gradle`, `.pyenv` | 55 |
| Logs | `logs`, `Logs`, `*.log` | 68–70 |
| OldInstaller | `*.dmg`, `*.pkg`, `*.msi`, `*.iso`, `*.exe` | 68–75 |
| Trash | `.Trash`, `$Recycle.Bin` | 60 |

Algorithm: iterative DFS carrying the set of categories an ancestor already
claimed (nested `node_modules` counted once). A match above `min_size` runs
through `safety::Rules::check`; `Forbidden` is dropped, `Caution` is kept.
Sort by size, cap at 40.

## Wiring

- `src/main.rs`: `mod analyze;`.
- `src/ui/app.rs`: `Session.analysis`; `start_analysis` spawns a worker on scan
  completion (in `poll_sessions`); `PanelTab` state.
- `src/ui/view.rs`: segmented toggle in `panel_ui`; suggestions list; row click
  → `Session::collect`; footer "Collect all" for high-confidence, non-caution
  items.
- `src/settings.rs`: `suggestions` (bool), `analysis_min_size_mb` (u32).
- `src/ui/modals.rs`: settings controls.
- `assets/i18n/*.json`: English source strings are enough; translations optional.

## Testing

- Unit tests in `rules.rs` (synthetic `Tree`): category hits, nested dedupe,
  min-size filter, safety filtering (`/System` never suggested).
- Integration: temp dir with `node_modules/`, `Cache/`, `target/`; scan then
  analyze.
- Manual: `SQD_WINDOW=WxH` layout check; demo scanner (`src/scan/demo.rs`) for a
  deterministic panel.

## Acceptance

- Suggestions appear within ~1–2 s of a finished scan, sorted by size, with
  category/confidence/rationale.
- `/System`, `~/Documents`, etc. never appear.
- One-click collect → Collector → existing delete confirmation.
- `cargo test` green.
