# future-features — backlog & nice-to-haves

Things intentionally **out of the near-term path**, kept here so they aren't lost.
Most layer on top of `ostd`/FFI rather than baking into core.

> Prioritization note: the *direction* is set in `docs/STRATEGY.md`. Several items
> below have since been **promoted onto the roadmap** as the strategic core — the
> OpenSubtitles-compatible endpoint and `/v1` contract (→ `v0.3`), media-server
> automation (→ `v0.4`), and the WASM provider SDK + standalone automation
> (→ `v0.6+`). They remain listed here for detail; `docs/ROADMAP.md` is the
> sequencing source of truth.

## More providers
- **OpenSubtitles.org via the legacy XML-RPC** as a fallback to the REST host.
- **Private trackers** (HDBits, AvistaZ/CinemaZ, KaraGarga) — cookies/passkey;
  optional, opt-in, likely as a separate `os-providers-private` crate.
- **Regional/community sources** Bazarr supports (Titlovi, LegendasDivX, Titulky,
  Wizdom, Subdivx, Zimuku/Assrt for zh, GreekSubs, etc.) — add by demand.
- **subscene/subf2m successors** if a stable endpoint exists.

## Identification & matching
- **anitomy-grade** anime filename parser (absolute episode numbering, batches,
  versioned releases `v2`/`v3`, multi-season packs).
- **Perceptual/audio fingerprint** matching when hash + filename both fail.
- **AniDB ↔ AniList ↔ TVDB mapping table** vendored for offset correctness.
- A **TMDB/TVDB-free** identification path using only AniList + IMDb where possible.

## Post-processing
- **Native sync** (VAD + FFT cross-correlation) in Rust to drop the ffsubsync
  external dependency; `alass`-style variable-offset alignment.
- **Styling-aware ASS handling** (keep karaoke/positioning when converting).
- **Mojibake auto-repair** beyond encoding guess (ftfy-equivalent passes).
- **Dual-language merge** (subtool's `mix`) — pair lines by index or timestamp.

## Translation & transcription
- **Streaming translation** for very large files; glossary/term locking.
- **Host-stack integration** (the NixOS speech-stack: TranslateGemma + Whisper)
  as first-class local backends.
- **On-the-fly translate-while-watching** via the mpv plugin.

## Frontends & integrations
- **GUI app** (Tauri/egui) over `ostd`.
- **Browser extension** for streaming sites (over WASM/FFI).
- **Media-server plugins**: Jellyfin/Kodi/Plex agents that call `ostd`.
- **Sonarr/Radarr webhook** mode (Bazarr-like library automation) — `ostd`
  consumes import webhooks and back-fills subs.
- **open-media integration**: `open-media` calls `os-engine` directly so its mpv
  playback gets subtitles inline.

## Operability
- **Caching backends** beyond memory/file (redis/sqlite) behind `CacheStore`.
- **Cloudflare/captcha** resilience module (cloudscraper-equivalent) for scraper
  providers that need it.
- **Metrics/telemetry opt-in** for provider success rates to auto-rank providers.
- **Proxy / custom-DNS** support (Bazarr-style) for blocked sources.

## Packaging
- **Homebrew / winget / AUR** in addition to Nix.
- **Container** image for `ostd`.
- **Prebuilt mpv plugin bundle** (binary + Lua) installable without Rust.
