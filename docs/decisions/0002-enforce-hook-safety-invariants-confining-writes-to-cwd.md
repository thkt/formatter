---
status: "accepted"
date: "2026-06-29"
decision-makers: "thkt"
---

# Enforce hook safety invariants confining writes to CWD and always exiting 0

## Context and Problem Statement

formatter runs as a Claude Code PostToolUse hook: it receives a file path on stdin and rewrites that file in place, on every Write/Edit the agent performs. Two properties make it safe to run unattended in that position. When this decision was first recorded, both were held only by code shape (no `process::exit`, an inline `starts_with` check), with no test or stated rule, so a future contributor adding a code path could silently break either. What invariants must every future change preserve?

## Decision Drivers

- A formatter must never rewrite files outside the project the agent is working in (path traversal / symlink escape)
- A PostToolUse hook that blocks or errors would interrupt the agent's workflow on every edit
- Invariants that live only in code shape drift; they need an enforceable statement

## Considered Options

- Confine writes to CWD and keep formatting outcomes at exit 0 (current behavior, made explicit and tested)
- Confine writes but surface formatter failures as non-zero exit
- No path confinement, rely on the agent only ever passing in-project paths

## Decision Outcome

Chosen option: "Confine writes to CWD and keep formatting outcomes at exit 0", because the hook position demands both non-interference and a hard write boundary. `validate_path` canonicalizes the path and rejects anything not under the current working directory. Formatting error paths log to stderr and stay at exit 0, while the non-formatting infrastructure faults classified in ADR-0066 (malformed hook input, internal panic) surface as 64 / 70.

### Consequences

- Good, because the agent is never blocked by formatting failures; the worst case is an unformatted file plus a stderr line
- Good, because canonicalize + `starts_with(cwd)` rejects symlink tricks, `..` traversal, and non-UTF-8 paths before any write
- Bad, because silent-on-failure can mask a real formatter misconfiguration; the only signal is stderr, which the agent may not surface

### Confirmation

Every future write path must pass through `validate_path` (`src/main.rs`), and every formatting outcome must exit 0. A non-zero exit is reserved for the non-formatting infrastructure faults classified in ADR-0066 (64 for malformed hook input, 70 for an internal panic). `validate_path_rejects_existing_file_outside_cwd` (`src/main.rs`) asserts an outside-CWD path is rejected before any write, `tests/exit_code.rs` asserts formatting outcomes exit 0 and malformed hook input exits 64 end to end, and the `exit_code_maps_each_variant_to_sysexits` unit test (`src/main.rs`) asserts the full 64 / 70 mapping.

## More Information

ADR-0066 complements this ADR rather than superseding it. The always-exit-0 invariant recorded here is scoped to formatting outcomes, which is the property that keeps the hook from blocking the agent. ADR-0066 records the sysexits.h exit-code taxonomy for the non-formatting infrastructure faults (malformed hook input, internal panic) that this ADR did not contemplate when written. This ADR stays accepted; the scope refinement narrows an overreaching absolute, it does not reverse the decision.

### Reassessment Triggers

- A use case requires formatting files outside the project root (e.g. a monorepo tool root above CWD)
- A consumer needs a non-zero exit to detect formatting failures programmatically, which would move this tool out of the silent-fix policy recorded here and in ADR-0066
