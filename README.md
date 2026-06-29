# open-subtitle

[![CI](https://github.com/grok-insider/open-subtitle/actions/workflows/ci.yml/badge.svg)](https://github.com/grok-insider/open-subtitle/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/grok-insider/open-subtitle?sort=semver)](https://github.com/grok-insider/open-subtitle/releases/latest)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

**One subtitle engine for everything.** Find, score, download, normalize, sync,
translate, and transcribe subtitles for **movies, series, and anime** — from
**any app, on any platform, in any language**, with **no API key required** for
the default source.

> **Status: released — v0.2.0.** The engine and all frontends (`ost` CLI, `ostd`
> daemon, the `libopensubtitle` C-ABI library, and the mpv plugin) are
> implemented and shipped, along with the full sync/translate/transcribe
> toolchain and Sonarr/Radarr automation. See [CHANGELOG.md](CHANGELOG.md) and
> [docs/ROADMAP.md](docs/ROADMAP.md) for what's next.

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
serves an mpv plugin, a CLI, a daemon other apps call over HTTP, and a C-ABI
library.

[`subliminal`]: https://github.com/Diaoul/subliminal
[`Bazarr`]: https://github.com/morpheus65535/bazarr
[`uosc`]: https://github.com/tomasklaen/uosc
[`mpv-subversive`]: https://github.com/nairyosangha/mpv-subversive
[`subtool`]: https://github.com/maxgfr/subtool
[`subg`]: https://github.com/kakeetopius/subg

## The four kinds of "agnostic"

| Axis | What it means here |
|------|--------------------|
| **App-agnostic** | One engine, many surfaces: the `ost` CLI, the `ostd` HTTP/JSON daemon, a C-ABI FFI (`libopensubtitle`), a Rust library crate, and a thin mpv Lua plugin. Any app drives the same core. (A WASM surface is on the roadmap.) |
| **Platform-agnostic** | A single **static Rust binary**. No Python/Node runtime, no system interpreter. Prebuilt for Linux, macOS, and Windows (x86_64 + aarch64). |
| **Media-agnostic** | Movies and live-action series (IMDb/TMDB/TVDB) **and** anime (AniList) flow through one identify → match → score → download path, with anime episode handling built in. |
| **Language-agnostic** | Every subtitle language, with automatic encoding detection → UTF-8, format conversion, and optional translation (LibreTranslate/local) + speech-to-text fallback (Whisper). |

## Key design goals

- **Keyless by default.** Out of the box it searches **OpenSubtitles.org** with no
  API key or login. Additional providers — **SubDL**, **OpenSubtitles.com**, and
  **Jimaku** (anime) — are optional upgrades you enable with a (mostly free) key.
  A provider that needs a key ships disabled; the keyless path always works.
- **Best-of-breed matching.** A port of subliminal's two-layer match→score model:
  providers emit a set of match tags, a swappable scorer maps tags → integer via a
  weights table, **a file-hash match dominates but must be corroborated**, and an
  ID match (imdb/tmdb/tvdb/anilist) implies the identity fields it guarantees.
- **Robust by construction.** Bazarr's hard-won anti-ban model: per-provider
  exception→cooldown throttling with a "5 strikes in 120 s" soft counter, layered
  HTTP (timeout → retry), and persistent throttle state.
- **A complete subtitle toolchain, not just a downloader.** Encoding
  normalization, format conversion (SRT/ASS/SSA/VTT), HI removal, archive
  extraction, **sync** (ffsubsync/alass), **translation** (LibreTranslate/local),
  and **transcription** fallback (Whisper) — all optional, all behind one
  orchestrator, all runtime-detected (never required).
- **Clean architecture.** Ports-and-adapters + SOLID, one crate per concern, so a
  new provider/translator/player is a small isolated change, never a core edit.

## The pipeline

```
            ┌──────────────┐  identify   ┌───────────────────────┐
  any app ─▶│  frontend     │────────────▶│  Identifier            │ filename parse
  (cli/mpv/ │ (ost/ostd/ffi)│             │  (os-identify)         │ + OSDB hash
   daemon)  └──────┬───────┘             └──────────┬────────────┘ + AniList refiner
                   │  Media + ids + hashes          │
                   ▼                                ▼
            ┌──────────────┐   list      ┌───────────────────────┐
            │   Engine      │────────────▶│  Provider(s)           │ OpenSubtitles.org
            │  (os-engine)  │             │  (os-providers)        │ (keyless), SubDL,
            └──────┬───────┘   score ◀────┘ candidates             │ OpenSubtitles.com,
                   │  best candidate(s)                            │ Jimaku — more planned
                   ▼
            ┌──────────────┐  normalize → convert → (sync) → (translate)
            │ Post-process  │◀───────────  encoding→UTF-8, ASS/VTT→SRT, HI removal,
            │ (os-process…) │              ffsubsync/alass, LibreTranslate, Whisper
            └──────┬───────┘
                   ▼
            ┌──────────────┐
            │  delivered    │  sidecar file written next to the video, returned
            │  subtitle     │  over FFI/HTTP, or loaded into the player
            └──────────────┘
```

See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for the full design,
[docs/RESEARCH.md](docs/RESEARCH.md) for the prior-art analysis this is built on,
[docs/STRATEGY.md](docs/STRATEGY.md) for the long-term direction (the backend
standard), and [docs/PROTOCOL.md](docs/PROTOCOL.md) for the integration contract.

## Features

- **Discover** the right subtitle by file hash, filename guess, or explicit
  ids/query — for movies, series, and anime.
- **Search providers in parallel**, score every candidate on one scale, and pick
  the best per language.
- **Download & normalize**: extract archives, detect encoding → UTF-8, convert
  format (incl. ASS/VTT → SRT), optionally strip hearing-impaired cues.
- **Sync** a subtitle to the actual audio (ffsubsync/alass adapters, runtime-detected).
- **Translate** into your language (LibreTranslate / local backend, timing-preserving).
- **Transcribe** from audio (Whisper) when no subtitle exists anywhere.
- **Automate** a whole library: Sonarr/Radarr webhooks, `ost scan <dir>` / `POST
  /scan` bulk fetch, and a **wanted list** that re-searches anything still missing
  on a timer until it's found (anime fansubs often lag a release).
- **Serve all of the above** to any app via CLI, HTTP/JSON, FFI, or the mpv plugin.

## Frontends (integration surfaces)

| Surface | Crate | For |
|---------|-------|-----|
| `ost` CLI | `os-cli` | humans + scripts; **every** subcommand supports `--json` |
| `ostd` daemon | `os-daemon` | any app, over local **HTTP/JSON** + an **OpenSubtitles.com-compatible** surface + Sonarr/Radarr webhooks |
| `libopensubtitle` | `os-ffi` | native apps embedding via **C ABI** (JSON in/out) |
| mpv plugin | `os-mpv` | a thin **Lua** client driving `ost --json` |
| Rust library | `os-engine` / `os-compose` | other Rust apps (e.g. **open-media**) |
| WASM module | `os-ffi` *(planned)* | browser/edge |

## Install

**Prebuilt binaries (no compiling)** — each [GitHub Release](https://github.com/grok-insider/open-subtitle/releases)
ships `ost`, `ostd`, `libopensubtitle` (+ `opensubtitle.h`), and the mpv plugin for
Linux (x86_64/aarch64, static musl), macOS (x86_64/arm64), and Windows (x86_64).
Download the archive for your platform, extract, and put `ost`/`ostd` on your
`PATH`.

**Nix / NixOS** — the flake builds `ost` + `ostd` + the library, with prebuilt
closures on the `grok-insider` cachix cache (so you don't compile):

```nix
# flake.nix inputs
inputs.open-subtitle.url = "github:grok-insider/open-subtitle";

# Home Manager
imports = [ inputs.open-subtitle.homeManagerModules.default ];
programs.open-subtitle = {
  enable = true;
  languages = [ "en" "es" ];
  mpv.enable = true;         # deploy the mpv plugin
  # daemon.enable = true;    # optional: run ostd as a user service
  # daemon.address = "127.0.0.1:4110";
};
```

Or ad hoc: `nix run github:grok-insider/open-subtitle -- get "Interstellar 2014" -l en`.

**From source** — `cargo build --release` (Rust ≥ 1.80); binaries land at
`target/release/{ost,ostd}`. TLS is rustls + ring, so there's no cmake/clang in the
build path.

## Usage

### `ost` (CLI)

```sh
ost init                                   # write the default config (keyless)
ost get "Interstellar 2014" -l en          # search by title, write a sidecar .srt
ost get video.mkv -l en,es --hi            # hash a local file, strip HI cues
ost search "frieren" -l en --kind anime --season 1 --episode 1
ost auto video.mkv -l en                   # identify → download → sync → transcribe-fallback
ost scan ~/Media -l en --dry-run           # report which videos are missing subs
ost scan ~/Media -l en                     # bulk-fetch the missing ones
ost sync subs.srt video.mkv                # align an existing sub (needs a sync backend)
ost translate subs.srt -t es               # translate (needs a translate backend)
ost providers                              # list wired providers + throttle state
ost config                                 # print the resolved config
```

Add `--json` to any command for the machine-readable sidecar contract (this is
exactly what the mpv plugin consumes). Global flags: `--config <path>`,
`-v/--verbose`.

### `ostd` (daemon)

```sh
ostd                                       # serve on 127.0.0.1:4110 (override with OSTD_ADDR)
```

- **Native:** `GET /health`, `/capabilities`, `/identify`, `/search`, `/get`
  (also under `/v1`). Errors use a typed envelope:
  `{ "error": { "kind", "message", "retry_after_secs?" } }`.
- **OpenSubtitles.com-compatible** (repoint existing clients at the local engine):
  `GET /osc/api/v1/subtitles`, `POST /osc/api/v1/download`, `GET /osc/file/<id>`.
- **Automation:** `POST /webhook` · `/webhook/sonarr` · `/webhook/radarr`;
  `POST /scan`; `GET`/`DELETE /wanted`; `POST /wanted/run`; `POST /wanted/clear`.

### `libopensubtitle` (C ABI)

JSON in, JSON out. `ost_version`, `ost_search`, `ost_get`, and `ost_free`:

```c
char *json = ost_search("Interstellar 2014", "en");
// ... parse json ...
ost_free(json);
```

### mpv plugin

**Alt+s** downloads and loads the best subtitle for the current file/stream;
**Alt+S** prompts for a manual query. See [crates/os-mpv/README.md](crates/os-mpv/README.md)
for install + `script-opts` configuration.

## Configuration

A single TOML file at `~/.config/open-subtitle/config.toml` (respects
`XDG_CONFIG_HOME`); `ost init` writes the default. **Secrets live only here** —
never in the binary, repo, or store. Keyless search works out of the box;
everything below is optional.

| Section / key | Default | Purpose |
|---------------|---------|---------|
| `languages` | `["en"]` | preferred languages, in priority order |
| `[providers.opensubtitles_org]` | enabled | the keyless default source |
| `[providers.subdl]` `api_key` | needs key | SubDL (key-optional) |
| `[providers.opensubtitles_com]` `api_key` / `username` / `password` | off | higher OpenSubtitles.com limits |
| `[providers.jimaku]` `api_key` | off | **anime** subs via Jimaku (free key) |
| `[process]` `to_utf8` / `format` / `remove_hi` / `keep_original_format` | utf8 on, `srt` | normalize & convert downloads |
| `[sync]` `backend` | `none` | `ffsubsync` \| `alass` \| `none` |
| `[translate]` `backend` / `endpoint` / `api_key` | `none` | `libretranslate` \| `local` \| `none` |
| `[transcribe]` `backend` / `model` | `none` | `whisper` \| `none` |
| `[net]` `max_concurrency` / `timeout_secs` / `user_agent` | 8 / 20s | HTTP fan-out |
| `[automation]` `enabled` / `track_wanted` / `recheck_interval_secs` / `max_attempts` / `path_map` / `output_dir` | enabled, wanted on, 6h | `ostd` webhook + wanted-list behavior |

> The config also exposes `podnapisi`, `gestdown`, `tvsubtitles`, and `animetosho`
> toggles, reserved for providers on the [roadmap](#roadmap); they have no adapter
> wired yet.

## Project layout

A Cargo workspace of 13 crates, one per concern (full table in
[AGENTS.md](AGENTS.md#module-layout)):

```
crates/
  os-core        domain model + ports (traits) + pure scoring/matching   — no I/O
  os-config      config schema + load/save + secrets policy
  os-identify    filename/release parsing + OSDB hashing + AniList refiner
  os-providers   adapters: opensubtitles_org, subdl, opensubtitles_com, jimaku
  os-process     encoding→UTF-8, format convert (incl. ASS/VTT→SRT), HI removal, archive extract
  os-sync        subtitle↔video sync (ffsubsync / alass adapters)
  os-translate   translation (LibreTranslate adapter; local-first)
  os-transcribe  speech-to-text fallback (Whisper CLI adapter)
  os-engine      use-cases + Engine + Throttler                          — depends only on os-core
  os-compose     shared composition root: build_engine(&Config) -> Engine
  os-daemon      the `ostd` HTTP/JSON server (+ OpenSubtitles-compatible + webhooks)
  os-cli         the `ost` binary (composition root)
  os-ffi         C-ABI libopensubtitle (cdylib/staticlib/rlib; + opensubtitle.h)
```

`crates/os-mpv/` is **not** a Cargo crate — it's the mpv Lua plugin asset
(`open-subtitle.lua`) that drives `ost --json`, bundled into the package and
release archives.

## Roadmap

Shipped: the keyless engine, all frontends, the full toolchain
(sync/translate/transcribe), and Sonarr/Radarr automation + wanted-list. Planned
(see [docs/ROADMAP.md](docs/ROADMAP.md)): more providers (Podnapisi,
Gestdown/Addic7ed, TVsubtitles, AnimeTosho, Kitsunekko, embedded), a native sync
path, daemon SSE, an interactive `ost` TUI, and the WASM build.

## License

[MIT](LICENSE) © 2026 Grok Insider.

This is a client for services you bring your own account to (where applicable)
and for public subtitle indexes. You are responsible for complying with the laws
of your jurisdiction and the terms of those services. Subtitle files carry the
rights of their authors/uploaders.
