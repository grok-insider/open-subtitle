# Changelog

All notable changes to this project are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres
to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- **`v0.4` flagship — wanted list + scheduled re-search + library scan.** When an
  import (or a scan) can't find a subtitle yet — anime fansubs and slow releases
  often lag — `ostd` now records the gap and re-searches it on a timer until it's
  found, the long-stated automation flagship.
  - `os-engine::library` — pure, tested local-filesystem helpers shared by every
    surface: recursive video walk + per-language sidecar detection
    (`walk_videos`, `missing_languages`). Reads the language from the first
    sidecar tag, so `Movie.en.hi.srt` is English (not Hindi) and untagged
    `Movie.srt` matches nothing.
  - `os-daemon::wanted` — a persistent wanted list backed by a single JSON file
    (`<cache>/wanted.json`; no database dependency), with tested dedupe/merge,
    `due` (interval + attempt-cap) selection, and `record_result` pruning.
  - `ostd` background **scheduler** that re-searches due items every
    `automation.recheck_interval_secs`, dropping each language as it's delivered
    and the item once it's complete.
  - `ostd` endpoints: **`POST /scan`** (walk a library, fetch missing subs, queue
    the rest), **`GET /wanted`**, **`POST /wanted/run`** (force a pass now),
    **`DELETE /wanted`** / **`POST /wanted/clear`** — all also under `/v1`.
  - Sonarr/Radarr **webhook imports that come up empty now enqueue** the missing
    languages automatically (when `automation.track_wanted`).
  - **`ost scan <dir>`** CLI command (one-shot bulk fetch; `--dry-run`,
    `--no-recursive`, `-l`, `--hi`, `--json`).
  - `os-config` `[automation]` gains `track_wanted`, `recheck_interval_secs`
    (default 6h), and `max_attempts` (default unlimited).
  - Refactored the import/fetch/write path into one shared `fetch_for_target`
    helper so webhooks, scans, and re-searches behave identically.
  - Documented in `docs/PROTOCOL.md` §5b/§5c. Verified end-to-end against a live
    `ostd` (scan → wanted → run → clear) and via 15 new unit tests.

## [0.1.0](https://github.com/grok-insider/open-subtitle/releases/tag/v0.1.0) - 2026-06-25

### Added

- full toolchain, all frontends, and the mpv plugin (Phases 5-9)
- implement the keyless engine (Phases 0-4) in Rust

### Added
- **Release & distribution pipeline (Nix/Cachix + GitHub Actions + release-plz).**
  - `flake.nix` — builds `ost` + `ostd` + `libopensubtitle` (+ `opensubtitle.h`)
    and bundles the mpv plugin; reuses the `grok-insider` cachix cache; ships a Home
    Manager module (`programs.open-subtitle` with `mpv.enable` and an optional
    `daemon.enable` systemd user service) and a dev shell. (ring → no
    cmake/clang needed.)
  - `.github/workflows/ci.yml` — fmt + clippy + hermetic tests, plus a Nix build
    that pushes the x86_64-linux closure to cachix.
  - `.github/workflows/release.yml` — release-plz (versioning + changelog + tag +
    GitHub Release), then a **cross-platform matrix** that attaches prebuilt
    archives for Linux (x86_64/aarch64 static musl), macOS (x86_64/arm64), and
    Windows (x86_64) — each with `ost`, `ostd`, `libopensubtitle` (+ header), and
    the mpv plugin. So users install **without compiling**.
  - `release-plz.toml` — single workspace version driven by `os-cli`, folding all
    12 other crates into the changelog; `git_only` (no crates.io).
  - `crates/os-ffi/include/opensubtitle.h` — the C header shipped with the lib.
  - Validated locally: `nix build`/`nix flake check` green, `actionlint` clean.
- **`v0.4` automation MVP — Sonarr/Radarr webhooks.** `ostd` now accepts
  Sonarr/Radarr "On Import" webhooks (`POST /webhook`, `/webhook/sonarr`,
  `/webhook/radarr`) and fetches subtitles for the imported file automatically,
  writing sidecars next to it.
  - `os-daemon::webhook` — a pure, tested parser for Sonarr/Radarr payloads
    (eventType handling, Sonarr-vs-Radarr detection, path fallback from
    `relativePath`, anime detection via `series.type`).
  - `os-config` `[automation]` section: `enabled`, `languages`, `path_map`
    (prefix remap for containerized *-arr setups), `output_dir` fallback.
  - Builds the search `Media` from the authoritative payload (ids/title/season/
    episode) via `engine.identify`, hashing the file when reachable.
  - Documented in `docs/PROTOCOL.md` §5b.

### Fixed
- **OpenSubtitles.org search** now issues the `imdbid` and `query` searches
  **separately** and merges them. The endpoint returns nothing when `imdbid` and
  `query` are combined in one request (confirmed against the live API), which made
  webhook movie imports (which carry an IMDb id) return zero results.

### Added (earlier)
- **`v0.3` "the contract" — first slice (the wedge).**
  - **OpenSubtitles.com-compatible surface on `ostd`** so existing OpenSubtitles
    clients can be repointed at the local engine: `GET /osc/api/v1/subtitles`,
    `POST /osc/api/v1/download`, `GET /osc/file/<id>`, backed by a candidate
    registry. Verified end-to-end (search → download → SRT served locally).
  - `os-engine::Engine::fetch_candidate` — fetch + post-process a single
    candidate by id (used by the OSc `file` endpoint).
  - `ostd` **`GET /capabilities`** (providers, features, version, default langs),
    a **typed error envelope** (`{ error: { kind, message, retry_after_secs? } }`)
    with proper HTTP status codes, and **`/v1` path aliases**.
  - **JSON Schemas** under `docs/schemas/` for `Language`, `Media`,
    `SubtitleCandidate`, `SubtitleFile`, and the error envelope (draft 2020-12).
  - 57 tests pass (incl. pure OSc-mapping + `fetch_candidate` tests); clippy clean.
- **Long-term strategy + protocol direction (docs).**
  - `docs/STRATEGY.md` — commits the project to becoming the embeddable subtitle
    *backend standard* (stable protocol + provider SDK), with the
    OpenSubtitles-compatible `ostd` surface as the wedge and self-hosted media
    automation as the flagship application. Records the five committed decisions.
  - `docs/PROTOCOL.md` — descriptive spec of the integration contract
    (daemon/FFI/WASM JSON shapes grounded in the engine types + the planned
    OpenSubtitles-compatible REST surface); frozen as `v1` at v0.5.
  - `docs/ROADMAP.md` — re-sequenced around the strategy (`v0.3` the contract,
    `v0.4` automation MVP, `v0.5` freeze `v1` + packaging, `v0.6+` ecosystem).
  - `AGENTS.md` / `future-features.md` updated to point at the strategy as the
    source of direction.
- **Full toolchain + all frontends (Phases 5–9).**
  - `os-core::cue` — pure SRT/WebVTT/SSA·ASS parser + **ASS/VTT→SRT conversion**
    (strips override tags, `\N`→newline); wired into the post-processor so `get`
    delivers `.srt` even when the source was `.ass`.
  - `os-providers` — added **Jimaku** (anime, AniList-matched, key-optional).
  - `os-sync` — **ffsubsync** + **alass** adapters (runtime-detected).
  - `os-translate` — **LibreTranslate** adapter (per-cue, timing-preserving,
    local-first).
  - `os-transcribe` — **Whisper** CLI adapter (transcribe fallback).
  - `os-engine` — wired the optional sync/translate/transcribe ports + an `auto`
    pipeline (identify→download→sync→transcribe-fallback).
  - `os-compose` — shared composition root used by all frontends.
  - `os-daemon` — **`ostd`** HTTP/JSON server (`/health`, `/identify`, `/search`,
    `/get`).
  - `os-ffi` — **`libopensubtitle`** C-ABI (cdylib/staticlib) with JSON in/out.
  - `os-mpv` — the **mpv Lua plugin** that drives `ost --json` and `sub-add`s the
    result (auto + manual `mp.input` search, configurable keybinds).
  - `ost` gained `auto`/`sync`/`translate` subcommands and `--sync`/`--translate`
    flags on `get`.
  - OpenSubtitles.org now runs **separate hash and text searches and merges**
    them (a non-matching file hash no longer suppresses query results).
  - 50 tests pass; clippy clean. **Verified in real mpv** (via the `wisp` driver):
    the plugin downloaded a subtitle keyless and rendered it; ASS→SRT conversion,
    the `ostd` daemon, and `get` on a hashed file all confirmed live.
- **Working keyless engine (Phases 0–4).** A Rust workspace implementing the
  hexagonal design end-to-end:
  - `os-core` — domain model, ports, errors, and the pure two-layer
    matching/scorer (weights with `hash == sum(others)`, ID→field equivalences,
    hash-must-corroborate, episode safety gate) + a release-name parser and
    ISO-639 language bridging. 16 unit tests.
  - `os-config` — TOML config, XDG paths, keyless-by-default provider toggles.
  - `os-identify` — filename identifier, the OSDB file hash (known-vector tested),
    and a keyless AniList refiner for anime ids/titles.
  - `os-providers` — `opensubtitles_org` (keyless primary), `subdl` and
    `opensubtitles_com` (key-optional), with a shared HTTP client.
  - `os-process` — gzip/zip extraction, encoding→UTF-8 (BOM + chardet), format
    detection, and hearing-impaired removal.
  - `os-engine` — the `Engine` (identify/search/download_best/auto) with parallel
    provider fan-out and the Bazarr-style throttler (exception→cooldown +
    5-strikes-in-120s).
  - `os-cli` — the `ost` binary: `init`/`config`/`providers`/`identify`/`search`/
    `get`, each with a `--json` sidecar mode.
  - 45 tests pass; `cargo clippy --workspace --all-targets` is clean.
  - Verified live, **with no API key**: identify (+AniList enrich), keyless search
    (correct scores), and a real subtitle download (gunzip→UTF-8→sidecar).
- Initial project documentation and design (docs-only first commit):
  - `README.md` — vision, the four "agnostic" axes, pipeline, layout.
  - `docs/RESEARCH.md` — prior-art analysis of subliminal, Bazarr, subg,
    uosc/ziggy, mpv-subversive, subtool, ffsubsync (with concrete algorithms:
    OSDB hash, scoring weights, throttle map, encoding tables).
  - `docs/ARCHITECTURE.md` — hexagonal Rust workspace: ports, adapters, the
    `Engine`, and the CLI/daemon/FFI/WASM/mpv frontends.
  - `docs/PLAN.md` — phased build plan (Phases 0–10) with acceptance criteria.
  - `docs/ROADMAP.md` — version milestones + feature matrix vs. surveyed tools.
  - `AGENTS.md` — contributor/agent guide, module layout, dependency rule.
  - `CONTRIBUTING.md`, `future-features.md`, `LICENSE` (MIT), `.gitignore`.

[Unreleased]: https://github.com/grok-insider/open-subtitle/commits/master
