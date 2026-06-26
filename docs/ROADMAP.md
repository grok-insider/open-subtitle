# ROADMAP — milestones & feature matrix

Milestones realize the direction set in [STRATEGY.md](STRATEGY.md) and depend on
the contract in [PROTOCOL.md](PROTOCOL.md). Dates are omitted; milestones ship
when their acceptance criteria pass.

## Delivered to date (snapshot)

The engine, all frontends, automation, and the release pipeline exist and are
tested (see `CHANGELOG.md`):

- **Engine:** hexagonal Rust workspace, keyless OpenSubtitles.org + key-optional
  (SubDL, OpenSubtitles.com, Jimaku), OSDB hashing, the two-layer scorer, the
  throttler, encoding→UTF-8, ASS/VTT→SRT conversion, and the
  sync/translate/transcribe adapters (`auto`).
- **Frontends:** the `ost` CLI (+`--json`), the `ostd` daemon (with
  `/capabilities`, a typed error envelope, the **OpenSubtitles-compatible**
  surface, and **Sonarr/Radarr webhooks**), the `libopensubtitle` C-ABI, and the
  mpv plugin (verified loading subtitles in real mpv).
- **Distribution:** **v0.1.0 released** with prebuilt binaries for Linux
  (x86_64/aarch64), macOS (x86_64/arm64), and Windows; Nix/Cachix (`grok-insider`),
  release-plz, and branch-protected CI (PR + green checks required).
- **Contract:** `docs/PROTOCOL.md` + JSON Schemas under `docs/schemas/`.

What remains is **commitment** (freezing the contract + a conformance suite) and
**reach** (the automation wanted-list, provider breadth, and broader
distribution). The actionable backlog lives in
[`../continue-plan.md`](../continue-plan.md).

## Milestones

### `v0.1` — "Keyless downloader" ✅ delivered
Find & load a subtitle with **no API key, no daily cap**: engine, keyless
OpenSubtitles.org, the scorer, `ost` CLI (+`--json`), the mpv plugin.

### `v0.2` — "Anime-grade + clean subtitles" (mostly delivered)
Jimaku (AniList-matched) + ASS/VTT→SRT conversion + encoding normalization are
done. **Remaining:** AnimeTosho/Kitsunekko, archive-member scoring, the mods
pipeline (HI/OCR/common/color), and RAR/7z/xz extraction.

### `v0.3` — "The contract" (the strategic core) — mostly delivered
Turn today's JSON into a real, documented backend contract.
- ✅ `PROTOCOL.md` spec + **JSON Schemas** under `docs/schemas/`.
- ✅ `ostd` `/capabilities`, the **typed error envelope**, and `/v1` aliases.
- ✅ **The wedge:** the **OpenSubtitles-compatible** REST surface on `ostd`
  (search → download → file), so existing OpenSubtitles.com clients can be
  repointed at our local engine.
- **Remaining:** OSc hardening (`/login`, header tolerance, `/infos/*`,
  `moviehash`), `POST /get` with a `Media` body, an **SSE progress stream**, a
  **conformance suite**, and the `ost plugin install <app>` self-installer.

### `v0.4` — "Automation MVP"
Own the recurring use case without rebuilding a UI.
- ✅ **Sonarr/Radarr webhook consumer** (fetch subs on import; `path_map` for
  containers) — delivered.
- **Remaining:** a library scan + a "wanted" list (re-search until found).
- In-tree **provider breadth**: Gestdown, TVsubtitles, Addic7ed, embedded
  (ffmpeg), local-folder, plus an id-refiner (TVDB) where needed.
- Caching + resilience hardening.

### `v0.5` — "Stable & packaged" (freeze `v1`)
- ✅ **Release pipeline**: Nix/Cachix (`grok-insider`) + release-plz + a GitHub Actions
  cross-platform matrix shipping prebuilt `ost`/`ostd`/`libopensubtitle` + the mpv
  plugin for Linux (x86_64/aarch64), macOS (x86_64/arm64), and Windows — delivered.
- ✅ **Branch-protected CI**: `master` requires a PR + a green `fmt + clippy +
  test` check (admins included).
- **Remaining:** freeze `v1` (semver-stable `/v1` HTTP shapes + C-ABI symbols +
  core JSON types) + a **conformance suite**; signed builds + SBOM; a glibc `.so`
  release leg; presence in **nixpkgs / Homebrew / AUR** + a **container image**
  for `ostd`; a `curl | sh` installer + `ost plugin install`.

### `v0.6+` — "Ecosystem"
- **WASM provider SDK** (wasmtime component model) + a signed provider registry —
  community sources without forking.
- WASM build of `libopensubtitle`; a browser extension on top of it.
- **Reference clients / addons:** VLC (Lua), IINA (JS), and daemon-based Kodi /
  Jellyfin / Bazarr integrations.
- A **standalone automation** mode/UI — only if usage justifies it.

### `v1.0` — "The standard"
The protocol is adopted by multiple third-party clients; broad provider coverage;
open-subtitle is the embeddable subtitle backend others depend on.

## Feature matrix (target vs. surveyed tools)

| Capability | open-subtitle | subliminal | Bazarr | uosc | mpv-subversive | subtool |
|---|---|---|---|---|---|---|
| No-key default sources | ✅ | partial | partial | ❌ (5/day) | anime-key | ✅ |
| Movies + Series | ✅ | ✅ | ✅ | ✅ | ❌ | ✅ |
| Anime (AniList/AniDB/Jimaku) | ✅ | ❌ | ✅ | ❌ | ✅ | ❌ |
| Two-layer scoring | ✅ | ✅ | ✅ | ❌ | ❌ | ❌ |
| OSDB + other hashes | ✅ | ✅ | ✅ | ✅ (OSDB) | ❌ | ❌ |
| Anti-ban throttling | ✅ | partial | ✅ | ❌ | ❌ | ❌ |
| Encoding→UTF-8 (lang-aware) | ✅ | ✅ | ✅ | ❌ | ❌ | partial |
| Mods (HI/OCR/clean) | partial | ❌ | ✅ | ❌ | ❌ | partial |
| Format convert (srt/ass/vtt) | ✅ | ✅ | ✅ | ❌ | ❌ | ✅ |
| Sync (ffsubsync/alass) | ✅ | ❌ | ✅ | ❌ | ❌ | ✅ |
| Translate (local-first) | ✅ | ❌ | partial | ❌ | ❌ | ✅ |
| Transcribe fallback (Whisper) | ✅ | ❌ | ✅ (provider) | ❌ | ❌ | ✅ |
| CLI | ✅ | ✅ | ❌ | ❌ | ❌ | ✅ |
| HTTP/JSON daemon | ✅ | ❌ | ✅ (web) | ❌ | ❌ | ❌ |
| OpenSubtitles-compatible API | ✅ | ❌ | ❌ | ❌ | ❌ | ❌ |
| C-ABI / WASM | ✅ / ⏳ | ❌ | ❌ | ❌ | ❌ | ❌ |
| Provider SDK (sandboxed) | ⏳ | partial (entrypoints) | ❌ | ❌ | ❌ | ❌ |
| Media-server automation | ✅ (webhooks) | ❌ | ✅ | ❌ | ❌ | partial |
| Prebuilt cross-platform binaries | ✅ | ❌ | ❌ | ❌ | ❌ | ❌ |
| mpv plugin | ✅ | ❌ | ❌ | ✅ | ✅ | ❌ |
| Single static binary | ✅ | ❌ (Python) | ❌ (Python) | ❌ | ❌ | ❌ (bash) |

## Guiding constraints

- **Never regress "keyless by default."** A new provider that needs a key ships
  **disabled**.
- **Never add a runtime interpreter dependency** to the core path. Optional tools
  (ffmpeg/ffsubsync/whisper) are detected, not required.
- **Backend-first.** Every integration is a client of the contract
  ([PROTOCOL.md](PROTOCOL.md)); plugins are reference clients, not bespoke forks.
- **Stable contracts from `v0.5`** (not v1.0): the `v1` HTTP/FFI/JSON shapes are
  versioned and backward-compatible thereafter, guarded by the conformance suite.

## Out of scope / deferred — see [../future-features.md](../future-features.md)
A first-party GUI app, captcha-farm and private-tracker providers, and cloud
features are out of scope. Media-server and extra-player integrations layer on top
of `ostd`/FFI/WASM rather than being baked into core.
