---
status: "accepted"
date: "2026-06-29"
decision-makers: "thkt"
---

# Adopt oxfmt as the sole formatter, dropping biome and rustfmt

## Context and Problem Statement

formatter is a Claude Code PostToolUse hook that auto-formats files after Write/Edit. Earlier revisions shipped biome as a fallback and rustfmt for Rust files. Maintaining multiple formatter integrations multiplied the binary-resolution, availability-check, and error-handling paths. Which formatter set should the tool commit to?

## Decision Drivers

- Single integration path is cheaper to maintain and test than per-language branches
- oxfmt (from oxc.rs) covers the frontend ecosystem with Prettier-compatible output in one binary
- Rust formatting is already covered by cargo fmt in Rust projects; the hook need not duplicate it

## Considered Options

- oxfmt only, frontend-family extensions (ts/js/json/css/html/vue/yaml/toml/md/graphql ...)
- oxfmt with biome fallback when oxfmt is unavailable
- Multi-formatter (oxfmt + biome + rustfmt) dispatched by file type

## Decision Outcome

Chosen option: "oxfmt only, frontend-family extensions", because a single formatter collapses select/resolve/format to one path and oxfmt's language coverage spans the files this hook targets. Files oxfmt does not handle fall through to EOF-newline enforcement only.

### Consequences

- Good, because `select_formatter` reduces to a single `Formatter::Oxfmt` variant with no fallback chain
- Good, because removing rustfmt scopes the tool to frontend-only formatting, matching cargo fmt's existing role
- Bad, because projects relying on biome-specific behavior lose it; a legacy `biome` config key is now silently ignored, not honored

### Confirmation

`src/oxfmt.rs` exposes a single `EXTENSIONS` list and `src/main.rs` `select_formatter` returns only `Formatter::Oxfmt` or `None`. The `legacy_biome_key_is_ignored` test asserts a `biome` key is accepted-but-ignored. No biome/rustfmt invocation remains in `src/`.

## More Information

### Migration Strategy

Existing configs using `biome`/`rustfmt` keys are ignored without error (backward compatible). Users wanting Rust formatting use cargo fmt directly.

### Missing-binary Handling

Clarifies, without changing the decision, the fall-through scope above. "Files oxfmt does not handle fall through to EOF-newline enforcement only" is selected by extension: an extension outside `EXTENSIONS` takes the `None` arm of `select_formatter` and gets EOF-newline. An oxfmt-supported file is owned by oxfmt end to end. When the `oxfmt` binary is missing, `emit_spawn_failure` records it as an `error` outcome rather than degrading the supported file to bare EOF-newline. The `select_formatter` match is `Some(Oxfmt)` exclusive-or `None`, so one file never takes both paths.

### Reassessment Triggers

- oxfmt's maturity, stability, or supported-language set changes materially (e.g. drops a format this hook depends on, or a competing single-binary formatter surpasses its coverage)
- A concrete need arises to format languages outside oxfmt's range within the hook
