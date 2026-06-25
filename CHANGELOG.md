# Changelog

All notable changes to this project are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres
to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
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

[Unreleased]: https://github.com/0xfell/open-subtitle/commits/master
