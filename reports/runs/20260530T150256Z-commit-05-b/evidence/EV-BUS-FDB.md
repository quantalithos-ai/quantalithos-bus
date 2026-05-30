# EV-BUS-FDB

- Run ID: `20260530T150256Z-commit-05-b`
- Phase / boundary: `PH-05 / commit-05-b`
- Reviewer: `Codex`
- Scope: backend delivery signal consumption, timeout signal consumption, feedback conflict handling, signal idempotency, and feedback-vs-timeout race protection

## Commands

```text
cargo fmt --all
cargo test -p bus-contracts
cargo test -p bus-domain
cargo test -p bus-application --test feedback
cargo test -p bus-worker
cargo test --workspace
cargo check --workspace
```

## Result Summary

- `bus-contracts`: 26 contract tests passed, including backend signal / timeout DTO roundtrips and signal receipt serialization.
- `bus-domain`: 35 domain tests passed, including backend-signal feedback construction, timeout feedback construction, and signal idempotency scope / digest coverage.
- `bus-application`: 8 feedback integration tests passed for backend signal recording, timeout recording, same-key same-digest replay, same-key different-digest conflict, and feedback-vs-timeout state conflict.
- `bus-worker`: 8 consumer tests passed for delivered backend signal handling, duplicate backend signal reuse, unknown-delivery ignore with audit, private-body-like backend result rejection, and timeout duplicate reuse.
- Workspace regression gate: full `cargo test --workspace` passed across all crates after wiring signal contracts, normalization, consumer adapters, and conflict guards.
- Compile gate: `cargo check --workspace` passed with the signal and timeout path linked through `contracts`, `domain`, `application`, `infra`, and `worker`.

## Evidence

- Feedback contracts and fixtures: [contracts.log](/home/aris/Projects/quantalithos-bus/artifacts/test/20260530T150256Z-commit-05-b/suites/feedback/contracts.log)
- Feedback domain behavior: [domain.log](/home/aris/Projects/quantalithos-bus/artifacts/test/20260530T150256Z-commit-05-b/suites/feedback/domain.log)
- Feedback service behavior: [application.log](/home/aris/Projects/quantalithos-bus/artifacts/test/20260530T150256Z-commit-05-b/suites/feedback/application.log)
- Feedback consumer behavior: [worker.log](/home/aris/Projects/quantalithos-bus/artifacts/test/20260530T150256Z-commit-05-b/suites/feedback/worker.log)
- Workspace regression gate: [workspace.log](/home/aris/Projects/quantalithos-bus/artifacts/test/20260530T150256Z-commit-05-b/suites/feedback/workspace.log)
- Workspace compile gate: [cargo-check.log](/home/aris/Projects/quantalithos-bus/artifacts/test/20260530T150256Z-commit-05-b/suites/feedback/cargo-check.log)

## Boundary Checks

- `TC-BUS-FDB-001`: delivered delivery plus ack feedback remains green, and backend signal now records normalized ack / fail feedback without skipping history or audit.
- `TC-BUS-FDB-002`: same key plus same digest now returns existing committed results for both command-path feedback and backend / timeout signal replay without duplicate truth.
- `TC-BUS-FDB-003`: same key plus different digest still records an idempotency conflict and returns stable `409 conflict.idempotency_request_mismatch` without mutating committed truth.
- `TC-BUS-FDB-004`: unknown delivery still produces no orphan feedback truth, and timeout-first versus later feedback now surfaces a visible conflict instead of reopening delivery state.
- `AC-FUNC-004`: ack / fail / timeout paths now produce feedback truth, delivery history, audit, and idempotency anchors; duplicate signal replay returns existing committed results.
- `AC-IDEM-001`: `ConsumeBackendDeliverySignal` and `ConsumeTimeoutSignal` now enforce same-key same-digest existing-result reuse, while request-digest mismatch remains a conflict.
- `AC-CONC-002`: timeout now finishes the dispatching attempt, moves delivery to `Failed`, and blocks later ack completion so only one terminal outcome is committed.

## Review Notes

- This boundary intentionally excludes retry scheduling, dead-letter creation, replay preparation, and any `Failed -> Scheduled` reschedule logic. Timeout only marks `recovery_candidate=true`; it does not create a retry plan.
- `ConsumeBackendDeliverySignal` currently records unknown-delivery input as an ignored outcome with audit only and no feedback truth, matching the design requirement that unknown signals must not create orphan delivery or feedback records.
