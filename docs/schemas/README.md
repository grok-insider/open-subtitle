# JSON Schemas

Machine-readable schemas for the open-subtitle contract (see
[../PROTOCOL.md](../PROTOCOL.md)). Draft 2020-12.

| File | Type |
|------|------|
| `language.schema.json` | `Language` |
| `media.schema.json` | `Media` (+ `IdSet`, `ReleaseInfo`) |
| `subtitle-candidate.schema.json` | `SubtitleCandidate` (a `/search` result) |
| `subtitle-file.schema.json` | `SubtitleFile` (a `/get` result) |
| `error.schema.json` | the typed error envelope |

**Status:** descriptive and tracking the engine until the protocol is frozen as
`v1` at **v0.5** (see `STRATEGY.md` decision 4). Breaking changes before then are
recorded in `CHANGELOG.md`. After `v0.5` these become semver-stable and are
enforced by the conformance suite.

`$id`s use the `https://opensubtitle.dev/schemas/` namespace as a stable
identifier; they need not resolve over the network. `$ref`s between schemas use
the same namespace, so a validator should register all files (e.g. load them into
one resolver/registry) before validating.
