# PLAN — phased build

The build is sequenced so each phase is independently useful and testable. Every
phase lists **deliverables** and **acceptance criteria** (the boxes to tick).
Phases 0–4 produce a genuinely useful keyless downloader; later phases add the
toolchain and frontends.

Legend: `[ ]` todo · `[~]` in progress · `[x]` done.

---

## Phase 0 — Workspace & core scaffolding
Stand up the Cargo workspace and the pure core with no I/O.

- [ ] Cargo workspace, `rust-toolchain.toml`, root clippy lints (deny warnings).
- [ ] `os-core`: domain model (`Media`/`Episode`/`Movie`/`IdSet`/`Hashes`/
      `ReleaseInfo`/`Language`/`SubtitleCandidate`/`SubtitleFile`).
- [ ] `os-core::ports`: all trait definitions compile (object-safe, async).
- [ ] `CoreError` + `CoreResult`.
- [ ] `os-config`: schema + load/save + XDG + secrets policy.

**Acceptance:** `cargo build --workspace` + `cargo clippy -D warnings` clean;
`os-core` has zero internal deps and compiles to `wasm32`.

## Phase 1 — Identify & hash
Turn a file/query into a rich `Media`.

- [ ] `FilenameIdentifier` (movie/series/anime title + season/episode + release
      fields), with a big unit-test corpus of real release names.
- [ ] `OsdbHasher` with the known-vector test (RESEARCH §3). `Hasher` trait.
- [ ] `LocalMetadataRefiner` (ffprobe/mediainfo; no network; sets
      resolution/codecs/duration + embedded tracks).
- [ ] `AniListRefiner` (keyless GraphQL: anilist/anidb/mal ids + offsets).
- [ ] `Engine::identify` wiring (parallel, best-effort refiners).

**Acceptance:** given a real anime and a real TV filename, `ost identify` emits
correct ids/season/episode; OSDB hash matches reference vectors.

## Phase 2 — Scoring & matching (pure)
The crown-jewel algorithm, fully unit-tested.

- [ ] `guess_matches`, `expand_equivalences`, `WeightedScorer`
      (`hash == sum(others) − 1`), dual `(score, score_without_hash)`.
- [ ] `series_safety_gate`; hash-must-corroborate rule.
- [ ] Weights tables for episode/movie (+ an anime profile).

**Acceptance:** a fixture set of (candidate, media) pairs ranks exactly as
expected; hash dominates but is rejected without corroboration; cross-series
false positives are gated out.

## Phase 3 — First providers + the Engine search/best path
Make it actually download, keyless.

- [ ] Shared HTTP client (timeout → retry → optional Cloudflare) + UA pool.
- [ ] `Throttler` (exception→cooldown map + 5-strikes-in-120s + persistence).
- [ ] Providers: **opensubtitles_org**, **podnapisi**, **subdl** (anon).
- [ ] `Engine::search` (parallel fan-out, score, sort) + `download_best`.
- [ ] `os-process` minimal: archive extract (zip) + encoding→UTF-8 +
      line-ending normalize.

**Acceptance:** `ost get <file> -l en --best` downloads and writes a correct,
UTF-8, validated `.srt` from a keyless provider, with **no API key configured**.
Throttling backs off on simulated 429s.

## Phase 4 — `ost` CLI + `--json` sidecar + mpv plugin
Ship the first real frontend(s).

- [ ] `os-cli`: `identify/search/get/providers` + `--json` for each.
- [ ] Interactive TUI (search → results → pick → write/print).
- [ ] `os-mpv`: Lua plugin driving `ost --json` (uosc/ziggy pattern) + in-player
      menu + `sub-add`; per-directory lookup cache.
- [ ] Nix flake + HM module (deploy binary + mpv script).

**Acceptance:** in mpv, trigger search → pick → subtitle loads, **no key**, on a
streamed file (reproduces and fixes the original "5/day / no results" problem).
Verified under the `wisp` driver.

## Phase 5 — Anime excellence
Be the best anime subtitle tool.

- [ ] Providers: **animetosho** (anidb episode id, xz), **jimaku** (free key,
      AniList-matched, offset handling), **kitsunekko** (JP scrape).
- [ ] Archive member scoring (pick best sub in a pack, not the largest).
- [ ] Anime scoring profile (release-group/`.ass` boost).

**Acceptance:** for a SubsPlease/Erai-raws/raw anime episode, the correct EN (and
JP if requested) sub is found and loaded; Jimaku path works with a free key.

## Phase 6 — Full post-processing
Match Bazarr/subzero quality.

- [ ] Format conversion SRT⇆ASS/SSA/VTT/MicroDVD (FPS-aware).
- [ ] Mods pipeline: `remove_HI`, `OCR_fixes`, `common`, `color`, `reverse_rtl`.
- [ ] Language-aware encoding tables + mojibake fix.
- [ ] RAR/7z/xz extraction.

**Acceptance:** HI removal, OCR fixes, and format conversion produce valid output
across a multi-language fixture set; original-format (`.ass`) opt-in preserved.

## Phase 7 — Sync, translate, transcribe (the toolchain)
Become a one-shot `auto`.

- [ ] `os-sync`: ffsubsync + alass adapters (external-tool detection).
- [ ] `os-translate`: local-first (LibreTranslate / host MT) + LLM optional.
- [ ] `os-transcribe`: Whisper adapter.
- [ ] `Engine::auto`: identify → search → best → sync → translate → (transcribe
      fallback).

**Acceptance:** `ost auto <file> -l es` downloads, syncs, and (if needed)
translates a correct Spanish sub; with no online sub + transcribe enabled, it
generates one from audio.

## Phase 8 — More frontends
True app-agnosticism.

- [ ] `os-daemon` (`ostd`): HTTP/JSON + SSE + `/health`.
- [ ] `os-ffi`: C ABI (`cbindgen`) + WASM (`wasm-bindgen`); JSON in/out.
- [ ] Example integrations (a tiny C and a JS caller).

**Acceptance:** an external process drives a full identify→get over HTTP and over
FFI; the WASM build runs in a browser harness.

## Phase 9 — More providers + hardening
Breadth + resilience.

- [ ] Providers: **opensubtitles_com** (key/login optional), **gestdown**,
      **tvsubtitles**, **addic7ed**, **embedded** (ffmpeg), **local_folder**.
- [ ] Captcha/Cloudflare resilience where needed; per-provider quota resets.
- [ ] Caching layer (refiner + listings) with TTLs.

**Acceptance:** ≥10 providers usable; throttle/persistence verified; a flaky
provider never breaks a run.

## Phase 10 — Packaging, CI, docs
Release-ready.

- [ ] CI: build + clippy + test (hermetic) on Linux/macOS/Windows; cross-compiled
      release artifacts (x86_64 + aarch64).
- [ ] `cargo-dist`/Nix release; signed checksums.
- [ ] User docs (per-frontend), provider matrix, `CHANGELOG` discipline.

**Acceptance:** `v0.1.0` tagged with binaries, the mpv plugin, and an `ostd`
container; README quickstarts verified.

---

## Cross-cutting acceptance (every phase)
- `cargo fmt --all` + `cargo clippy --workspace --all-targets -D warnings` clean.
- New pure logic has unit tests; new adapters have fixture/mock tests.
- No secret keys in code or logs; tokens masked in any display.
- `CHANGELOG.md` `Unreleased` updated; relevant `PLAN.md` boxes ticked.
