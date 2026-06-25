# ROADMAP — milestones & feature matrix

Version milestones map groups of [PLAN.md](PLAN.md) phases to releases. Dates are
intentionally omitted; milestones ship when their acceptance criteria pass.

## Milestones

### `v0.1.0` — "Keyless downloader" (Phases 0–4)
The thing that motivated the project: find and load a subtitle in mpv/CLI with
**no API key and no daily cap**.
- Core model + ports, identify + OSDB hash, the scorer.
- Keyless providers (OpenSubtitles.org, Podnapisi, SubDL-anon).
- `ost` CLI (+ `--json`) and the mpv plugin.
- Nix flake / HM module.

### `v0.2.0` — "Anime-grade" (Phase 5)
Best-in-class anime: AnimeTosho + Jimaku + Kitsunekko, archive-member scoring,
anime scoring profile.

### `v0.3.0` — "Clean subtitles" (Phase 6)
Format conversion + the mods pipeline (HI/OCR/common/color) + language-aware
encoding + full archive support.

### `v0.4.0` — "The toolchain" (Phase 7)
`auto`: download → sync (ffsubsync/alass) → translate (local-first) → transcribe
fallback (Whisper).

### `v0.5.0` — "Everywhere" (Phase 8)
`ostd` HTTP/JSON daemon, C-ABI + WASM bindings, example integrations.

### `v0.6.0` — "Breadth & hardening" (Phase 9)
OpenSubtitles.com + Gestdown + TVsubtitles + Addic7ed + embedded + local-folder;
caching; resilience.

### `v1.0.0` — "Stable & packaged" (Phase 10)
CI, cross-platform release artifacts, signed builds, complete docs, stable
config + FFI/JSON contracts.

## Feature matrix (target vs. surveyed tools)

| Capability | open-subtitle (target) | subliminal | Bazarr | uosc | mpv-subversive | subtool |
|---|---|---|---|---|---|---|
| No-key default sources | ✅ | partial | partial | ❌ (5/day) | anime-key | ✅ |
| Movies + Series | ✅ | ✅ | ✅ | ✅ | ❌ | ✅ |
| Anime (AniList/AniDB/Jimaku) | ✅ | ❌ | ✅ | ❌ | ✅ | ❌ |
| Two-layer scoring | ✅ | ✅ | ✅ | ❌ | ❌ | ❌ |
| OSDB + other hashes | ✅ | ✅ | ✅ | ✅ (OSDB) | ❌ | ❌ |
| Anti-ban throttling | ✅ | partial | ✅ | ❌ | ❌ | ❌ |
| Encoding→UTF-8 (lang-aware) | ✅ | ✅ | ✅ | ❌ | ❌ | partial |
| Mods (HI/OCR/clean) | ✅ | ❌ | ✅ | ❌ | ❌ | partial |
| Format convert (srt/ass/vtt) | ✅ | ✅ | ✅ | ❌ | ❌ | ✅ |
| Sync (ffsubsync/alass) | ✅ | ❌ | ✅ | ❌ | ❌ | ✅ |
| Translate (local-first) | ✅ | ❌ | partial | ❌ | ❌ | ✅ |
| Transcribe fallback (Whisper) | ✅ | ❌ | ✅ (provider) | ❌ | ❌ | ✅ |
| CLI | ✅ | ✅ | ❌ | ❌ | ❌ | ✅ |
| HTTP/JSON daemon | ✅ | ❌ | ✅ (web) | ❌ | ❌ | ❌ |
| C-ABI / WASM | ✅ | ❌ | ❌ | ❌ | ❌ | ❌ |
| mpv plugin | ✅ | ❌ | ❌ | ✅ | ✅ | ❌ |
| Single static binary | ✅ | ❌ (Python) | ❌ (Python) | ❌ | ❌ | ❌ (bash) |

## Guiding constraints

- **Never regress "keyless by default."** A new provider that needs a key ships
  **disabled**.
- **Never add a runtime interpreter dependency** to the core path. Optional tools
  (ffmpeg/ffsubsync/whisper) are detected, not required.
- **Stable contracts from v1.0**: the config schema and the `--json`/HTTP/FFI
  shapes are versioned and backward-compatible thereafter.

## Out of scope (for now) — see [../future-features.md](../future-features.md)
Private-tracker providers, captcha-farm integrations, a GUI app, browser
extension, and media-server-specific plugins (Plex/Jellyfin/Kodi) live in the
backlog, layered on top of `ostd`/FFI rather than baked into core.
