# Contributing to open-subtitle

Thanks for helping build a subtitle engine that's keyless by default and agnostic
across apps, platforms, media, and languages. Please read `AGENTS.md` and
`docs/ARCHITECTURE.md` first — they define the architecture you must work within.

## Ground rules

1. **Respect the dependency rule.** `os-engine` depends only on `os-core`; only
   the frontends (`os-cli`/`os-daemon`/`os-ffi`/`os-mpv`) may name concrete
   adapters. New capability ⇒ new **port** in `os-core`, not a cross-crate hack.
2. **Keyless-by-default is sacred.** A provider that needs an API key or login
   ships **disabled** in the default config. Never make the default path require a
   key.
3. **No runtime interpreter dependency** on the core path. Optional tools
   (ffmpeg/ffsubsync/alass/whisper) are detected at runtime; their absence
   disables a feature, never aborts.
4. **No secrets in code, logs, repo, or release artifacts.** Keys live only in the
   user's config file. Mask tokens in any display.
5. **Errors carry meaning.** Map adapter errors to the right `CoreError` variant
   (`RateLimited`/`DownloadLimit`/`AuthRequired`/`NotFound`/…) so the `Throttler`
   and fallback logic behave correctly.

## Workflow

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

All three must be clean before you open a PR.

- **Pure logic** (scoring, matching, hashing, parsing, encoding, config) needs
  **unit tests, no network**.
- **Adapters** need **fixture / mock-HTTP** tests (record real responses; do not
  hit live services in CI). Live tests are gated behind `#[ignore]` + an env var.
- Update `CHANGELOG.md` (`Unreleased`) and tick the relevant boxes in
  `docs/PLAN.md` for anything user-visible.

## Adding a provider

See `AGENTS.md` → "How to add a provider." In short: implement `Provider` in
`crates/os-providers/src/<source>.rs`, declare `capabilities()`, map results to
`SubtitleCandidate` (fill every match field you can), map errors to `CoreError`,
wire it in each frontend's `compose.rs` (disabled by default if it needs a key),
add config keys, and add fixture tests. No other crate should change.

## Commit & PR conventions

- Small, focused commits; imperative subject lines (e.g. "add subdl provider").
- Reference the PLAN phase where relevant (e.g. "Phase 3: …").
- A PR should keep the four agnostic axes intact (app/platform/media/language) and
  not regress keyless-by-default.
- `Cargo.lock`: committed for the binaries' reproducibility starting at the first
  code commit; libraries don't pin. (The current docs-only commit has no lock.)

## Scope discipline

Breadth (more providers, more languages) is welcome. Private-tracker scraping,
captcha farms, and GUI/extension work live in `future-features.md` and should be
built **on top of** `ostd`/FFI, not inside core.

## Code of conduct

Be respectful and constructive. Prioritize technical accuracy over consensus;
disagree with reasons. We optimize for correctness and the user's autonomy
(keyless, local-first, no lock-in).

## Branch policy

Open feature/fix PRs against **`dev`**, not `master`. When a batch is ready, open a single **`dev` → `master`** integration PR.
