# Changelog

All notable changes to this project are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres
to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
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
