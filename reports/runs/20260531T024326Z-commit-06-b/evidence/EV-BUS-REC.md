# EV-BUS-REC

- Run ID: `20260531T024326Z-commit-06-b`
- Phase / boundary: `PH-06 / commit-06-b`
- Reviewer: `Codex`
- Scope: recovery orchestration services, retry-cycle job runner, dead-letter persistence, replay approval / audit-chain guard, and recovery integration coverage

## Commands

```text
cargo fmt --all
cargo check --workspace
cargo test -p bus-application --test recovery
cargo test -p bus-jobs retry_cycle_job_runner
cargo test --workspace
```

## Result Summary

- `bus-application`: recovery integration coverage passed for `RequestRetry`, `MoveDeliveryToDeadLetter`, and `PrepareReplay`, including failed-delivery lookup, existing failure material linkage, dead-letter state commit, and replay ready / rejected guard paths.
- `bus-jobs`: retry-cycle runner coverage passed for due retry dispatch and zero-budget exhaustion, confirming per-plan isolation and stable job summary counters.
- `bus-infra`: in-memory recovery repository now commits retry plans, dead letters, replay preparations, failure-material updates, and audit-chain lookup under the same staged transaction model as prior phases.
- Workspace regression gate passed after wiring recovery ports, services, retry job DTOs, and recovery tests across all crates.

## Evidence

- Formatting gate: [fmt.log](/home/aris/Projects/quantalithos-bus/artifacts/test/20260531T024326Z-commit-06-b/suites/recovery/fmt.log)
- Workspace compile gate: [cargo-check.log](/home/aris/Projects/quantalithos-bus/artifacts/test/20260531T024326Z-commit-06-b/suites/recovery/cargo-check.log)
- Recovery application flow tests: [application.log](/home/aris/Projects/quantalithos-bus/artifacts/test/20260531T024326Z-commit-06-b/suites/recovery/application.log)
- Retry-cycle job tests: [jobs.log](/home/aris/Projects/quantalithos-bus/artifacts/test/20260531T024326Z-commit-06-b/suites/recovery/jobs.log)
- Workspace regression gate: [workspace.log](/home/aris/Projects/quantalithos-bus/artifacts/test/20260531T024326Z-commit-06-b/suites/recovery/workspace.log)

## Boundary Checks

- `TC-BUS-REC-001`: failed delivery plus existing failure material plus `max_attempts` now commits a scheduled retry plan with `remaining_attempts` initialized from the command and a recovery audit appended.
- `TC-BUS-REC-002`: failed delivery plus existing `failure_material_ref` now commits `DeliveryStatus::DeadLettered`, opens one dead-letter entry, and relinks the existing failure material to the new dead-letter reference.
- `TC-BUS-REC-003`: replay preparation now rejects blank approvals and missing audit-chain truth before any ready state is persisted.
- `TC-BUS-REC-004`: dead-letter plus trusted audit chain plus approval ref now commits `ReplayPreparationStatus::Ready` and appends replay readiness audit evidence.
- `AC-FUNC-005`: recovery write paths now exist for retry request, retry execution, dead-letter, and replay preparation without writing governance decision bodies or bypassing the controlled chain.
- `AC-RED-007`: replay ready state is gated by committed dead-letter truth and a resolvable audit-chain reference; missing chain or approval is rejected before persistence.
- `AC-STATE-004`: retry plans, dead letters, and replay preparations now advance through the corrected normalized states in application flow, and retry exhaustion is committed separately from DLQ.

## Review Notes

- This boundary still does not implement read projections, query output, or outbound publisher emission. `DeadLetterCreatedEvent` and `ReplayPreparationReadyEvent` payload publication remain in `PH-07`.
- `RunRetryCycleJob` follows the protocol schema with an explicit `now` field for deterministic scans. The current service uses that job-supplied timestamp as the retry execution time within the in-memory verification path.
- Existing failure material is seeded in recovery tests from failed-feedback truth and audit references. The later failure-summary / outbound materialization path remains outside `commit-06-b`.
