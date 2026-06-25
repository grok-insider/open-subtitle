# PLAN — phased build

The build is sequenced so each phase is independently useful and testable. Every
phase lists **deliverables** and **acceptance criteria** (the boxes to tick).
Phases 0–4 produce a genuinely useful keyless downloader; later phases add the
toolchain, frontends, automation, and packaging.

Legend: `[ ]` todo · `[~]` in progress / partial · `[x]` done.

> **Status (current):** Phases 0–10 are all at least partially landed and tested.
> The keyless engine, the toolchain (sync/translate/transcribe), all frontends
> (CLI, `ostd` daemon + OpenSubtitles-compatible surface, C-ABI, mpv plugin),
> Sonarr/Radarr automation, and the cross-platform release pipeline (Nix/Cachix +
> release-plz + GitHub Actions matrix) are in place; **v0.1.0 is released** with
> prebuilt binaries. The remaining `[ ]`/`[~]` items below are tracked, with the
> actionable backlog consolidated in [`../continue-plan.md`](../continue-plan.md).
> 50+ tests pass; clippy is clean; `master` is protected (PR + green CI required).

---

## Phase 0 — Workspace & core scaffolding ✅
Stand up the Cargo workspace and the pure core with no I/O.

- [x] Cargo workspace, `rust-toolchain.toml`, root clippy lints.
- [x] `os-core`: domain model (`Media`/`IdSet`/`Hashes`/`ReleaseInfo`/`Language`/
      `SubtitleCandidate`/`SubtitleFile`).
- [x] `os-core::ports`: all trait definitions compile (object-safe, async).
- [x] `CoreError` + `CoreResult`.
- [x] `os-config`: schema + load/save + XDG + secrets policy.

**Acceptance:** `cargo build --workspace` + `cargo clippy -D warnings` clean ✓.
(`os-core` is pure/no-internal-deps; explicit `wasm32` compile not yet verified.)

## Phase 1 — Identify & hash [~]
Turn a file/query into a rich `Media`.

- [x] `FilenameIdentifier` (movie/series/anime title + season/episode + release
      fields), with a unit-test corpus.
- [x] `OsdbHasher` with the known-vector test. `Hasher` trait.
- [ ] `LocalMetadataRefiner` (ffprobe/mediainfo; resolution/codecs/duration +
      embedded tracks) — **pending**.
- [x] `AniListRefiner` (keyless GraphQL: anilist/mal ids + canonical titles).
- [x] `Engine::identify` wiring (best-effort refiners).

**Acceptance:** `ost identify` emits correct ids/season/episode for anime + TV ✓;
OSDB hash matches reference vectors ✓.

## Phase 2 — Scoring & matching (pure) ✅
The crown-jewel algorithm, fully unit-tested.

- [x] `guess_matches`, `expand_equivalences`, `WeightedScorer` (invariant
      `hash == sum(others)`, subliminal-exact), dual `(score, score_without_hash)`.
- [x] `series_safety_gate`; hash-must-corroborate rule.
- [x] Weights tables for episode/movie. (Swappable anime profile — pending.)

**Acceptance:** fixture pairs rank as expected ✓; hash dominates but is rejected
without corroboration ✓; cross-series false positives gated out ✓.

## Phase 3 — First providers + the Engine search/best path [~]
Make it actually download, keyless.

- [~] Shared HTTP client (timeout + UA). Retry/Cloudflare layering — pending.
- [x] `Throttler` (exception→cooldown map + 5-strikes-in-120s). Persistence —
      pending (in-memory today).
- [x] Providers: **opensubtitles_org** (keyless). `subdl` (key-optional).
      `podnapisi` — pending.
- [x] `Engine::search` (parallel fan-out, score, sort) + `download_best`
      (+ `fetch_candidate`).
- [x] `os-process`: archive extract (zip + gzip) + encoding→UTF-8 + line-ending
      normalize.

**Acceptance:** `ost get <file> -l en` downloads a correct, UTF-8, validated `.srt`
from a keyless provider with **no API key** ✓; throttling backs off on 429s ✓.

## Phase 4 — `ost` CLI + `--json` sidecar + mpv plugin [~]
Ship the first real frontend(s).

- [x] `os-cli`: `init/config/providers/identify/search/get/auto/sync/translate`
      + `--json`.
- [ ] Interactive TUI (search → results → pick) — **pending** (subcommands cover
      the flow).
- [~] `os-mpv`: Lua plugin driving `ost --json` + `sub-add` (auto + manual
      `mp.input` search). In-player menu / per-directory cache — pending.
- [x] Nix flake + HM module (deploy binaries + mpv plugin).

**Acceptance:** in mpv, trigger search → subtitle loads, **no key** — reproduced &
fixed the original "5/day / no results" problem; verified under the `wisp`
driver ✓.

## Phase 5 — Anime excellence [~]
Be the best anime subtitle tool.

- [x] **jimaku** (free key, AniList-matched).
- [ ] **animetosho** (anidb episode id), **kitsunekko** (JP scrape).
- [ ] Archive-member scoring (pick best sub in a pack).
- [ ] Anime scoring profile (release-group/`.ass` boost).

**Acceptance:** Jimaku path works with a free key ✓; AnimeTosho/Kitsunekko +
member scoring pending.

## Phase 6 — Full post-processing [~]
Match Bazarr/subzero quality.

- [x] Format conversion SRT/VTT/SSA·ASS → SRT (tags stripped, `\N`→newline).
      MicroDVD FPS — pending.
- [~] Mods pipeline: `remove_HI` done; `OCR_fixes`/`common`/`color`/`reverse_rtl`
      — pending.
- [~] Encoding → UTF-8 (BOM + chardetng). Language-specific tables — pending.
- [ ] RAR/7z/xz extraction (today: zip + gzip).

**Acceptance:** ASS→SRT conversion + HI removal validated ✓; OCR/common/color +
extra archive formats pending.

## Phase 7 — Sync, translate, transcribe (the toolchain) ✅
Become a one-shot `auto`.

- [x] `os-sync`: ffsubsync + alass adapters (runtime-detected).
- [x] `os-translate`: LibreTranslate (local-first, per-cue).
- [x] `os-transcribe`: Whisper adapter.
- [x] `Engine::auto`: identify → download → sync → transcribe-fallback (+ `ost
      sync`/`translate` subcommands).

**Acceptance:** the pipeline wires end-to-end ✓. (External tools detected at
runtime; deep live-testing of each backend pending.)

## Phase 8 — More frontends [~]
True app-agnosticism.

- [x] `os-daemon` (`ostd`): HTTP/JSON, `/health`, `/capabilities`, `/identify`,
      `/search`, `/get`, the **OpenSubtitles-compatible** surface, and Sonarr/Radarr
      **webhooks**. SSE progress stream — pending.
- [x] `os-ffi`: C-ABI (`libopensubtitle`) + a hand-written `opensubtitle.h`.
      WASM (`wasm-bindgen`) build — pending.
- [ ] Example integrations (a tiny C and a JS caller) — pending.

**Acceptance:** external processes drive identify→get/search over HTTP and the
OSc surface end-to-end ✓; FFI builds and runs; WASM + examples pending.

## Phase 9 — More providers + hardening [~]
Breadth + resilience.

- [x] **opensubtitles_com** (key/login optional).
- [ ] **gestdown**, **tvsubtitles**, **addic7ed**, **embedded** (ffmpeg),
      **local_folder** — pending (needs a TVDB id refiner for Gestdown).
- [ ] Caching layer (refiner + listings) with TTLs — pending.

**Acceptance:** keyless primary + key-optional providers usable; throttle verified
✓; broader catalog + caching pending.

## Phase 10 — Packaging, CI, docs [~]
Release-ready.

- [x] CI: `fmt` + `clippy` + hermetic `test`; Nix build + cachix push (`0xfell`).
- [x] Cross-compiled release artifacts: Linux (x86_64/aarch64 static musl), macOS
      (x86_64/arm64), Windows (x86_64) — via release-plz + a GitHub Actions matrix.
- [x] release-plz versioning/changelog + Nix flake/HM module; SHA-256 checksums.
- [x] **Branch protection** on `master` (PR + `fmt + clippy + test` required,
      admins included).
- [~] User docs (README install/quickstarts ✓; per-frontend guides pending).
- [ ] `ostd` **container image**; signed builds + SBOM; nixpkgs/Homebrew/AUR —
      pending.

**Acceptance:** **v0.1.0 tagged** with prebuilt binaries + `libopensubtitle` + the
mpv plugin for all 5 targets ✓; README quickstarts verified ✓; container +
signing pending.

---

## Cross-cutting acceptance (every change)
- `cargo fmt --all` + `cargo clippy --workspace --all-targets -D warnings` clean.
- New pure logic has unit tests; new adapters have fixture/mock tests.
- No secret keys in code or logs; tokens masked in any display.
- `CHANGELOG.md` `Unreleased` updated; relevant boxes ticked; `master` changes
  land via PR with green CI.
