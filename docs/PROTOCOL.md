# PROTOCOL — the open-subtitle contract (descriptive, pre-`v1`)

This is the integration contract other software builds on. It is **descriptive,
not yet frozen** — per `STRATEGY.md` decision 4, `v1` is frozen with semver
guarantees at **v0.5**. Until then the shapes here track the engine and may
change; breaking changes are noted in `CHANGELOG.md`.

The same shapes are served over every surface:

| Surface | Crate / binary | Status |
|---------|----------------|--------|
| CLI sidecar (`ost … --json`) | `os-cli` | ✅ implemented |
| HTTP/JSON daemon (`ostd`) | `os-daemon` | ✅ implemented (subset) |
| C-ABI (`libopensubtitle`) | `os-ffi` | ✅ implemented (subset) |
| WASM | `os-ffi` (wasm target) | ⏳ planned |
| OpenSubtitles-compatible REST | `os-daemon` | ⏳ planned (the wedge) |

Design rules: **JSON in, JSON out**; identical shapes across surfaces; additive
evolution before `v1`; capability flags so clients negotiate rather than assume.

---

## 1. Core types

These are the serialized engine types (`os-core::model`). Field names are stable
intentions; optional fields may be absent/null.

### Language
```json
{ "alpha3": "eng", "region": "BR", "hearing_impaired": false, "forced": false }
```
- `alpha3` — ISO 639-3 / 639-2T (e.g. `eng`, `spa`, `jpn`). `region` optional
  (e.g. `BR`). Clients also accept `en`, `pt-BR`, `pob`, with `:hi` / `:forced`
  suffixes on input.

### Media
```json
{
  "kind": "anime",
  "ids": { "imdb": null, "tmdb": null, "tvdb": null,
           "series_imdb": null, "series_tmdb": null, "series_tvdb": null,
           "anilist": 130298, "anidb_episode": null, "mal": 48316 },
  "title": "The Eminence in Shadow",
  "original_title": null,
  "alternative_titles": ["Kage no Jitsuryokusha ni Naritakute!"],
  "year": 2022,
  "season": 1,
  "episodes": [1],
  "episode_title": null,
  "release": { "release_group": null, "source": null, "resolution": null,
               "video_codec": null, "audio_codec": null,
               "streaming_service": null, "edition": null },
  "hashes": { "osdb": "d652b5740d034cff" },
  "size": 1001400000,
  "name": "…the original filename/release string…"
}
```
- `kind` ∈ `movie | series | anime`. `episodes` is a list (multi-episode files).

### SubtitleCandidate (a search result; metadata only)
```json
{
  "provider": "opensubtitles_org",
  "id": "1954659816",
  "language": { "alpha3": "eng", "region": null, "hearing_impaired": false, "forced": false },
  "release": "Interstellar.2014.720p.BluRay.x264-DAA",
  "hi": false,
  "forced": false,
  "format": "srt",
  "download_url": "https://…/1954659816.gz",
  "matched_by_hash": false,
  "hints": { "imdb": "816692", "downloads": "2278022" },
  "score": 216,
  "score_without_hash": 216
}
```
- `score` is the engine's match score; `score_without_hash` is the tie-breaker.
  See `ARCHITECTURE.md` §5 / `RESEARCH.md` §2 for the weights.

### SubtitleFile (a delivered subtitle)
```json
{
  "language": { "alpha3": "eng", "region": null, "hearing_impaired": false, "forced": false },
  "format": "srt",
  "text": "1\n00:00:06,000 --> 00:00:12,074\n…",
  "provider": "opensubtitles_org",
  "release": "…",
  "hi": false,
  "forced": false
}
```
- `text` is UTF-8; `format` is the delivered format (post-conversion).

---

## 2. Operations

Three core operations, mirrored on every surface.

### identify
Resolve an input (file path / release name / title) into a `Media`.
Input params: `input` (required), `season?`, `episode?`, `kind?`.
Output: a `Media` object.

### search
List scored candidates for a `Media` in the requested languages.
Input params: `input` (or a `Media`), `langs?` (comma list, defaults to config).
Output: an array of `SubtitleCandidate`, sorted best-first.

### get
Download + post-process the best subtitle(s), one per requested language.
Input params: `input`, `langs?`.
Output: an array of `SubtitleFile` (inline `text`).

> Planned for `v1`: `auto` (identify → get → sync → transcribe fallback), `sync`,
> `translate`, plus a **progress event stream** (SSE/callback) for long ops, and a
> `capabilities` call (providers available, languages, features, version).

---

## 3. HTTP/JSON daemon (`ostd`) — current

Binds `127.0.0.1:4110` by default (`OSTD_ADDR` to override). Localhost only.

| Method | Path | Query | Returns |
|--------|------|-------|---------|
| GET | `/health` | — | `{ "ok": true, "name": "ostd" }` |
| GET | `/identify` | `input,[season,episode,kind]` | `Media` |
| GET | `/search` | `input,[langs]` | `SubtitleCandidate[]` |
| GET | `/get` | `input,[langs]` | `SubtitleFile[]` |

Errors return `{ "error": "<message>" }` with HTTP 200 today; `v1` will use proper
status codes + the typed error model below.

Planned `v1` additions: `/v1` prefix, `GET /capabilities`, `POST /get` with a
`Media` body, `GET /events` (SSE progress), and auth (a bearer token) for non-
loopback binds.

---

## 4. C-ABI (`libopensubtitle`) — current

```c
const char *ost_version(void);
char       *ost_search(const char *input, const char *langs);  // JSON SubtitleCandidate[]
char       *ost_get   (const char *input, const char *langs);  // JSON SubtitleFile[]
void        ost_free  (char *ptr);                              // release returned strings
```
JSON in/out keeps the ABI tiny and stable. A WASM build will expose the same
functions via `wasm-bindgen`.

---

## 5. OpenSubtitles-compatible surface (planned — the wedge)

To unlock the large set of apps that already integrate **OpenSubtitles.com**, the
daemon will expose a compatible REST surface so those clients can be pointed at
`ostd` and transparently use our engine. Target shape (subset of the upstream
API):

```
GET  /osc/api/v1/subtitles?query=&languages=&season_number=&episode_number=&imdb_id=&moviehash=
        → { "data": [ { "id", "type": "subtitle",
                        "attributes": { "language", "release", "files": [ { "file_id", "file_name" } ],
                                        "hearing_impaired", "foreign_parts_only", … } } ] }
POST /osc/api/v1/download   { "file_id": <int> }
        → { "link": "http://127.0.0.1:4110/osc/file/<id>", "file_name": "…" }
GET  /osc/file/<id>         → the subtitle bytes
```

Mapping: our `SubtitleCandidate` → an OSc `data[]` entry (its `id`/`file_id` is our
candidate id); `download` resolves to a local URL the daemon serves from
`Provider::fetch` + post-processing. Languages accept ISO 639-1/2. This is a
**compatibility shim**, not the native protocol — clients keep their own UI; we
become the backend.

Status / quota fields (`remaining`, `reset_time`) are reported as effectively
unlimited for keyless providers, and as the real provider quota when a keyed
provider served the result.

---

## 5b. Automation webhooks (Sonarr/Radarr)

`ostd` accepts Sonarr/Radarr **"On Import"** webhooks and fetches subtitles for the
imported file automatically — the flagship automation use case.

| Method | Path | Body | Action |
|--------|------|------|--------|
| POST | `/webhook` | Sonarr or Radarr payload (auto-detected) | fetch on import |
| POST | `/webhook/sonarr` | Sonarr payload | fetch on import |
| POST | `/webhook/radarr` | Radarr payload | fetch on import |

Behavior: acts on `eventType ∈ {Download, Import}`; `Test` is acknowledged;
everything else is ignored. The handler builds a `Media` from the payload
(title, year, ids, season/episode, anime when `series.type == "anime"`), hashes
the file when reachable, downloads the best subtitle per configured language, and
writes sidecars **next to the media file** (or to `automation.output_dir` when the
path isn't reachable). Config (`[automation]`): `enabled`, `languages`,
`path_map` (prefix remap for containers), `output_dir`.

Response:
```json
{ "ok": true, "action": "import", "source": "radarr",
  "media": "Interstellar (2014)", "count": 1,
  "downloaded": [ { "language": "en", "provider": "opensubtitles_org", "path": "…/Movie.en.srt" } ] }
```

## 6. Error model (planned `v1`)

A typed envelope so clients and the throttler react correctly:

```json
{ "error": { "kind": "rate_limited", "message": "…", "retry_after_secs": 3600 } }
```

`kind` ∈ `config | network | rate_limited | download_limit | auth_required |
not_found | throttled | parse | unsupported | io | provider`. These mirror
`os_core::CoreError`. HTTP surfaces map them to status codes (404/401/429/…).

---

## 7. Versioning & conformance

- **Pre-`v1` (now → v0.5):** additive changes preferred; any breaking change is
  noted in `CHANGELOG.md`. No semver guarantee on the protocol yet.
- **`v1` (frozen at v0.5):** the `/v1` HTTP shapes, the C-ABI symbols, and the
  core JSON types become semver-stable. New capabilities are added behind
  `capabilities` flags, never by breaking existing fields.
- **Conformance suite:** a language-agnostic test set (fixtures + a runner) that
  any implementation/surface must pass — published alongside the spec so third
  parties can validate their clients and we can validate the daemon/FFI/WASM
  surfaces against one source of truth.

JSON Schemas for `Media`, `SubtitleCandidate`, `SubtitleFile`, and the error
envelope will live under `docs/schemas/` once drafted.
