# continue-plan — what's left to do

A living checklist of outstanding work, so we can pick up later without
re-deriving context. Grouped by theme; roughly priority-ordered within each
section. The strategic *why* lives in [`docs/STRATEGY.md`](docs/STRATEGY.md); the
milestones in [`docs/ROADMAP.md`](docs/ROADMAP.md); the contract in
[`docs/PROTOCOL.md`](docs/PROTOCOL.md). This file is the actionable backlog.

> Convention: `[ ]` todo · `[~]` partially done · `[x]` done (kept briefly for
> context, then pruned). `master` is protected — land changes via PR.

---

## 0. Immediate ops follow-ups (release/CI)

- [ ] **Add `RELEASE_PLZ_TOKEN` secret** (fine-grained PAT or GitHub App) so the
      release PR auto-runs CI under the required-check rule. Scopes: repo
      open-subtitle, **Contents: read/write**, **Pull requests: read/write**.
      Until then, click **"Approve and run"** on each release PR. `release.yml`
      already reads the secret with a `GITHUB_TOKEN` fallback.
- [ ] **glibc dynamic lib leg:** the musl release archive ships only
      `libopensubtitle.a` (no `.so`). Add an `x86_64-unknown-linux-gnu` matrix
      leg (or a dedicated lib job) so a dynamic `.so` ships for glibc Linux too.
      (macOS/Windows archives already include `.dylib`/`.dll`.)
- [ ] **open-media parity (optional):** apply the same branch protection +
      `release.yml` token fallback to `grok-insider/open-media` (it's also unprotected).
- [ ] **cargo-dist installers (later):** layer `cargo-dist` on top for one-line
      `curl|sh` / PowerShell installers + a Homebrew tap, keeping release-plz for
      versioning. Note: cargo-dist doesn't package the cdylib/staticlib — keep the
      custom FFI artifact step.
- [ ] **Workflows housekeeping:** bump actions when GitHub forces Node24
      (checkout/cachix-action warnings are cosmetic for now).
- [ ] Consider requiring **"branches up to date before merge"** and/or a PR-only
      `nix build` (no cachix push) check — stricter, slower PRs. Deferred.

## 1. Protocol / contract — finish `v0.3` (docs/PROTOCOL.md)

- [ ] **OpenSubtitles-compatible hardening** so real OSc clients work unchanged:
      `POST /osc/api/v1/login` (accept any creds → fake token), tolerate
      `Api-Key`/`Authorization` headers, `GET /osc/api/v1/infos/languages`,
      `GET /osc/api/v1/infos/user` (high quota), `moviehash` search param,
      `page` passthrough. (`crates/os-daemon/src/osc.rs` + `main.rs`.)
- [ ] **`POST /get`** with a `Media` JSON body (not just query params).
- [ ] **`GET /events` (SSE)** progress stream for long ops (sync/transcribe).
- [ ] **`/v1` versioning**: keep the `/v1` alias, document stability, and prep the
      semver freeze at v0.5.
- [ ] **JSON Schemas**: validate live responses against `docs/schemas/` in a test;
      add a schema for `/capabilities` and the OSc shapes.
- [ ] **Conformance suite**: a language-agnostic fixture set + runner that the
      daemon/FFI/(WASM) surfaces must pass (prereq for the v1 freeze).

## 2. Automation — finish `v0.4` (the flagship)

- [ ] **Wanted list + scheduled re-search:** when an import has no sub yet (anime
      often lags), record it and re-search on a timer until found. Needs small
      persistent state (sqlite or a json file) + a scheduler in `ostd`.
- [ ] **Library scan:** a `POST /scan` (or CLI `ost scan <dir>`) that walks a
      media tree and fetches missing subs (Bazarr's bulk workflow).
- [ ] **Auth for non-loopback:** optional bearer token when `ostd` binds beyond
      `127.0.0.1` (today localhost-only). Document.
- [ ] Webhook hardening: dedupe repeated imports; honor `isUpgrade`; optional
      per-language "only if missing" check before downloading.

## 3. Providers & matching — breadth (`v0.2`/`v0.4`/`v0.6`)

- [ ] **TVDB id refiner** (in `os-identify`) — unlocks Gestdown (keyless TV) and
      improves episode scoring. (AniList refiner already exists for anime.)
- [ ] **Gestdown** provider (keyless Addic7ed proxy; needs `series_tvdb`).
- [ ] **TVsubtitles**, **Addic7ed** (login-optional) providers.
- [ ] **AnimeTosho** (keyless; needs AniDB episode-id mapping) + **Kitsunekko**
      (JP scrape) for anime breadth.
- [ ] **Podnapisi** (keyless) — implement against its current endpoint (was
      deferred as flaky; verify live first).
- [ ] **Embedded extraction** provider (ffmpeg) — pull subs already muxed in.
- [ ] **Local folder / offline map** provider (personal subtitle library;
      AniList-id → directory, mpv-subversive style).
- [ ] **Archive-member scoring:** when an archive holds many subs, guess + score
      each member (season/episode/group), pick the best (today: best-effort).
- [ ] **Anime scoring profile** (boost release-group / `.ass`); wire as a
      swappable `Scorer` in the engine.

## 4. Processing — match Bazarr/subzero quality (`v0.3`/`v0.6`)

- [ ] **Mods pipeline** beyond HI removal: `OCR_fixes` (per-language SnR),
      `common` (dash/quote/ellipsis normalization, ad-line strip), `color`,
      `reverse_rtl`. Registry-based, ordered, language-gated. (`os-process`.)
- [ ] **RAR / 7z / xz** archive extraction (today: zip + gzip only).
- [ ] **ASS styling preservation** path (keep-original-format), MicroDVD FPS
      handling, mixed-format edge cases.
- [ ] Language-aware encoding tables (extend `os-process::encoding` beyond
      BOM + chardetng for CJK/RTL specifics).

## 5. Frontends & integrations (`v0.5`/`v0.6`)

- [ ] **`ost plugin install <app>`** self-installer: embed assets via
      `include_str!`, write to the per-OS location, set `ost_path`; plus
      `plugin list/uninstall/doctor`. The killer easy-install path.
- [ ] **Repo reorg:** move `crates/os-mpv/` → `integrations/mpv/` and group future
      app assets under `integrations/`; update `flake.nix` postInstall + release
      packaging paths. Update AGENTS/ARCHITECTURE references.
- [ ] **WASM build** of `libopensubtitle` (`os-ffi`, `wasm-bindgen`) + a tiny demo.
- [ ] **VLC** (Lua extension) and **IINA** (JS `.iinaplugin`) reference clients.
- [ ] **Daemon-based addons:** Kodi (Python), Jellyfin (C#), and a Bazarr "custom
      provider" — all driving `ostd`.
- [ ] **Browser extension** (over the WASM build) for streaming sites.

## 6. Packaging & release — finish `v0.5`

- [ ] **Freeze `v1`**: make the `/v1` HTTP shapes, the C-ABI symbols, and the core
      JSON types semver-stable; gate with the conformance suite.
- [ ] **Signed builds + SBOM**; checksums already shipped.
- [ ] Distribution breadth: **nixpkgs**, **Homebrew tap**, **AUR**, a **container
      image** for `ostd`.

## 7. Quality / tech debt

- [ ] **Live integration tests** (gated `#[ignore]` + env) against real providers,
      run on demand — currently only hermetic tests in CI.
- [ ] **Engine `min_score` tuning** + per-language HI/forced preference handling in
      `download_best` (today: gate + best-of-language).
- [ ] **Throttle persistence** across `ost`/`ostd` runs (today in-memory only) —
      write `throttle.json` (path already in config `net.throttle_state`? add it).
- [ ] **`ost` interactive TUI** (search → pick → load) — referenced in docs, not
      built; CLI subcommands cover the flow for now.
- [ ] Wire `os-translate`/`os-sync`/`os-transcribe` into more CLI/daemon surfaces
      and add fixture/mock tests where feasible (external tools make this partial).

## 8. Cross-repo (open-media)

- [ ] **Episode-title bug:** open-media sets mpv `--force-media-title` to the
      series name only (no `SxxEyy`/episode title). Fix lives in open-media
      (`crates/om-app/src/lib.rs:192` + `episode_label`); a detailed agent prompt
      was drafted earlier in chat. Thread the `Episode` (title + coordinate) into
      `PlayRequest`.
- [ ] **open-media CI parity:** branch protection + release token fallback (see §0).

---

## Suggested next session order

1. `RELEASE_PLZ_TOKEN` secret + glibc `.so` leg (§0) — quick wins, closes the
   release pipeline gaps.
2. **Wanted list + re-search** (§2) — finishes the automation flagship.
3. **TVDB refiner + Gestdown/TVsubtitles** (§3) — provider breadth, low-risk.
4. **OSc `/login` + headers hardening** (§1) — makes the wedge a true drop-in.
5. Then pick from processing (§4) or frontends (§5) per appetite.
