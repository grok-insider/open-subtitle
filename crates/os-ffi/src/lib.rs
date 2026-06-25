//! # libopensubtitle (C ABI)
//!
//! A tiny, stable C surface so native apps (C/C++/Swift/Node/Python via cffi…)
//! can embed the engine. Everything is **JSON in, JSON out** across the boundary
//! to keep the ABI minimal. Strings returned by this library must be released
//! with [`ost_free`].
//!
//! ```c
//! char *json = ost_search("Interstellar 2014", "en");
//! // ... parse json ...
//! ost_free(json);
//! ```

use os_compose::build_engine;
use os_config::Config;
use os_core::ports::{MediaInput, ProcessOpts};
use os_core::Language;
use std::ffi::{c_char, CStr, CString};

/// Library version string (static; do NOT free).
#[no_mangle]
pub extern "C" fn ost_version() -> *const c_char {
    concat!(env!("CARGO_PKG_VERSION"), "\0").as_ptr() as *const c_char
}

/// Free a string returned by this library.
///
/// # Safety
/// `ptr` must have been returned by one of this library's functions and not
/// already freed.
#[no_mangle]
pub unsafe extern "C" fn ost_free(ptr: *mut c_char) {
    if !ptr.is_null() {
        drop(CString::from_raw(ptr));
    }
}

fn cstr_to_string(ptr: *const c_char) -> Option<String> {
    if ptr.is_null() {
        return None;
    }
    unsafe { CStr::from_ptr(ptr) }
        .to_str()
        .ok()
        .map(|s| s.to_string())
}

fn out(json: serde_json::Value) -> *mut c_char {
    CString::new(json.to_string())
        .unwrap_or_else(|_| CString::new("{\"error\":\"encoding\"}").unwrap())
        .into_raw()
}

fn err(msg: impl std::fmt::Display) -> *mut c_char {
    out(serde_json::json!({ "error": msg.to_string() }))
}

fn runtime() -> Result<tokio::runtime::Runtime, String> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())
}

/// Search for subtitles. `input` is a file path / release name / title; `langs`
/// is a comma list like `"en,es"`. Returns a JSON array of scored candidates.
///
/// # Safety
/// `input`/`langs` must be valid NUL-terminated C strings (or null).
#[no_mangle]
pub unsafe extern "C" fn ost_search(input: *const c_char, langs: *const c_char) -> *mut c_char {
    let input = match cstr_to_string(input) {
        Some(s) => s,
        None => return err("input required"),
    };
    let langs = cstr_to_string(langs).unwrap_or_else(|| "en".to_string());
    run_search(&input, &langs).unwrap_or_else(err)
}

/// Download the best subtitle(s). Returns a JSON array of `SubtitleFile`
/// (with inline UTF-8 `text`).
///
/// # Safety
/// `input`/`langs` must be valid NUL-terminated C strings (or null).
#[no_mangle]
pub unsafe extern "C" fn ost_get(input: *const c_char, langs: *const c_char) -> *mut c_char {
    let input = match cstr_to_string(input) {
        Some(s) => s,
        None => return err("input required"),
    };
    let langs = cstr_to_string(langs).unwrap_or_else(|| "en".to_string());
    run_get(&input, &langs).unwrap_or_else(err)
}

fn make_input(input: &str) -> MediaInput {
    let p = std::path::Path::new(input);
    if p.is_file() {
        MediaInput {
            path: Some(p.to_path_buf()),
            name: p.file_name().map(|s| s.to_string_lossy().into_owned()),
            ..Default::default()
        }
    } else {
        MediaInput {
            name: Some(input.to_string()),
            ..Default::default()
        }
    }
}

fn parse_langs(spec: &str) -> Vec<Language> {
    spec.split(',')
        .filter_map(|c| Language::parse(c.trim()))
        .collect()
}

fn run_search(input: &str, langs: &str) -> Result<*mut c_char, String> {
    let cfg = Config::load_default().map_err(|e| e.to_string())?;
    let engine = build_engine(&cfg).map_err(|e| e.to_string())?;
    let rt = runtime()?;
    let media_input = make_input(input);
    let langs = parse_langs(langs);
    let result = rt.block_on(async {
        let media = engine.identify(&media_input).await?;
        engine.search(&media, &langs).await
    });
    match result {
        Ok(cands) => Ok(out(serde_json::to_value(cands).unwrap_or_default())),
        Err(e) => Err(e.to_string()),
    }
}

fn run_get(input: &str, langs: &str) -> Result<*mut c_char, String> {
    let cfg = Config::load_default().map_err(|e| e.to_string())?;
    let engine = build_engine(&cfg).map_err(|e| e.to_string())?;
    let rt = runtime()?;
    let media_input = make_input(input);
    let langs = parse_langs(langs);
    let opts = ProcessOpts {
        to_utf8: cfg.process.to_utf8,
        target_format: Some(cfg.process.format.clone()),
        remove_hi: cfg.process.remove_hi,
        language: None,
    };
    let result = rt.block_on(async {
        let media = engine.identify(&media_input).await?;
        engine.download_best(&media, &langs, &opts).await
    });
    match result {
        Ok(files) => Ok(out(serde_json::to_value(files).unwrap_or_default())),
        Err(e) => Err(e.to_string()),
    }
}
