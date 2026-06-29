---
status: "accepted"
date: "2026-06-29"
decision-makers: "thkt"
---

# Adopt the Group 3 (Hook tool) sysexits.h exit-code baseline

## Context and Problem Statement

formatter runs as a Claude Code PostToolUse hook: the agent runtime invokes it automatically after every Write/Edit and reads its exit code to decide whether the action succeeded. A non-zero exit is interpreted by the runtime as a failed action that interrupts the agent, so the exit code carries different meaning here than it would for an interactive CLI or a pipeline filter. The code already returns 0 for every formatting outcome and reserves 64/70 for infrastructure faults (PR #24, PR #29), and README.md / README.ja.md / `src/main.rs` / `tests/exit_code.rs` all cite this as the "Group 3 (Hook tool) baseline of ADR-0066". That ADR did not exist as a file, leaving the shipped behavior without a decision record. What exit-code contract should a hook tool in this position commit to?

## Decision Drivers

- A non-zero exit from a PostToolUse hook is read by the agent runtime as a failed action and interrupts the agent on every edit
- A metrics dashboard consuming the hook needs to branch on real infrastructure faults without treating an unformatted file as a failure
- The shipped code already encodes this split; an absent ADR leaves it as undocumented code shape that a future contributor could break

## Considered Options

- Group 3 (Hook tool) baseline: exit 0 for any formatting outcome, non-zero only for hook-infrastructure faults
- Treat formatter failures as non-zero so a consumer can detect them programmatically
- Use a single non-zero code for every fault class, without distinguishing input from internal bug

## Decision Outcome

Chosen option: "Group 3 (Hook tool) baseline", because the hook position demands non-interference for formatting outcomes while still surfacing faults that mean the hook itself could not run. The exit-code classification distinguishes how a CLI is consumed. A Group 3 tool is invoked automatically by an agent runtime, so it reserves non-zero strictly for faults that prevent the hook from honoring its contract, never for the formatting result. The baseline follows sysexits.h.

| Exit code | sysexits.h name | Condition                                                                                                                                    |
| --------- | --------------- | -------------------------------------------------------------------------------------------------------------------------------------------- |
| 0         | EX_OK           | Any formatting outcome: formatted, left unchanged, formatter unavailable, or formatter returned an error (the silent-fix policy of ADR-0002) |
| 64        | EX_USAGE        | The hook input on stdin violated the PostToolUse contract, it was not the expected JSON shape                                                |
| 70        | EX_SOFTWARE     | An internal invariant was violated, the hook caught a panic, a bug in the formatter itself                                                   |

### Consequences

- Good, because formatting never interrupts the agent; an unformatted file is at worst a stderr line with exit 0
- Good, because 64 and 70 separate a malformed hook contract from an internal bug, so a consumer can branch on the fault class
- Bad, because a real formatter misconfiguration stays exit 0 and is visible only on stderr, which the agent may not surface

### Confirmation

`src/main.rs` defines `EX_USAGE = 64` and `EX_SOFTWARE = 70` and maps `FormatterError::InvalidInput` and `FormatterError::Internal` to them via `exit_code`; `main` returns `ExitCode` and never escalates a formatting outcome to non-zero. The `exit_code_maps_each_variant_to_sysexits` unit test (`src/main.rs`) asserts the full 64 / 70 mapping, and `tests/exit_code.rs` asserts end to end that formatting outcomes exit 0 and malformed hook input exits 64.

## More Information

This ADR complements ADR-0002 rather than superseding it. ADR-0002 owns the invariant that formatting outcomes never block the agent (always exit 0) together with CWD write confinement. ADR-0066 records the exit-code taxonomy for the non-formatting fault class that ADR-0002 did not contemplate. Read together: formatting outcomes exit 0 (ADR-0002), hook-infrastructure faults exit 64 or 70 (ADR-0066).

### Reassessment Triggers

- A consumer needs a non-zero exit to detect formatting failures programmatically, which would move this tool out of the Group 3 baseline
- The PostToolUse hook contract changes the exit-code semantics the agent runtime expects
- A new fault class appears that neither 64 (malformed input) nor 70 (internal bug) describes
