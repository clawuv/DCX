# P2 — Optional cloud (AI) analysis, privacy-first

Builds on P1's local rule engine. Adds an **opt-in** path that sends an
**anonymized** structural summary of a finished scan to a remote analysis
service and merges its suggestions into the same panel.

Non-goals (later phases): accounts/billing, hosted sync, model choice. The
backend is an external contract; the client is what ships here.

## Privacy contract (must hold)

- Sent: base names of folders/files, their sizes, a coarse depth, a file-type
  (extension) summary, the OS, and totals.
- Never sent: absolute paths, the scan root, the username/home, file contents,
  timestamps, account identifiers.
- Off by default. Requires explicit consent, shown with a **preview** of exactly
  what will be sent. The "what we send" list lives in one file (`payload.rs`),
  mirroring the sponsor `wire.rs` discipline.

## Components

- `src/analyze/payload.rs` — `build(tree) -> Prepared { payload, nodes }`
  (anonymized, capped) + `preview()` for the consent dialog. Unit-tested:
  no root path / separator / username leaks; caps respected.
- `src/analyze/cloud.rs` (gui only) — `analyze(tree, prepared, endpoint)` via
  `ureq`; maps the response by payload-local index back to tree nodes, re-runs
  the `safety` gate, tags `Src::Cloud`. Response mapping is factored out and
  unit-tested.
- `Analysis` gains cloud state (running / error / results); `snapshot()` merges
  rule + cloud suggestions (cloud overrides a rule for the same node).
- Settings: `ai_cloud` (opt-in, default off), `ai_endpoint`, `ai_token`.
- UI: an AI button in the Suggestions header; a consent modal
  (`Modal::AiConsent`) showing the preview + endpoint.

## Acceptance

- Local rules keep working with AI off (unchanged).
- Enabling AI shows the preview first; only then does anything leave the device.
- Cloud results pass the same safety gate; forbidden paths never appear.
- `cargo test` green; anonymization tests pass.
