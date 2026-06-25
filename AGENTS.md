# AGENTS.md

Instructions for AI agents and contributors working on **open-subtitle**.

## Project overview

open-subtitle is a **Rust** engine to find, score, download, normalize, sync,
translate, and transcribe subtitles for **movies, series, and anime** — exposed
through **many frontends** (CLI, HTTP daemon, C-ABI/WASM, an mpv plugin, and a
Rust library). It is **agnostic** across apps, platforms, media types, and
languages, and works **with no API key** for its default sources.

It is a from-scratch synthesis of the best ideas from `subliminal`, `Bazarr`
(`subliminal_patch`/`subzero`), `subg`, `uosc`/`ziggy`, `mpv-subversive`,
`subtool`, and `ffsubsync` — see `docs/RESEARCH.md` for the analysis and
`docs/ARCHITECTURE.md` for the design.

- **Native Rust**, async (tokio). No runtime interpreter; optional external tools
  (ffmpeg/ffsubsync/whisper) are **detected**, never required.
- **Cargo workspace**, one crate per concern (see Module layout).
- **Ports & adapters (hexagonal) + SOLID.** Capabilities are trait *ports* in
  `os-core`; concrete *adapters* implement them; the app layer depends only on
  ports; each binary is a composition root. Adding a backend = a new adapter, not
  a core edit.
- **License: MIT.**

This mirrors the sibling project **open-media** (`~/dev/personal/open-media`) on
purpose — same architecture, same discipline. open-media may depend on
`os-engine` as a library.

## Module layout

One crate per concern. To add a top-level concern, add `crates/os-<name>` and a
member entry in the root `Cargo.toml`. Crate prefix is `os-`.

| Crate | Owns | Implements |
|-------|------|------------|
| `crates/os-core` | Domain model, **port traits**, `CoreError`, and pure **scoring/matching**. **No I/O, no heavy deps.** | — (defines ports) |
| `crates/os-config` | Config schema, load/save, XDG paths, secrets policy. | — |
| `crates/os-identify` | Filename/release parsing, hashing (OSDB + others), refiners (AniList/AniDB/MAL, TMDB/TVDB/IMDb, local metadata). | `Identifier`, `Hasher`, `Refiner` |
| `crates/os-providers` | Subtitle source adapters (one module per source). | `Provider` |
| `crates/os-process` | Encoding→UTF-8, format conversion, mods (HI/OCR/common/color), archive extraction. | `PostProcessor` |
| `crates/os-sync` | Subtitle↔video/subtitle alignment (ffsubsync/alass). | `Synchronizer` |
| `crates/os-translate` | Translation backends (local-first, LLM optional). | `Translator` |
| `crates/os-transcribe` | Speech-to-text fallback (Whisper). | `Transcriber` |
| `crates/os-engine` | Use-cases + `Engine` composition + `Throttler`. **Depends only on `os-core`.** | — (consumes ports) |
| `crates/os-cli` | The `ost` binary: args, composition root, `--json`, TUI. | — (wires adapters) |
| `crates/os-daemon` | The `ostd` HTTP/JSON (+ SSE) server. | — (wires adapters) |
| `crates/os-ffi` | C-ABI (`libopensubtitle`) + WASM bindings. | — (wires adapters) |
| `crates/os-mpv` | mpv sidecar contract + thin Lua plugin. | — |

### The dependency rule (do not break this)

```
os-cli/os-daemon/os-ffi/os-mpv ──▶ os-engine ──▶ os-core ◀── every adapter crate
        │                                                        ▲
        └───────────────────── wires ─────────────────────────────┘
```

- `os-core` depends on nothing internal.
- `os-engine` depends on **only** `os-core`. It must never `use` an adapter crate.
  If you want to, you need a **new port** in `os-core` instead.
- Adapter crates depend on `os-core` (+ their own I/O deps). They never depend on
  each other or on `os-engine`.
- `os-cli`/`os-daemon`/`os-ffi`/`os-mpv` are the **only** crates allowed to name
  concrete adapters; they assemble the `Engine` in their `compose.rs`.

## Architecture in one screen

1. **Ports** (`os-core::ports`) are small, object-safe `async` traits:
   `Provider`, `Identifier`, `Hasher`, `Refiner`, `PostProcessor`, `Synchronizer`,
   `Translator`, `Transcriber`, `Scorer`, `CacheStore`.
2. **Adapters** implement a port each, mapping concrete errors → `CoreError` at
   the boundary.
3. **`Engine`** (`os-engine`) holds `Arc<dyn Port>` fields and implements the
   use-cases (`identify`, `search`, `download_best`, `fallback`, `auto`). Built by
   `EngineBuilder`; unset optional ports disable their features. A `Throttler`
   guards provider calls.
4. **Composition roots** (each frontend's `compose.rs`) read `Config` and choose
   which adapters to instantiate.

The matching/scoring model and provider catalog are specified in
`docs/ARCHITECTURE.md` (§5, §7) and `docs/RESEARCH.md` (§2, §8).

## Coding standards

- **SOLID, concretely:**
  - *SRP* — one reason to change per type/crate.
  - *OCP* — extend by adding an adapter/port impl, never by editing core/app.
  - *LSP* — every adapter honors its port's documented contract, including error
    semantics (return the right `CoreError` variant so the `Throttler` reacts).
  - *ISP* — keep ports narrow. A provider must not depend on translator types.
  - *DIP* — depend on `os-core` ports, never a concrete adapter, outside the
    frontends.
- **Errors:** ports return `os_core::CoreResult<T>`. Adapters convert explicitly
  (no blind `?` from `reqwest::Error` into `CoreError`). User-facing messages are
  actionable.
- **Async:** tokio. Don't block the runtime; use `spawn_blocking` for sync I/O.
  Network fan-out (`search`, refiners) parallelizes with bounded concurrency.
- **No secrets in code or logs.** Tokens/keys come only from `os-config`. Never
  log a token; mask in any display.
- **Keyless-by-default invariant:** a provider that needs a key ships **disabled**
  in the default config. Never regress this.
- **No runtime interpreter dep** on the core path. Optional tools are detected via
  `which`-style probes; their absence disables a feature, never aborts.
- **Formatting/lints:** `cargo fmt` + `cargo clippy --workspace --all-targets -D
  warnings` must be clean. Repo-wide lints live in the root `Cargo.toml`.
- **Tests:** pure logic (scoring, matching, hashing, parsing, encoding, config) is
  unit-tested with no network. Adapters are tested against recorded fixtures /
  in-process mock HTTP, not the live service. The engine is tested with fake
  ports. Live tests are gated `#[ignore]` + env.
- **Comments** explain *why* (a protocol quirk, a rate limit, a scoring choice),
  not *what*. Keep them factual.

## How to add a provider (the common task)

Example: add a `Foobar` subtitle source.

1. In `crates/os-providers/src/`, add `foobar.rs` with a `Foobar` struct that
   `impl Provider`. Declare `capabilities()` (media kinds, languages, needs_hash,
   needs_id). Map Foobar's API/HTML → `SubtitleCandidate` (fill every match field
   you can: ids, release strings, hi/forced). Map its errors → `CoreError`
   variants (use `RateLimited`/`DownloadLimit`/`AuthRequired` correctly).
2. Use the shared HTTP client + UA pool; don't hand-roll a session.
3. Export it from `os-providers`'s `lib.rs`.
4. In each frontend's `compose.rs`, select it when its config block is enabled
   (default **disabled** if it needs a key).
5. Add config keys to `os-config`.
6. Tests (fixture/mock) + `cargo clippy`/`fmt`. **No other crate changes** —
   that's OCP working.

Adding a new *capability* (not just a backend) means a new port trait in
`os-core::ports`, consumed by `os-engine`, wired in the frontends.

## Commands

```bash
cargo build                                   # debug build
cargo run -p os-cli -- search "frieren" -l en # run the CLI
cargo run -p os-cli -- get <file> -l en --best --json
cargo test --workspace                        # all tests
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
```

Smoke-test without touching real config by setting a throwaway `XDG_CONFIG_HOME`.

## Research material

The upstream projects analyzed for this design were cloned to `/tmp/ost-research`
during planning (subliminal, bazarr, subg, uosc, mpv-subversive, subtool,
ffsubsync, sublarr, animeSubs_dl). They are **not** part of this repo — re-clone
if you need to re-read them. `docs/RESEARCH.md` captures the distilled findings
with the concrete algorithms (OSDB hash, scoring weights, throttle map).

## Roadmap & planning

- `docs/STRATEGY.md` — the long-term direction (backend standard) + the committed
  decisions. **Read this first** to understand *why* the roadmap is ordered as it
  is.
- `docs/PROTOCOL.md` — the integration contract (daemon/FFI/WASM JSON shapes + the
  OpenSubtitles-compatible surface). Descriptive until frozen at `v0.5`.
- `docs/PLAN.md` — the phased build plan + acceptance criteria.
- `docs/ROADMAP.md` — version milestones and the feature matrix.
- `docs/RESEARCH.md` — prior-art analysis the design is built on.
- `docs/ARCHITECTURE.md` — the full design.
- `future-features.md` — backlog / nice-to-haves.
- `CHANGELOG.md` — keep-a-changelog; update `Unreleased` with each meaningful
  change.

When the contract (PROTOCOL.md) changes before `v0.5`, note it in `CHANGELOG.md`;
after `v0.5` it is semver-stable and guarded by the conformance suite.

When you finish a phase, tick its boxes in `docs/PLAN.md` and move anything
deferred into `future-features.md`.

## Conventions

- Repo: `github.com/0xfell/open-subtitle`. Binaries: `ost` (CLI), `ostd` (daemon).
  Library/FFI: `libopensubtitle`.
- Crate prefix `os-`. Provider modules are lowercase source names
  (`opensubtitles_org.rs`, `subdl.rs`, `jimaku.rs`).
- Prefer fixing the contract in `os-core` over working around it in an adapter.
- Keep the four "agnostic" axes intact (app / platform / media / language) with
  every change.
