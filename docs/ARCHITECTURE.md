# ARCHITECTURE

open-subtitle is a **Rust workspace** built as **ports & adapters (hexagonal) +
SOLID**. Capabilities are trait *ports* in `os-core`; concrete *adapters*
implement them; the application layer (`os-engine`) depends only on ports; each
binary/frontend is a composition root that wires the adapters it needs.

This is the same discipline as its sibling project **open-media**, so the two can
share mental model and even link directly (`open-media` can depend on `os-engine`
as a library).

## 1. Design principles

1. **One core, many frontends.** All logic lives behind ports in `os-core` and is
   orchestrated by `os-engine`. CLI, daemon, FFI, WASM, and the mpv plugin are
   thin composition roots — no business logic in a frontend.
2. **Adapters are leaves.** A provider/translator/sync/transcribe backend is an
   adapter that maps its concrete errors to `CoreError` at the boundary. Adding
   one is an isolated change (OCP), never a core edit.
3. **The dependency rule** (do not break):
   ```
   os-cli/os-daemon/os-ffi/os-mpv ─▶ os-engine ─▶ os-core ◀── every adapter crate
            │                                                      ▲
            └───────────────────── wires ──────────────────────────┘
   ```
   `os-core` depends on nothing internal. `os-engine` depends on **only**
   `os-core` (never an adapter). Only the frontends may name concrete adapters.
4. **No I/O in core.** `os-core` is pure: model, ports, scoring, matching, error
   types. It compiles to WASM trivially and is the most-tested crate.
5. **Keyless-by-default, secrets-in-config-only.** Default providers need no key;
   keys/logins are optional config. Nothing secret is ever compiled in.
6. **Degrade, don't fail.** A provider outage, a missing optional tool (ffmpeg,
   ffsubsync, whisper), or a throttled source disables a feature — it never aborts
   the run.

## 2. Crate layout (one crate per concern)

| Crate | Owns | Implements |
|-------|------|------------|
| `os-core` | Domain model (`Media`, `Episode`, `Movie`, `SubtitleCandidate`, `Language`, `IdSet`, `Hashes`), the **port traits**, `CoreError`, and the pure **scoring/matching**. **No I/O, no heavy deps.** | — (defines the ports) |
| `os-config` | Config schema, load/save, XDG paths, secrets policy, provider toggles. | — |
| `os-identify` | Filename/release parsing (movie/series/anime), hashing (OSDB + others), refiners (AniList/AniDB/MAL, TMDB/TVDB/IMDb, local metadata). | `Identifier`, `Hasher`, `Refiner` |
| `os-providers` | Subtitle source adapters. One module per source. | `Provider` |
| `os-process` | Encoding→UTF-8, format conversion, mods (HI/OCR/common/color), archive extraction. | `PostProcessor` |
| `os-sync` | Subtitle↔video/subtitle↔subtitle alignment. | `Synchronizer` |
| `os-translate` | Translation backends (local-first, LLM optional). | `Translator` |
| `os-transcribe` | Speech-to-text fallback. | `Transcriber` |
| `os-engine` | Application use-cases + the `Engine` that composes ports, the `Throttler`, the parallel search/score/best orchestration. **Depends only on `os-core`.** | — (consumes ports) |
| `os-cli` | The `ost` binary: arg parsing, composition root, `--json` mode, interactive TUI. | — (wires adapters) |
| `os-daemon` | The `ostd` binary: HTTP/JSON (+ SSE) server over the engine. | — (wires adapters) |
| `os-ffi` | C-ABI (`libopensubtitle`) + WASM bindings. | — (wires adapters) |
| `os-mpv` | mpv sidecar contract (uses `ost --json`) + a thin Lua plugin. | — |

### The dependency rule, concretely

- `os-core` → nothing internal.
- `os-engine` → **only** `os-core`. It must never `use` an adapter crate. If you
  want to, you need a **new port** in `os-core` instead.
- Adapter crates → `os-core` (+ their own I/O deps). They never depend on each
  other or on `os-engine`.
- Frontends (`os-cli`, `os-daemon`, `os-ffi`, `os-mpv`) → the **only** crates
  allowed to name concrete adapters; they assemble the `Engine` in their
  `compose.rs`.

## 3. Domain model (`os-core`)

```text
Media            kind: Movie | Series | Anime
                 ids: IdSet, title, original_title, year, …
Episode/Movie    season, episodes[], episode_title, release fields…
IdSet            imdb, tmdb, tvdb, series_*; anilist, anidb_episode, mal
Hashes           map<ProviderHashName, String>   (osdb, napiprojekt, …)
ReleaseInfo      release_group, source, resolution, video_codec,
                 audio_codec, frame_rate, streaming_service
Language         IETF (BCP-47) + ISO 639-1/2/3 bridging, region, script,
                 hearing_impaired / forced flags
SubtitleCandidate provider, id, language, release, hi, forced, url/handle,
                 download lazily → bytes; format; score (filled by engine)
SubtitleFile     bytes + encoding + format (the materialized result)
```

The model is deliberately a **superset** of what any one provider needs (the
identifier matrix from RESEARCH §3). Providers read the fields they support.

## 4. Ports (the trait surface)

Small, focused, object-safe `async` traits in `os-core::ports`:

```rust
trait Provider {                      // a subtitle source
    fn name(&self) -> &str;
    fn capabilities(&self) -> Capabilities;          // media types, langs, needs_hash, needs_id
    async fn list(&self, q: &Query) -> CoreResult<Vec<SubtitleCandidate>>;
    async fn fetch(&self, c: &SubtitleCandidate) -> CoreResult<RawSubtitle>; // bytes (maybe archived)
}

trait Identifier {  async fn identify(&self, input: &MediaInput) -> CoreResult<Media>; }
trait Hasher {      fn name(&self) -> &str; fn hash_file(&self, path: &Path) -> CoreResult<Option<String>>; }
trait Refiner {     async fn refine(&self, media: &mut Media) -> CoreResult<()>; }
trait PostProcessor { fn process(&self, sub: RawSubtitle, opts: &ProcessOpts) -> CoreResult<SubtitleFile>; }
trait Synchronizer {  async fn sync(&self, sub: &SubtitleFile, ref_: &SyncRef) -> CoreResult<SubtitleFile>; }
trait Translator {    async fn translate(&self, sub: &SubtitleFile, to: &Language) -> CoreResult<SubtitleFile>; }
trait Transcriber {   async fn transcribe(&self, audio: &MediaRef, lang: Option<&Language>) -> CoreResult<SubtitleFile>; }
trait Scorer {        fn score(&self, c: &SubtitleCandidate, media: &Media) -> i32; } // pure, swappable
trait CacheStore {    /* get/put for refiner + provider listings */ }
```

A `Capabilities` struct lets the engine pre-filter providers cheaply (subliminal's
classmethod checks): which `MediaKind`s, which languages, whether a hash or a
specific id is required.

## 5. Scoring & matching (`os-core`, pure)

A direct port of the two-layer model (RESEARCH §2), implemented as pure functions
so it is exhaustively unit-tested with no network:

- `guess_matches(media, guess) -> MatchSet` — compares structured fields and
  GuessIt-parsed release strings, producing tags (`hash`, `series`, `season`,
  `episode`, `title`, `year`, `release_group`, `source`, `resolution`, …).
- `expand_equivalences(MatchSet) -> MatchSet` — ID → implied identity fields.
- `Scorer::score` — default `WeightedScorer` maps tags → int via a weights table
  with the invariant `hash == sum(others) − 1`; `hash` short-circuits; returns
  `(score, score_without_hash)` for tie-breaking.
- `series_safety_gate` — refuse episode subs lacking
  `{season,episode} ⊆ matches and (series|imdb_id)`.

The `Scorer` is a port: callers can swap weights (e.g. an anime profile that
boosts release-group/`.ass`) without touching providers.

## 6. The Engine (`os-engine`)

`Engine` holds `Arc<dyn Port>` collections and implements the use-cases. Built by
`EngineBuilder`; unset optional ports simply disable their features (e.g. no
`Translator` → no translation).

Use-cases:

- `identify(input) -> Media` — parse + hash + run refiners (parallel, best-effort).
- `search(media, langs) -> Vec<Scored>` — fan out `Provider::list` across enabled,
  non-throttled providers (bounded concurrency), score every candidate, sort.
- `download_best(media, langs, opts) -> Vec<SubtitleFile>` — pick best per
  language above `min_score`, `fetch`, post-process (encoding→format→mods),
  optional sync/translate; fall back to next candidate on failure.
- `fallback(media, langs) -> SubtitleFile` — subg-style ordered "first that
  yields," for a fast path.
- `auto(input, langs, opts)` — the subtool-style one-shot: identify → search →
  best → (sync) → (translate) → if nothing found and enabled, **transcribe**.

Cross-cutting:

- **`Throttler`** (Bazarr model): per-provider `exception → cooldown` map + soft
  "5 strikes in 120 s" counter + persistent state; the engine consults it before
  calling a provider and reports failures to it.
- **Parallelism**: `futures` bounded fan-out for search and refiners.
- **Cache**: refiner + listing results via the `CacheStore` port (memory/file).

## 7. Adapters

### Providers (`os-providers`) — v1 set

Keyless (default on): `opensubtitles_org`, `podnapisi`, `gestdown`, `tvsubtitles`,
`animetosho`, `kitsunekko`. Generous-anon: `subdl`. Key/login optional:
`opensubtitles_com`, `jimaku`, `addic7ed`. Local: `embedded` (ffmpeg),
`local_folder`/`offline_map`.

Each adapter:
- declares `Capabilities`,
- maps API/HTML → `SubtitleCandidate` (filling whatever match-relevant fields it
  knows: ids, release strings, hi/forced),
- uses the shared HTTP client (timeout → retry → optional Cloudflare) and the
  shared UA pool,
- returns `RawSubtitle` from `fetch` (engine handles archive extraction).

### Identify (`os-identify`)

- `FilenameIdentifier` — release/anime title + season/episode parsing (a
  guessit/anitomy-equivalent).
- `OsdbHasher` (+ trait for others).
- Refiners: `AniListRefiner` (keyless GraphQL, anime ids + offsets),
  `TmdbRefiner` (user key), `TvdbRefiner`/`OmdbRefiner` (bundled-key-free or own
  key), `LocalMetadataRefiner` (ffprobe/mediainfo, no network).

### Process / Sync / Translate / Transcribe

- `os-process`: `EncodingNormalizer` (UTF-8), `FormatConverter` (pysubs2-equivalent
  Rust crate), `ModsPipeline` (HI/OCR/common/color), `ArchiveExtractor`
  (zip/rar/7z/xz with member scoring).
- `os-sync`: `FfsubsyncAdapter`, `AlassAdapter` (wrap external tools), later a
  native VAD+correlation `Synchronizer`.
- `os-translate`: `LibreTranslateAdapter`, `LocalMtAdapter` (host stack, e.g. the
  NixOS speech-stack), `LlmAdapter` (optional).
- `os-transcribe`: `WhisperAdapter` (local/CLI), `OpenAiWhisperAdapter` (optional).

External tools are **detected at runtime** and their absence only disables that
feature.

## 8. Frontends (composition roots)

### `ost` CLI (`os-cli`)
Human + script use, **and the sidecar** for the mpv plugin. Every subcommand has a
`--json` mode that prints a single JSON document to stdout (the ziggy contract).

```
ost identify <file>                       # → Media JSON
ost search   <file|--query …> -l en,es    # → scored candidates JSON
ost get      <file> -l en --best          # download+process, print result JSON
ost auto     <file> -l es                 # download→sync→translate→(transcribe)
ost sync|translate|convert|extract …      # toolchain subcommands
ost providers                             # list providers + throttle state
ost                                       # interactive TUI (search→pick→load)
```

### `ostd` daemon (`os-daemon`)
A localhost HTTP/JSON server so **any app** (Jellyfin/Kodi plugin, a GUI, another
service) integrates without FFI. REST endpoints mirror the CLI; SSE streams
progress for long ops (sync/transcribe). Binds `127.0.0.1`, `GET /health`.

### `libopensubtitle` (`os-ffi`)
A C ABI (`cbindgen`) + WASM (`wasm-bindgen`) wrapping the engine for native/web
embedding. JSON in/out across the boundary to keep the ABI tiny and stable.

### mpv plugin (`os-mpv`)
A thin Lua script (the **only** non-Rust frontend) that drives `ost --json` via
`mp.command_native_async` (exactly uosc's `call_ziggy_async` pattern), renders an
in-player menu, and loads the result with `sub-add`. Falls back to `ostd` if a
daemon is running. Includes mpv-subversive's per-directory lookup caching.

## 9. Configuration (`os-config`)

Single TOML at `~/.config/open-subtitle/config.toml` (XDG-aware). Secrets only
here. Shape:

```toml
languages = ["en", "es"]              # preference order
[providers]                           # toggles + per-provider options
opensubtitles_org = { enabled = true }
subdl             = { enabled = true, api_key = "" }
opensubtitles_com = { enabled = false, api_key = "", username = "", password = "" }
jimaku            = { enabled = false, api_key = "" }   # anime
[process]  to_utf8 = true; format = "srt"; remove_hi = false; keep_original_format = false
[sync]      backend = "ffsubsync"     # ffsubsync | alass | none
[translate] backend = "local"         # local | libretranslate | llm | none
[transcribe] backend = "none"         # whisper | none
[net]       max_concurrency = 8; throttle_state = "~/.cache/open-subtitle/throttle.json"
```

## 10. Error model

Ports return `CoreResult<T>` = `Result<T, CoreError>`. Adapters convert their
concrete errors explicitly (no blind `?` from `reqwest`), choosing the right
variant so the `Throttler` can react:

```
CoreError::{ Config, Network, RateLimited{retry_after}, DownloadLimit{reset},
             AuthRequired, NotFound, Throttled, Parse, Unsupported, Io, Provider(String) }
```

`RateLimited`/`DownloadLimit`/`AuthRequired` drive the throttle map; `NotFound`
is a soft miss (try the next provider).

## 11. Testing strategy

- **Pure logic** (scoring, matching, hashing, filename parsing, encoding, config)
  — exhaustive unit tests, no network. The OSDB hash has a known-vector test.
- **Adapters** — tested against **recorded fixtures / in-process mock HTTP**
  (wiremock-equivalent), never the live service in CI.
- **Engine** — tested with **fake ports** (a `MockProvider`, etc.), covering
  scoring/best-selection, throttling, and fallback.
- **End-to-end** — gated `#[ignore]` + env tests hit real providers; the mpv
  plugin is verified with the `wisp` driver (drive `ost`/mpv, assert the loaded
  sub), mirroring how open-media is tested.

## 12. Packaging

A Nix flake + Home Manager module (this is built on NixOS): the `ost`/`ostd`
binaries, the mpv plugin deployed into `~/.config/mpv/scripts/`, and optional
runtime tools (`ffmpeg`, `ffsubsync`, `alass`, `whisper`) wired as detected
dependencies. Cross-compiled release artifacts for Linux/macOS/Windows.
