# open-subtitle

**One subtitle engine for everything.** Find, score, download, normalize, sync,
translate, and transcribe subtitles for **movies, series, and anime** — from
**any app, on any platform, in any language**, with **no API key required** for
the default sources.

> Status: **pre-implementation (design/docs).** This first commit is the research,
> architecture, plan, and contributor guide. Code lands in phases — see
> [docs/PLAN.md](docs/PLAN.md).

---

## Why

The subtitle world is a graveyard of single-purpose tools, each solving one slice
in one language for one host:

- [`subliminal`] — great provider/scoring library, but **Python-only** and
  library-shaped (you build the app).
- [`Bazarr`] — the most complete provider set and anti-ban knowledge anywhere, but
  a **Python web service** bolted to Sonarr/Radarr.
- [`uosc`]'s downloader — slick in-mpv UI, but **hardcoded to OpenSubtitles.com**
  behind a **shared public key capped at 5 downloads/day**.
- [`mpv-subversive`] — best-in-class **anime** subs (AniList + Jimaku), but
  **mpv-only and Lua**.
- [`subtool`] — a fantastic full pipeline (download → translate → sync →
  transcribe → embed), but a **4,500-line bash script**.
- [`subg`] — clean multi-provider CLI in Go, but **CLI-only and OpenSubtitles
  needs a key**.

Every one of them re-implements the same primitives (hashing, filename parsing,
scoring, encoding normalization, archive extraction, throttling) and then locks
them inside one frontend. **open-subtitle unifies the best ideas from all of them
into a single, fast, embeddable engine with many frontends** — so the same logic
serves an mpv plugin, a CLI, a daemon other apps call over HTTP, a C-ABI library,
and WASM.

[`subliminal`]: https://github.com/Diaoul/subliminal
[`Bazarr`]: https://github.com/morpheus65535/bazarr
[`uosc`]: https://github.com/tomasklaen/uosc
[`mpv-subversive`]: https://github.com/nairyosangha/mpv-subversive
[`subtool`]: https://github.com/maxgfr/subtool
[`subg`]: https://github.com/kakeetopius/subg

## The four kinds of "agnostic"

| Axis | What it means here |
|------|--------------------|
| **App-agnostic** | One engine, many surfaces: `ost` CLI, `ostd` HTTP/JSON daemon, C-ABI FFI (`libopensubtitle`), WASM, a Rust library crate, and a thin mpv Lua plugin. Any app drives the same core. |
| **Platform-agnostic** | A single **static Rust binary**. No Python/Node runtime, no system interpreter. Cross-compiles to Linux, macOS, Windows (x86_64 + aarch64). |
| **Media-agnostic** | Movies and live-action series (TMDB/TVDB/IMDb) **and** anime (AniList/AniDB/MAL) flow through one identify → match → score → download path, with anime episode-offset handling built in. |
| **Language-agnostic** | Every subtitle language, with automatic encoding detection → UTF-8, RTL handling, and pluggable translation (incl. **local**, key-free) + speech-to-text fallback. |

## Key design goals

- **No key, no cap, by default.** Ships with keyless sources (OpenSubtitles.org
  REST, Podnapisi, Gestdown/Addic7ed, TVsubtitles, AnimeTosho, Kitsunekko) and
  generous-anon ones (SubDL ≈ 300/day per IP). API keys and logins are **optional
  upgrades**, never a requirement. No more "5 downloads/day."
- **Best-of-breed matching.** A port of subliminal's two-layer match→score model:
  providers emit a set of match tags, a swappable scorer maps tags → integer via a
  weights table, **a file-hash match dominates but must be corroborated**, and an
  ID match (imdb/tmdb/tvdb/anilist) implies the identity fields it guarantees.
- **Robust by construction.** Bazarr's hard-won anti-ban model: per-provider
  exception→cooldown throttling with a "5 strikes in 120 s" soft counter, layered
  HTTP (timeout → retry → Cloudflare), and persistent throttle state.
- **A complete subtitle toolchain, not just a downloader.** Encoding
  normalization, format conversion (SRT/ASS/SSA/VTT/SUB), HI/OCR/cleanup mods,
  archive extraction, **sync** (ffsubsync/alass + a native path later),
  **translation** (local-first), and **transcription** fallback (Whisper) — all
  optional, all behind one orchestrator.
- **Clean architecture.** Ports-and-adapters + SOLID, one crate per concern, so a
  new provider/translator/player is a small isolated change, never a core edit.

## The pipeline

```
            ┌──────────────┐  identify   ┌───────────────────────┐
  any app ─▶│  frontend     │────────────▶│  Identifier            │ filename parse
  (cli/mpv/ │ (ost/ostd/ffi)│             │  (os-identify)         │ + OSDB/other hash
   daemon)  └──────┬───────┘             └──────────┬────────────┘ + refiners (AniList/
                   │  Media + ids + hashes          │               TMDB/TVDB/IMDb)
                   ▼                                ▼
            ┌──────────────┐   list      ┌───────────────────────┐
            │   Engine      │────────────▶│  Provider(s)           │ OpenSubtitles.org,
            │  (os-engine)  │             │  (os-providers)        │ SubDL, Gestdown,
            └──────┬───────┘   score ◀────┘ candidates             │ Podnapisi, Jimaku,
                   │  best candidate(s)                            │ AnimeTosho, embedded…
                   ▼
            ┌──────────────┐  normalize → convert → (sync) → (translate)
            │ Post-process  │◀───────────  encoding→UTF-8, mods (HI/OCR),
            │ (os-process…) │              ffsubsync/alass, local MT, Whisper
            └──────┬───────┘
                   ▼
            ┌──────────────┐
            │  delivered    │  sidecar file or stream, loaded into the player /
            │  subtitle     │  returned over FFI / written next to the video
            └──────────────┘
```

See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for the full design and
[docs/RESEARCH.md](docs/RESEARCH.md) for the prior-art analysis this is built on.

## What it will do (target capabilities)

- **Discover** the right subtitle by file hash, filename guess, or explicit
  ids/query — for movies, series, and anime.
- **Search many providers in parallel**, score every candidate on one scale, and
  pick the best per language.
- **Download & normalize**: extract archives, detect encoding → UTF-8, convert
  format, optionally strip hearing-impaired/fix OCR.
- **Sync** a subtitle to the actual audio (ffsubsync/alass adapters).
- **Translate** into your language — **local/offline first** (LibreTranslate or
  the host's own translation stack), with optional cloud/LLM backends.
- **Transcribe** from audio (Whisper) when no subtitle exists anywhere.
- **Serve all of the above** to any app via CLI, HTTP/JSON, FFI, or the mpv plugin.

## Frontends (integration surfaces)

| Surface | Crate | For |
|---------|-------|-----|
| `ost` CLI | `os-cli` | humans + scripts + an interactive TUI search |
| `ostd` daemon | `os-daemon` | any app, over local **HTTP/JSON** (+ SSE) |
| `libopensubtitle` | `os-ffi` | native apps embedding via **C ABI** |
| WASM module | `os-ffi` (wasm) | browser/edge |
| mpv plugin | `os-mpv` | a thin **Lua** client driving the binary (sidecar/JSON) |
| Rust library | `os-engine` | other Rust apps (e.g. **open-media**) |

## Configuration (planned)

A single TOML file at `~/.config/open-subtitle/config.toml` (respects
`XDG_CONFIG_HOME`). **Secrets live only here** — never in the binary, repo, or
store. Keyless sources work out of the box; everything below is optional.

| Key | Required | Purpose |
|-----|----------|---------|
| `languages` | ➖ | preference order, e.g. `["en","es"]` (default from locale) |
| `providers.*` | ➖ | enable/disable + per-provider options |
| `opensubtitles_com.api_key` / login | ➖ | higher OpenSubtitles.com limits |
| `subdl.api_key` | ➖ | SubDL key (anon works without it) |
| `jimaku.api_key` | ➖ | **anime** subs via Jimaku (free key) |
| `translate.backend` | ➖ | `local` / `libretranslate` / `llm` / `none` |
| `sync.backend` | ➖ | `ffsubsync` / `alass` / `none` |

## Project layout

A Cargo workspace, one crate per concern (full table in
[AGENTS.md](AGENTS.md#module-layout)):

```
crates/
  os-core        domain model + ports (traits) + scoring/matching   — no I/O
  os-config      config schema + load/save + secrets policy
  os-identify    filename parsing + hashing + refiners (AniList/TMDB/TVDB/IMDb)
  os-providers   provider adapters (OpenSubtitles.org/.com, SubDL, Gestdown,
                 Podnapisi, TVsubtitles, Jimaku, AnimeTosho, Kitsunekko, embedded…)
  os-process     encoding→UTF-8, format convert, mods (HI/OCR), archive extract
  os-sync        subtitle↔video sync (ffsubsync/alass adapters)
  os-translate   translation adapters (local-first, LLM optional)
  os-transcribe  speech-to-text fallback (Whisper adapters)
  os-engine      use-cases + Engine (composition)                    — depends only on os-core
  os-cli         the `ost` binary (composition root + TUI)
  os-daemon      the `ostd` HTTP/JSON server
  os-ffi         C-ABI + WASM bindings (libopensubtitle)
  os-mpv         mpv sidecar contract + thin Lua plugin
```

## License

[MIT](LICENSE) © 2026 0xfell.

This is a client for services you bring your own account to (where applicable)
and for public subtitle indexes. You are responsible for complying with the laws
of your jurisdiction and the terms of those services. Subtitle files carry the
rights of their authors/uploaders.
