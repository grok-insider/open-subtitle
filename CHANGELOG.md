# Changelog

All notable changes to this project are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres
to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
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
