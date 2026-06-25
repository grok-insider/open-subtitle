# STRATEGY — the long-term direction

This document records the **deliberate long-term bet** for open-subtitle and the
decisions behind it. It is opinionated on purpose; `ROADMAP.md` turns it into
milestones and `PROTOCOL.md` specifies the contract it depends on.

## The bet, in one line

**Become the standard, embeddable subtitle *backend* that other software builds
on — a stable protocol (the `ostd` daemon + C-ABI/WASM) plus a sandboxed provider
SDK — and prove it by owning the self-hosted media-automation use case.**

First-party app plugins (mpv, VLC, IINA) are **reference clients of that
protocol, not the strategy.**

## Why this beats "ship more plugins"

- **Plugins are O(1) value, O(forever) maintenance.** Each bespoke plugin tracks
  one app's API churn permanently. A stable contract is O(1) to maintain and O(N)
  in value: every app the community wants becomes possible without us. That is the
  only way "agnostic across apps" becomes *true* rather than aspirational.
- **The volume is in automation, not in-player one-offs.** Most subtitles are
  consumed by people auto-filling libraries (the Sonarr/Radarr/Bazarr/Jellyfin
  crowd). That audience has a *recurring* need (every new episode), values our
  exact differentiators (keyless/no-cap, local-first, anime-grade), and is sticky
  once wired in.
- **We are ~80% there architecturally.** The engine already emits clean JSON;
  ports/adapters are clean; `ostd`/`os-ffi` exist. The remaining work is
  *commitment* (stability, versioning, a spec, conformance), which is
  high-leverage, not high-volume.
- **The LSP precedent.** Language Server Protocol won not by shipping editor
  plugins but by defining one contract every editor adopted. Subtitles are simpler
  than LSP; the same move is very achievable here.

## The decisions (committed)

| # | Question | Decision | Why |
|---|----------|----------|-----|
| 1 | Backend standard vs end-user in-player tool | **Backend standard.** Plugins are reference clients. | A platform compounds; a feature doesn't. |
| 2 | Provider SDK: WASM plugin system vs in-tree Rust adapters | **In-tree Rust now; keep the `Provider` port WASM-ready; build the WASM SDK later.** | Don't ship a security/complexity surface with no users; win breadth fast in-tree. |
| 3 | Automation: standalone vs ecosystem-integrated | **Ecosystem-integrated first** (an OpenSubtitles-compatible `ostd` surface + Sonarr/Radarr webhook consumer); standalone automation later. | Ride existing distribution instead of rebuilding scanning/scheduling/UI against a mature incumbent. |
| 4 | Freeze `v1` now vs keep fluid | **Keep fluid; freeze at v0.5. Write the spec descriptively now.** | Premature freezing locks in mistakes; a written spec keeps the design deliberate. |
| 5 | Build spec-first vs MVP-first | **Integration MVP first; harden the protocol from real use.** | Design the API from what the MVP actually needs, not ivory-tower-first. |

## The wedge: an OpenSubtitles-compatible endpoint

The single highest-leverage first move is exposing an **OpenSubtitles.com-API-
compatible REST surface from `ostd`**. Many players and tools already integrate
OpenSubtitles.com; pointing them at our **local** daemon gives them our keyless,
multi-provider, anime-grade engine with **near-zero client work**.

One build ticks four strategic boxes at once:

- it is the **integration MVP** (decision 5),
- it is the **wedge into existing ecosystems** (decision 3),
- it is the first concrete shape of the **backend standard** (decision 1),
- and it **pressure-tests the contract** before any freeze (decision 4).

See `PROTOCOL.md` for both the native protocol and this compatibility surface.

## The four pillars (priority order)

1. **A versioned, documented contract — the real product.** Promote today's
   ad-hoc JSON into `open-subtitle protocol v1`: identify / search / get / score +
   a progress event stream, capability negotiation, and a typed error model —
   exposed identically over the daemon (HTTP/JSON, local-first), the C-ABI, and
   WASM, with JSON Schemas, semver guarantees, and a **conformance suite**.
2. **The flagship application: self-hosted media automation.** Make `ostd` a
   drop-in subtitle service for the *-arr / Jellyfin / Kodi world: webhook on
   import + library scan + scheduled re-search + a "wanted" list — keyless-first.
   This creates daily, sticky usage and is the best forcing function for the
   protocol.
3. **A sandboxed provider SDK — the growth engine and the moat.** Let the
   community add sources **without forking**, as **WASM component-model** plugins
   (wasmtime) that are capability-scoped (host-allowlisted network only),
   cross-platform, and safe to run. Provider breadth becomes an ecosystem, not our
   backlog.
4. **Trustworthy, reproducible distribution.** Signed, reproducible releases
   (cargo-dist + Nix), SBOM, and presence in nixpkgs / Homebrew / AUR + a
   container image for `ostd`. A tool that touches the network and writes into
   media libraries lives or dies on trust.

## How the plugins fit (so the work isn't wasted)

Keep `ost plugin install <app>` and the mpv-family script — but framed as
**reference clients** that exercise the protocol and serve as copy-paste templates
for others. Same code, reframed: the plugin is a *demo of the platform*, not the
platform. The CLI sidecar (`ost --json`) remains the **zero-daemon** path; the
daemon is opt-in for persistent/multi-client use. Both speak the identical
contract, so nothing is duplicated.

## Anti-goals (what we will NOT over-invest in)

- A first-party GUI app — let the ecosystem build UIs on the protocol.
- A bespoke native plugin for every player — VLC/IINA are reference clients; the
  rest go through the daemon.
- Cloud anything — local-first is a feature; keep it.
- A WASM provider SDK *before* there is community demand for it.

## Honest risks & mitigations

- **A daemon is heavier than a one-file binary for a casual user.** → The CLI
  sidecar stays the zero-daemon path; the daemon is opt-in.
- **A stable API is a commitment.** → Version from day one (`/v1`), capability
  flags, conformance suite; don't promise semver until v0.5.
- **A provider SDK adds a trust/security surface.** → WASM sandbox + capability
  scoping + a signed, reviewed registry; never native plugins.
- **Owning automation invites comparison to Bazarr.** → We don't beat it on
  provider count on day one; we win on keyless-by-default, local-first,
  anime-grade, single static binary, and *being an embeddable backend*. That is a
  real, defensible wedge.

## What I'd validate before over-committing

Decision 3 is the riskiest. Before building automation UI/workflow, confirm real
demand with a handful of self-hosting / *-arr users. The OpenSubtitles-compatible
endpoint is low-risk regardless, so it is safe to build first while validating the
bigger automation play.

## Sequence (high level — see ROADMAP.md for milestones)

1. Descriptive **protocol spec** + JSON Schemas from the JSON we already emit.
2. **OpenSubtitles-compatible** REST surface on `ostd` (the wedge) + a
   Sonarr/Radarr **webhook consumer**.
3. **Reproducible/signed releases** + nixpkgs / Homebrew / AUR / container.
4. **Provider breadth** in-tree.
5. **Freeze `v1` at v0.5**; then the **WASM provider SDK** and a **standalone
   automation** mode if usage justifies it.
