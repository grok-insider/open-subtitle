# open-subtitle — mpv plugin

A thin Lua plugin that drives the `ost` CLI (open-subtitle's JSON sidecar) to find
and load subtitles **from inside mpv**, keyless by default.

## Install

1. Build/install the `ost` binary so it's on `PATH`
   (`cargo build --release -p os-cli` → `target/release/ost`).
2. Copy the script into mpv's scripts dir:
   ```sh
   cp open-subtitle.lua ~/.config/mpv/scripts/
   ```
3. (Optional) configure via `~/.config/mpv/script-opts/open-subtitle.conf`:
   ```conf
   ost_path=ost
   languages=en,es
   auto=no
   keybind=alt+s
   keybind_manual=alt+S
   ```

## Use

- **Alt+s** — download the best subtitle(s) for the current file/stream and load
  them. For a local file it passes the path (so the OSDB hash is used); for a
  stream it uses the media title.
- **Alt+S** — manual: type a query (mpv 0.38+), then download.
- Set `auto=yes` to auto-download on file load when no subtitle is present.

It writes sidecar files next to local videos, or into
`~/.cache/open-subtitle/mpv/` for streams, then `sub-add … select`s them.

## How it works

The plugin runs `ost get --json -l <langs> -o <dir> <input>` via mpv's
`subprocess`, parses the JSON array of results, and loads each `path` with
`sub-add`. All matching/scoring/decoding/conversion happens in the Rust engine;
the plugin is just glue. This is the same sidecar contract any host can use.

## NixOS / Home Manager

Deploy declaratively by sourcing this file into the mpv scripts dir, e.g.:

```nix
xdg.configFile."mpv/scripts/open-subtitle.lua".source =
  "${open-subtitle}/share/mpv/scripts/open-subtitle.lua";
```

and ensure the `ost` package is in `home.packages` / `environment.systemPackages`.
