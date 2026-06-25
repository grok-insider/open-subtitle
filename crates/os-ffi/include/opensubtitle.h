/*
 * libopensubtitle — C ABI for the open-subtitle engine.
 *
 * Everything is JSON in, JSON out. Strings returned by ost_search/ost_get must
 * be released with ost_free. See docs/PROTOCOL.md and crates/os-ffi/src/lib.rs.
 *
 * Example:
 *   char *json = ost_search("Interstellar 2014", "en");
 *   // ... parse json ...
 *   ost_free(json);
 */
#ifndef OPENSUBTITLE_H
#define OPENSUBTITLE_H

#ifdef __cplusplus
extern "C" {
#endif

/* Library version string (static; do NOT free). */
const char *ost_version(void);

/* Search for subtitles. `input` = file path / release name / title; `langs` =
 * comma list (e.g. "en,es"). Returns a JSON array of scored candidates.
 * Caller owns the result — release it with ost_free. */
char *ost_search(const char *input, const char *langs);

/* Download the best subtitle(s). Returns a JSON array of SubtitleFile objects
 * (with inline UTF-8 `text`). Caller owns the result — release with ost_free. */
char *ost_get(const char *input, const char *langs);

/* Free a string returned by this library. */
void ost_free(char *ptr);

#ifdef __cplusplus
}
#endif

#endif /* OPENSUBTITLE_H */
