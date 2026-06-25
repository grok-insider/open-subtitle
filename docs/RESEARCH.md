# RESEARCH — prior art, distilled

This document is the evidence base for open-subtitle's design. We read the source
of the leading open-source subtitle tools, extracted what each does best, and
recorded the concrete algorithms/values worth reusing. Everything below is
grounded in real code (paths/line refs are to the upstream repos as of analysis).

## Projects analyzed

| Project | Lang | Shape | What it's best at |
|---------|------|-------|-------------------|
| [subliminal](https://github.com/Diaoul/subliminal) | Python | library | provider abstraction, two-layer **scoring**, refiners, OSDB hash |
| [Bazarr](https://github.com/morpheus65535/bazarr) (`subliminal_patch`/`subzero`) | Python | service | ~60 **providers**, **anti-ban/throttling**, encoding + **mods** (HI/OCR) |
| [subg](https://github.com/kakeetopius/subg) | Go | CLI | clean `Provider` interface + **fallback chain**, format convert |
| [uosc](https://github.com/tomasklaen/uosc) (`ziggy`) | Lua+Go | mpv UI | the **sidecar-binary** contract (JSON stdout), in-player menu |
| [mpv-subversive](https://github.com/nairyosangha/mpv-subversive) | Lua | mpv plugin | **anime** (AniList+Jimaku), per-directory lookup caching |
| [subtool](https://github.com/maxgfr/subtool) | Bash | CLI | full **pipeline**: download→translate→sync→transcribe→embed, keyless |
| [ffsubsync](https://github.com/smacke/ffsubsync) | Python | CLI/lib | language-agnostic **subtitle↔video sync** (VAD + FFT correlation) |
| [Sublarr](https://github.com/Abrechen2/sublarr), [animeSubs_dl](https://github.com/TnTora/animeSubs_dl) | Py/Py | service/plugin | anime-first provider lists (Jimaku/Kitsunekko/AnimeTosho) |

## 1. Provider abstraction (from subliminal + subg)

**subliminal** models a provider as a context manager with capability classvars,
so a pool can filter providers *before* paying any network/instantiation cost:

- `languages: Set[Language]`, `video_types: (Episode, Movie)`,
  `required_hash: str | None`, `subtitle_class`.
- Classmethod pre-checks: `check(video)`, `check_types(video)`,
  `check_languages(langs)` (returns the intersection).
- Two methods do the work: `list_subtitles(video, languages)` (returns metadata
  only) and `download_subtitle(subtitle)` (lazily fetches `.content`).
- Registration via **stevedore** entry points → internal list → runtime
  `register()`. Third parties can ship providers with zero core edits.

**subg** distills the same idea into a tiny Go interface plus an explicit
**fallback chain** that is excellent for a "just get me a sub" mode:

```go
type Provider interface {
    Name() string
    SearchSubtitle() error
    DisplaySelections() ([]Subtitle, error)
    Download([]Subtitle) error
    DownloadBest() error
}
// ProviderSet.StartSearchAndDownload(): for each query, try each provider in
// order; on ErrNextProvider continue to the next; on success move to next query.
```

**Takeaway for us:** a `Provider` port with capability metadata + cheap
pre-checks, plus an `Engine` that supports both "search all, score, pick best"
and "ordered fallback until one yields."

## 2. Matching + scoring (from subliminal; refined by Bazarr)

This is the crown jewel and the main reason results feel "right." It's a
**two-layer** system: providers compute a *set of match-tag strings*; a separate,
swappable scorer maps that set → integer via a weights table.

### subliminal weights (vanilla)

Episodes (`score.py`): `hash 971`, `series 486`, `country 162`, `year 162`,
`episode 54`, `season 54`, `release_group 18`, `streaming_service 18`, `fps 9`,
`source 4`, `audio_codec 2`, `resolution 1`, `video_codec 1`.

Movies: `hash 323`, `title 162`, `country 54`, `year 54`, `release_group 18`,
`streaming_service 18`, `fps 9`, `source 4`, `audio_codec 2`, `resolution 1`,
`video_codec 1`.

The weights form a **quasi-positional number system**: each tier ≈ sum of all
lower tiers + 1, and **`hash` alone == the sum of everything else**, so a hash
match outranks any combination of weaker signals. `compute_score` short-circuits:
if `hash` matched, `matches &= {'hash'}`.

### Match equivalences (ID → implied fields)

An external-ID match implies the identity fields it guarantees, putting ID-based
and filename-based providers on one scale:

- episode `imdb_id` → `{series, year, country, season, episode}`
- episode `series_imdb_id` → `{series, year, country}` (same for tmdb/tvdb)
- episode `title` (episode title) → `+episode`
- movie `imdb_id`/`tmdb_id` → `{title, year, country}`

### GuessIt fusion

Providers re-run the **filename/release parser** on every release string the API
returns and union the resulting matches — turning messy release names
(`Show.S01E01.1080p.WEB-DL.x264-GRP`) into structured matches for free.

### Bazarr's refinements (production-hardened)

- Rescaled weights (episode `hash 359`, `series 180`, `year 90`,
  `season/episode 30`, `release_group 14`, …) with the invariant
  `hash == sum(others) − 1`.
- **Hash must corroborate**: a hash match is only trusted if structural matches
  are also present (episode needs `{series,season,episode,source}`; movie needs
  `{video_codec,source}`) — prevents hash collisions from winning.
- **Dual sort key** `(score, score_without_hash)` so ties break on real content.
- **Series safety gate**: before download, require
  `{season,episode} ⊆ matches and (series ∈ matches or imdb_id ∈ matches)` to kill
  cross-series false positives that pure scoring misses.

**Takeaway for us:** port this wholesale into `os-core` as a pure, unit-tested,
swappable scorer. It is the single most valuable algorithm in the ecosystem.

## 3. Identification & hashing (from subliminal refiners)

A `Video`/`Media` must carry a **rich identifier matrix** so each provider can
cherry-pick what it supports: `title/series`, `year`, `season`, `episodes[]`,
`release_group`, `source`, `resolution`, `video_codec`, `audio_codec`,
`frame_rate`, `imdb_id`, `tmdb_id`, `tvdb_id` (+ `series_*` variants),
`anidb_episode_id`, `anilist_id`, `mal_id`, and `hashes: {provider: hash}`.

### The OpenSubtitles (OSDB) file hash — exact algorithm

Used by OpenSubtitles.org/.com, BSPlayer, Subtis; reproduced identically in
uosc's ziggy (`lib.OSDBHashFile`). For a file ≥ 128 KiB:

```
hash  = filesize                       # seed with the size (u64)
hash += sum of first 64 KiB read as 8192 little-endian u64 chunks
hash += sum of last  64 KiB read as 8192 little-endian u64 chunks
hash &= 0xFFFFFFFFFFFFFFFF              # wrap at 64 bits after each add
return lowercase hex, 16 digits        # e.g. "09a2c497663259cb"
```

Other hashes seen: **NapiProjekt** = md5 of the first 10 MiB plus a digit-shuffle;
**Shooter** = its own scheme. We implement OSDB first (it's the most widely
useful), behind a `Hasher` trait so others slot in.

### Refiners (progressive enrichment)

subliminal enriches a `Video` before search via a refiner pipeline:
`metadata` (local mediainfo/ffmpeg — needs no key), then online id refiners
`tmdb` (user key), `tvdb` (ships a key), `omdb` (ships a key). We add an
**AniList/AniDB/MAL** refiner for anime (AniList GraphQL is keyless).

## 4. Anti-ban / throttling (from Bazarr — the production wisdom)

Bazarr's `subliminal_patch` is where years of "the site blocked us" lessons live:

- **Layered HTTP sessions**: `Timeout → Certifi → Retry(tries=3, delay=5) →
  Cloudflare(cloudscraper, cache cf_clearance) `, plus optional custom DNS and a
  captcha "pitcher." A retrying session is injected uniformly with per-provider
  opt-out (Podnapisi is excluded "so we don't hurt it more").
- **Throttle map**: a per-provider `exception → (cooldown, reason)` table.
  Defaults: `TooManyRequests→1h`, `DownloadLimitExceeded→3h`,
  `ServiceUnavailable→20m`, `APIThrottled→10m`, `Timeout→1h`,
  `Auth/Config→12h`; with per-provider overrides.
- **Two-tier counting**: "soft" exceptions don't throttle on the first hit — they
  `sleep(5)` and only throttle after **5 occurrences within 120 s**. "Hard"
  exceptions throttle immediately. Throttle state persists to disk and
  auto-restores when the cooldown passes.
- **Quota-aware resets**: providers with daily quotas reset at the site's local
  midnight (e.g. Prague/Lisbon), so a ban expires exactly when quota refreshes.

**Takeaway for us:** a single `Throttler` in `os-core`/`os-engine` with this
exact model. It's reusable across every provider and is what separates a toy from
something you can run all day.

## 5. Post-processing (from Bazarr `subzero` + subg)

- **Encoding → UTF-8**: try UTF-8, then BOM detection, then a **language-specific
  candidate table** (e.g. `zho→cp936/gb2312/gbk/big5/gb18030`,
  `jpn→shift-jis/cp932/euc_jp`, `ara/fas→windows-1256`, `heb→windows-1255`,
  Cyrillic vs Latin Serbian split), validating printability, then fall back to a
  `chardet`-equivalent. Normalize line endings to `\n`.
- **Format conversion**: SRT ⇆ ASS/SSA ⇆ VTT ⇆ MicroDVD/SUB (MicroDVD needs FPS).
  SRT is the safe default; an opt-in path preserves the original (`.ass` styling).
- **Mods pipeline** (ordered, language-gated, registry-based): `OCR_fixes`
  (per-language search/replace for `I↔l`, `F'` artifacts), `remove_HI` (strip
  `[...]`/`(...)`/`NAME:`/`♪` cues), `common` (dash/quote/ellipsis normalization,
  ad-line stripping), `color`, `offset`, `fps`, `reverse_rtl` (heb/ara/fas).
- **Archive extraction**: ZIP/RAR/7z/xz; when an archive holds many subs, **guess
  each member and score it** (season/episode/release-group), pick the best, not
  just the largest.

## 6. The frontend contract (from uosc/ziggy + mpv-subversive)

uosc proves the cleanest way to make a native engine drive a scripting host: a
**sidecar binary** with subcommands that takes flags and prints **JSON to
stdout**; the Lua side calls it via async subprocess and parses the JSON.

```
ziggy search-subtitles --api-key K --agent "app v1" --languages en,es \
                       --hash FILE --query "..." --page 1      # → JSON results
ziggy download-subtitles --file-id N --destination DIR         # → {file, remaining, …}
```

```lua
-- mpv side: mp.command_native_async{ name='subprocess',
--   args = {ziggy_path, 'search-subtitles', ...}, capture_stdout=true }
-- then parse_json(result.stdout); {error,message} signals failure.
```

**mpv-subversive** adds two ideas we want: drive identification from **AniList**
(GraphQL, keyless) and **cache the lookup per directory** (`.anilist.id`) so every
episode in a folder resolves instantly.

**Takeaway for us:** `ost` *is* the ziggy. The mpv plugin (and any other host)
shells out to `ost --json <subcommand>` or talks to `ostd` over HTTP. One
contract, many hosts.

## 7. The full toolchain (from subtool + ffsubsync)

subtool shows users want more than download — they want a one-shot **auto**:
`download → translate → sync → (transcribe fallback) → embed`. Concretely:

- **Keyless download**: OpenSubtitles.org REST (`rest.opensubtitles.org`,
  lowercase queries, season/episode as alphabetical path segments) + Podnapisi
  JSON. No key.
- **Translate**: default Google via `translate-shell` (keyless); optional LLM
  (Claude/OpenAI/etc.). We invert the default to **local-first** (LibreTranslate
  or the host's own MT stack) for privacy + no quota.
- **Sync**: **ffsubsync** aligns subtitle activity to **VAD-detected speech** via
  FFT cross-correlation — language-agnostic, no transcription needed. `alass` is
  the alternative (handles variable offsets/ad-breaks). We wrap both.
- **Transcribe fallback**: **Whisper** (local) when nothing is found online.
- **Convert/merge/clean/embed**: format conversion, dual-language merge, mojibake
  fix (ftfy-equivalent), and muxing via ffmpeg.

## 8. Provider catalog & key requirements

The crucial point for our "no key, no cap" goal — which sources need what:

### Keyless (no account, default ON)
| Provider | Type | Identifiers used |
|----------|------|------------------|
| **OpenSubtitles.org** REST | movie+TV | query (lowercase), imdb, season/episode, hash |
| **Podnapisi** | movie+TV | title, season/episode, hash |
| **Gestdown** (Addic7ed proxy) | TV | **tvdb_id** + season/episode/lang |
| **TVsubtitles** | TV | series/season/episode |
| **AnimeTosho** | anime | **anidb_episode_id** (xz attachments) |
| **Kitsunekko** | anime (JP) | title scrape |
| **Subtitulamos / Subtis** | TV(es) / movie(es) | scrape |

### Generous anonymous (default ON)
| **SubDL** | movie+TV+anime | imdb/tmdb/film_name; **~300 downloads/day per IP** anon; free key = 2000 req/day |

### Key/login optional (OFF unless configured)
| **OpenSubtitles.com** REST | movie+TV | needs an API key (a shared one exists) + login for higher limits (≈20/day free) |
| **Jimaku** | **anime** (best) | free API key; AniList-id matched; the gold standard for anime |
| **Addic7ed** | TV | anonymous works; login lifts limits |

### Local / offline (no network)
- **Embedded extraction** (ffmpeg) — pull subs already muxed in the file.
- **Local archive/folder** + **offline AniList→dir mapping** (mpv-subversive
  style) for a personal subtitle library.

## 9. What each project taught us (one line each)

- **subliminal** → the two-layer scorer, refiner pipeline, OSDB hash, capability
  pre-checks, entry-point provider discovery.
- **Bazarr** → throttle map + 5-strikes-in-120s, layered/Cloudflare HTTP,
  language-aware encoding, the mods engine, the hash-must-corroborate gate, the
  60-provider catalog.
- **subg** → a minimal `Provider` interface and an explicit fallback chain; a
  clean format enum.
- **uosc/ziggy** → the sidecar JSON contract that lets one binary drive any host.
- **mpv-subversive** → keyless AniList identification + per-directory lookup
  caching; the anime mindset.
- **subtool** → the one-shot `auto` pipeline and the keyless OpenSubtitles.org +
  local-translate + ffsubsync + Whisper-fallback chain.
- **ffsubsync** → language-agnostic sync via VAD + FFT correlation (no OCR/STT).

## 10. Anti-goals (explicitly not copying)

- **No hardcoded single provider** (uosc's mistake) — providers are adapters.
- **No mandatory key/cap** for default operation (uosc's 5/day) — keys are
  upgrades.
- **No runtime interpreter dependency** (subliminal/Bazarr/subtool need
  Python/bash) — ship one static binary; optional external tools (ffmpeg,
  ffsubsync, whisper) are detected, not required.
- **No bundling secret keys into the binary/repo** — keys live only in user config.
- **No captcha-farm / private-tracker scraping in core** — those Bazarr providers
  are out of scope for v1 (can be optional plugins later).
