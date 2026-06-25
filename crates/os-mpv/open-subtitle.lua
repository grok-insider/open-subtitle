-- open-subtitle.lua — mpv plugin that drives the `ost` CLI (the sidecar contract)
-- to find & load subtitles, keyless by default.
--
-- Install: copy this file into ~/.config/mpv/scripts/ (and make sure `ost` is on
-- PATH, or set ost_path below / in script-opts/open-subtitle.conf).
--
-- Keybinds (default):
--   Alt+s  — download the best subtitle(s) for the current file (auto query)
--   Alt+S  — manual: type a query, then download (mpv 0.38+ for the input box)
--
-- Options (script-opts/open-subtitle.conf):
--   ost_path=ost
--   languages=en
--   auto=no            -- if yes, run automatically on file load when no sub is present
--   keybind=alt+s
--   keybind_manual=alt+S

local mp = require 'mp'
local utils = require 'mp.utils'
local options = require 'mp.options'

local opts = {
    ost_path = 'ost',
    languages = 'en',
    auto = false,
    keybind = 'alt+s',
    keybind_manual = 'alt+S',
}
options.read_options(opts, 'open-subtitle')

local running = false

local function osd(msg, secs)
    mp.osd_message('[open-subtitle] ' .. msg, secs or 3)
end

local function is_protocol(path)
    return path ~= nil and path:find('^%a[%a%d-_]*://') ~= nil
end

local function dirname(path)
    return path:match('^(.*)[/\\][^/\\]*$')
end

-- Decide the search input and output directory for the current file.
local function current_target()
    local path = mp.get_property('path')
    local out_dir, query, local_path

    if path and not is_protocol(path) then
        -- Local file: pass the full path (enables hashing) and write next to it.
        local_path = path
        out_dir = dirname(path) or '.'
    else
        -- Stream: use the media title; write to the cache dir.
        query = mp.get_property('media-title') or mp.get_property('filename')
        local home = os.getenv('HOME') or '/tmp'
        out_dir = home .. '/.cache/open-subtitle/mpv'
        mp.command_native({ name = 'subprocess', playback_only = false, args = { 'mkdir', '-p', out_dir } })
    end
    return local_path, query, out_dir
end

-- Load every subtitle path from an `ost get --json` result array.
local function load_results(json_text)
    local data = utils.parse_json(json_text)
    if not data or #data == 0 then
        osd('no subtitles found')
        return 0
    end
    local n = 0
    for _, item in ipairs(data) do
        if item.path then
            mp.commandv('sub-add', item.path, 'select')
            n = n + 1
        end
    end
    if n > 0 then
        local first = data[1]
        osd(string.format('loaded %d subtitle(s) — %s [%s]', n, first.language or '?', first.provider or '?'))
    end
    return n
end

-- Run `ost get` asynchronously and load the result.
local function run_get(extra_args, label)
    if running then
        osd('already searching…')
        return
    end
    local local_path, query, out_dir = current_target()
    local input = local_path or query
    if not input or input == '' then
        osd('nothing playing to search for')
        return
    end

    local args = { opts.ost_path, 'get', '--json', '-l', opts.languages, '-o', out_dir }
    for _, a in ipairs(extra_args or {}) do
        table.insert(args, a)
    end
    table.insert(args, input)

    running = true
    osd((label or 'searching') .. ' subtitles…', 30)
    mp.command_native_async({
        name = 'subprocess',
        playback_only = false,
        capture_stdout = true,
        capture_stderr = true,
        args = args,
    }, function(success, result)
        running = false
        if not success or not result or result.status ~= 0 then
            local err = (result and result.stderr) or 'unknown error'
            osd('search failed: ' .. tostring(err):gsub('%s+$', ''))
            return
        end
        load_results(result.stdout or '')
    end)
end

-- Auto query (current file/title).
local function find_subs()
    run_get(nil, 'searching')
end

-- Manual query via the input box (mpv 0.38+).
local function find_subs_manual()
    if not mp.input then
        osd('manual search needs mpv 0.38+; using auto')
        return find_subs()
    end
    mp.input.get({
        prompt = 'Subtitle search:',
        submit = function(text)
            mp.input.terminate()
            if text and text ~= '' then
                run_get({ text }, 'searching')
            end
        end,
    })
end

mp.add_key_binding(opts.keybind, 'find-subs', find_subs)
mp.add_key_binding(opts.keybind_manual, 'find-subs-manual', find_subs_manual)
mp.register_script_message('find-subs', find_subs)

-- Optional auto-download on file load when no subtitle track is present.
if opts.auto then
    mp.register_event('file-loaded', function()
        local count = mp.get_property_number('sub-tracks/count', 0)
        -- crude: only auto when no embedded/external sub is selected
        if mp.get_property('sid') == 'no' or count == 0 then
            find_subs()
        end
    end)
end
