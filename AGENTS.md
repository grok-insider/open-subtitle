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
| `crates/os-compose` | The shared **composition root** (`build_engine(&Config) -> Engine`) used by every frontend. The one place (besides binaries) allowed to name concrete adapters. | — (wires adapters) |
| `crates/os-cli` | The `ost` binary: args, `--json` sidecar, subcommands (`init/config/providers/identify/search/get/auto/sync/translate`). | — (uses `os-compose`) |
| `crates/os-daemon` | The `ostd` binary: HTTP/JSON server (`/capabilities`, `/identify`/`/search`/`/get`, the **OpenSubtitles-compatible** surface, Sonarr/Radarr **webhooks**). | — (uses `os-compose`) |
| `crates/os-ffi` | C-ABI `libopensubtitle` (cdylib/staticlib) + hand-written `include/opensubtitle.h`; WASM later. | — (uses `os-compose`) |
| `crates/os-mpv` | **Not a cargo crate** — the mpv Lua plugin asset (`open-subtitle.lua`) that drives `ost --json`. Bundled into the package + release archives. | — |

### The dependency rule (do not break this)

```
os-cli / os-daemon / os-ffi ──▶ os-compose ──▶ (adapters)
        │                            │
        └────────▶ os-engine ──▶ os-core ◀── every adapter crate
```

- `os-core` depends on nothing internal.
- `os-engine` depends on **only** `os-core`. It must never `use` an adapter crate.
  If you want to, you need a **new port** in `os-core` instead.
- Adapter crates depend on `os-core` (+ their own I/O deps). They never depend on
  each other or on `os-engine`.
- **`os-compose`** is the single shared composition root: it names the concrete
  adapters and builds the `Engine` from `Config`. The frontends (`os-cli`,
  `os-daemon`, `os-ffi`) call `os_compose::build_engine` rather than wiring
  adapters themselves — so they all behave identically.

## Architecture in one screen

1. **Ports** (`os-core::ports`) are small, object-safe `async` traits:
   `Provider`, `Identifier`, `Hasher`, `Refiner`, `PostProcessor`, `Synchronizer`,
   `Translator`, `Transcriber`, `Scorer`, `CacheStore`.
2. **Adapters** implement a port each, mapping concrete errors → `CoreError` at
   the boundary.
3. **`Engine`** (`os-engine`) holds `Arc<dyn Port>` fields and implements the
   use-cases (`identify`, `search`, `download_best`, `fetch_candidate`, `auto`,
   `sync_to`, `translate_to`). Built by `EngineBuilder`; unset optional ports
   disable their features. A `Throttler` guards provider calls.
4. **`os-compose::build_engine(&Config)`** reads config and instantiates the
   enabled adapters into the `Engine`. Every frontend calls it.

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
4. In **`os-compose::build_engine`**, select it when its config block is enabled
   (default **disabled** if it needs a key).
5. Add config keys to `os-config`.
6. Tests (fixture/mock) + `cargo clippy`/`fmt`. **No other crate changes** —
   that's OCP working.

Adding a new *capability* (not just a backend) means a new port trait in
`os-core::ports`, consumed by `os-engine`, wired in the frontends.

## Commands

```bash
cargo build                                   # debug build
cargo run -p os-cli -- search "frieren" -l en # run the CLI (ost)
cargo run -p os-cli -- get <file> -l en --json
cargo run -p os-daemon                         # run the daemon (ostd) on 127.0.0.1:4110
cargo test --workspace                        # all tests (hermetic)
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
nix build .#open-subtitle -L                   # the packaged build (ost+ostd+lib+plugin)
nix flake check --no-build                     # validate flake outputs
```

Smoke-test without touching real config by setting a throwaway `XDG_CONFIG_HOME`.

## Releases, CI & branch protection

- **`master` is protected:** changes land via **PR**, and the **`fmt + clippy +
  test`** check must pass (admins included — no direct pushes). The master-only
  `nix build + cachix push` job is *not* a required check (it's skipped on PRs).
- **CI** (`.github/workflows/ci.yml`): fmt + clippy + hermetic tests on every PR;
  on master/tags it also `nix build`s and pushes the closure to the `grok-insider`
  cachix cache.
- **Releases** (`.github/workflows/release.yml` + `release-plz.toml`):
  **release-plz** keeps a "release PR" updated (single workspace version driven by
  `os-cli`, all 12 other crates folded into the changelog; `git_only` + `publish =
  false`, no crates.io). Merging it tags `vX.Y.Z`, creates the GitHub Release, and
  a **cross-platform matrix** attaches prebuilt `ost`/`ostd`/`libopensubtitle` +
  the mpv plugin for Linux (x86_64/aarch64 static musl), macOS (x86_64/arm64), and
  Windows.
- **`RELEASE_PLZ_TOKEN`** (a PAT/App secret) lets the release PR run CI under the
  required-check rule; without it, approve the release PR's CI run manually.
- TLS is **rustls + ring** → no cmake/clang in the build path (lighter than
  open-media's aws-lc).

## Research material

The upstream projects analyzed for this design (subliminal, bazarr, subg, uosc,
mpv-subversive, subtool, ffsubsync, sublarr, animeSubs_dl) were cloned during
planning and are **not** part of this repo — re-clone to `/tmp` if you need to
re-read them. `docs/RESEARCH.md` captures the distilled findings with the concrete
algorithms (OSDB hash, scoring weights, throttle map).

## Roadmap & planning

- `docs/STRATEGY.md` — the long-term direction (backend standard) + the committed
  decisions. **Read this first** to understand *why* the roadmap is ordered as it
  is.
- `docs/PROTOCOL.md` — the integration contract (daemon/FFI/WASM JSON shapes + the
  OpenSubtitles-compatible surface). Descriptive until frozen at `v0.5`.
- `docs/PLAN.md` — the phased build plan + per-phase status (what's `[x]`/`[~]`).
- `docs/ROADMAP.md` — version milestones, the delivered snapshot, feature matrix.
- `continue-plan.md` — the **actionable backlog** (what's left, by theme + a
  suggested next-session order). Start here when picking up work.
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

- Repo: `github.com/grok-insider/open-subtitle`. Binaries: `ost` (CLI), `ostd` (daemon).
  Library/FFI: `libopensubtitle`.
- Crate prefix `os-`. Provider modules are lowercase source names
  (`opensubtitles_org.rs`, `subdl.rs`, `jimaku.rs`).
- Prefer fixing the contract in `os-core` over working around it in an adapter.
- Keep the four "agnostic" axes intact (app / platform / media / language) with
  every change.
