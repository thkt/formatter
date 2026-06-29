---
status: "accepted"
date: "2026-06-30"
decision-makers: "thkt"
---

# Commit to a stable structured-output JSON record schema

## Context and Problem Statement

`src/report.rs` emits one JSON line per action to stderr so a consuming agent reads `degraded`/`next_step`/`notes` without parsing free-form text. The moment an agent or dashboard branches on these keys, the record shape becomes a cross-process contract: renaming a key, making an optional field mandatory, or growing the neutral record breaks every consumer silently. The `neutral_record_keeps_three_key_shape` test pins today's exact bytes, but a contributor can edit the test to match new output, so the test cannot by itself express the forward-compatibility rule a consumer relies on. What stability does the JSON record schema commit to?

## Decision Drivers

- An agent consumes the record across a process boundary; an unannounced schema change breaks it with no compile-time signal
- A neutral (success) record and a degraded record are distinguished by one flag, so a consumer must be able to branch on a single stable key
- The contract must hold for future fields, not just the fields shipped today

## Considered Options

- Commit to a stable schema: neutral = fixed three keys, every degraded-only field stays optional, additions are append-only
- Treat the JSON as advisory debug output with no compatibility guarantee
- Version the schema with an explicit `schema_version` field consumers must check

## Decision Outcome

Chosen option: "Commit to a stable schema", because the record exists precisely so an agent need not parse free-form stderr, and that value evaporates if the keys can shift underneath it. The committed rules are:

- A neutral record serializes to exactly `file`, `formatter`, `action` and nothing else. Consumers may treat the presence of `degraded` as the sole signal that a record is degraded.
- Every degraded-only field (`degraded`, `next_step`, `notes`) stays `skip_serializing_if`-optional, so it is absent (not empty) on records that do not carry it.
- Schema growth is append-only: a new field must be optional and must never appear on the neutral three-key record nor make an existing field mandatory.

### Consequences

- Good, because an agent branches on `degraded` and reads `next_step`/`notes` with no schema negotiation, and a neutral record never carries noise
- Good, because the append-only rule lets the record gain fields without a versioning handshake or a consumer migration
- Bad, because a genuinely breaking schema change (renaming a key, dropping a field, changing the neutral shape) now requires superseding this ADR, not a silent edit
- Bad, because the neutral record cannot carry diagnostic context; anything beyond the three keys forces the record into the degraded form

### Confirmation

`src/report.rs` keeps the `degraded`/`next_step`/`notes` fields under `#[serde(skip_serializing_if = ...)]` and the neutral constructor sets them empty. `neutral_record_keeps_three_key_shape` asserts the exact three-key serialization, `degraded_record_omits_empty_notes` asserts an absent (not empty) field, and `degraded_record_carries_next_step_and_notes` asserts the degraded keys. A change that adds a key to the neutral record, drops a degraded field, or makes one mandatory must update this ADR (supersede) rather than silently edit the tests.

## More Information

### Trade-offs

The append-only rule trades schema flexibility for consumer stability. It is the right trade because the consumer is an autonomous agent that cannot be coordinated with at change time; a `schema_version` handshake was rejected as ceremony a single-line stderr record does not warrant.

### Reassessment Triggers

- A consumer needs a field on the neutral record, which would break the fixed three-key shape and force a superseding decision
- The output sink moves off stderr, or the record carries something other than one JSON object per line
- A breaking schema change becomes unavoidable, at which point `schema_version` and supersession are reconsidered together
