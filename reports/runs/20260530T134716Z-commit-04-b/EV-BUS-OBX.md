# EV-BUS-OBX

- Run ID: `20260530T134716Z-commit-04-b`
- Phase / boundary: `PH-04 / commit-04-b`
- Reviewer: `Codex`
- Scope: committed outbox fact ingestion into publication acceptance, consumer/job relay wiring, source ack replay, and event-source idempotency

## Commands

```text
cargo fmt
cargo test -p bus-contracts
cargo test -p bus-infra
cargo test -p bus-worker
cargo test -p bus-jobs
cargo test
cargo check
```

## Result Summary

- `bus-contracts`: 20 contract and fixture tests passed, including committed outbox fact/input roundtrips, event metadata, relay receipt, and outbox job summary DTO coverage.
- `bus-infra`: 5 source-adapter tests passed for cursor progression, invalid cursor rejection, post-ack suppression, ack-failure replay, and duplicate fact-ref rejection.
- `bus-worker`: 3 consumer tests passed for accepted committed facts, duplicate reconsumption, and rejected missing `core_event_ref`.
- `bus-jobs`: 5 job-runner tests passed for batch acceptance, partial rejected continuation, source-unavailable retryable failure, and ack-failure replay without duplicate truth.
- Workspace regression gate: full `cargo test` passed across all workspace crates, including the shared publication rejection path now used by both command and outbox ingestion.
- Compile gate: `cargo check` passed after wiring the new outbox contracts and relay services through `contracts`, `domain`, `application`, `worker`, `jobs`, and `infra`.

## Evidence

- Outbox contracts and fixtures: [contracts.log](/home/aris/Projects/quantalithos-bus/artifacts/test/20260530T134716Z-commit-04-b/suites/outbox/contracts.log)
- Outbox source adapter behavior: [infra.log](/home/aris/Projects/quantalithos-bus/artifacts/test/20260530T134716Z-commit-04-b/suites/outbox/infra.log)
- Outbox consumer behavior: [worker.log](/home/aris/Projects/quantalithos-bus/artifacts/test/20260530T134716Z-commit-04-b/suites/outbox/worker.log)
- Outbox relay job behavior: [jobs.log](/home/aris/Projects/quantalithos-bus/artifacts/test/20260530T134716Z-commit-04-b/suites/outbox/jobs.log)
- Workspace regression gate: [workspace.log](/home/aris/Projects/quantalithos-bus/artifacts/test/20260530T134716Z-commit-04-b/suites/outbox/workspace.log)
- Workspace compile gate: [cargo-check.log](/home/aris/Projects/quantalithos-bus/artifacts/test/20260530T134716Z-commit-04-b/suites/outbox/cargo-check.log)

## Boundary Checks

- `TC-BUS-OBX-001`: committed outbox facts carrying `core_event_ref`, `core_event_envelope_ref`, `delivery_mode`, and `target_scope` are accepted through the outbox consumer and preserved in `PublicationMaterial`.
- `TC-BUS-OBX-002` duplicate subset: replaying the same committed fact returns the existing publication result instead of creating another acceptance fact.
- `TC-BUS-OBX-002` ack-recovery subset: source ack failure leaves committed truth intact, forces cursor replay, and the later replay is absorbed by idempotency before the source ack succeeds.
- `AC-FUNC-007`: outbox relay no longer conflates envelope and contract references; empty `core_event_ref` is rejected and never produces accepted truth.
- `AC-IF-003 / AC-IF-008`: committed source facts enter bus truth only through `ConsumeCommittedOutboxFact` / `RunOutboxRelay`, and source-level replay is handled without requiring business payload access.
- `AC-TX-002`: publication truth, audit, and idempotency anchor commit before source ack; ack failure is isolated to replay handling and does not roll back committed bus truth.
- `AC-IDEM-002`: event-source idempotency is enforced on `event_id + source_ref + idempotency_key`, covering direct duplicates and ack-failure replays.

## Review Notes

- The shared publication rejection path now records `Pending -> Rejected` truth for missing `core_event_ref`, which aligns the command and outbox acceptance flows with the finalized state matrix.
- This boundary claims the `RunOutboxRelayJob.dry_run` field only as a preserved contract field. The PH-04 OBX slice does not define a special dry-run execution mode, so no dry-run behavior is asserted by this report.
