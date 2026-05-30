# EV-BUS-OBX

- Run ID: `20260530T110834Z-commit-04-a`
- Phase / boundary: `PH-04 / commit-04-a`
- Reviewer: `Codex`
- Scope: committed outbox fact DTOs, outbox source port, in-memory source fixture, cursor / ack / duplicate source semantics

## Commands

```text
cargo fmt
cargo test -p bus-contracts
cargo test -p bus-infra
cargo check
```

## Result Summary

- `bus-contracts`: 15 contract and fixture tests passed, including the new committed outbox fact and page DTO roundtrips plus forbidden-body rejection on source fixtures.
- `bus-infra`: 5 source-adapter tests passed for cursor progression, invalid cursor rejection, post-ack suppression, ack-failure replay, and duplicate fact-ref seeding rejection.
- Workspace compile gate: `cargo check` passed after wiring the source port into `contracts`, `application`, and `infra`.

## Evidence

- Outbox source contract fixtures: [contracts.log](/home/aris/Projects/quantalithos-bus/artifacts/test/20260530T110834Z-commit-04-a/suites/outbox/contracts.log)
- Outbox source adapter behavior: [infra.log](/home/aris/Projects/quantalithos-bus/artifacts/test/20260530T110834Z-commit-04-a/suites/outbox/infra.log)
- Workspace compile gate: [cargo-check.log](/home/aris/Projects/quantalithos-bus/artifacts/test/20260530T110834Z-commit-04-a/suites/outbox/cargo-check.log)

## Boundary Checks

- `TC-BUS-OBX-001` source subset: committed outbox fact fixtures stay reference-only and serialize without inline payload body fields, preserving the later consumer boundary.
- `TC-BUS-OBX-002` source subset: duplicate fact references are rejected at fixture seeding time instead of silently producing duplicate source truth.
- `TC-BUS-OBX-002` ack-recovery subset: source ack failure does not remove the committed fact; the same fact remains pollable until a later ack succeeds, matching the documented replay expectation.
- `AC-TX-002` source subset: ack state is applied after poll-time fact visibility and does not mutate bus truth because this boundary implements only the source side.
- `AC-IDEM-002` source subset: once a fact is acknowledged, later polls skip it for the in-memory source adapter instead of replaying it indefinitely.

## Review Notes

- This boundary does not implement `ConsumeCommittedOutboxFact`, `OutboxRelayService`, `RunOutboxRelay`, or publication acceptance from source facts. Those remain reserved for `PH-04 / commit-04-b`.
- The source fixture intentionally models only committed-fact polling, cursor validation, and ack recovery semantics. Consumer-level idempotency (`event_id + source_ref + idempotency_key`) is not claimed complete in this report.
