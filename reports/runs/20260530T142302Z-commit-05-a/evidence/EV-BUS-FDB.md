# EV-BUS-FDB

- Run ID: `20260530T142302Z-commit-05-a`
- Phase / boundary: `PH-05 / commit-05-a`
- Reviewer: `Codex`
- Scope: feedback command DTOs, feedback truth/history, command idempotency, `DeliveryRecord.mark_completed(...)`, and command-path API wiring

## Commands

```text
cargo fmt --all
cargo test -p bus-contracts
cargo test -p bus-domain
cargo test -p bus-application
cargo test -p bus-api
cargo test --workspace
cargo check --workspace
```

## Result Summary

- `bus-contracts`: 22 fixture and contract tests passed, including `RecordDeliveryFeedbackCommand` and `FeedbackRecordResult` roundtrips.
- `bus-domain`: 31 domain tests passed, including ack/fail feedback construction, idempotency digest coverage, and `DeliveryRecord.mark_completed(...)` transition guards.
- `bus-application`: 4 existing delivery tests and 4 feedback integration tests passed for committed feedback truth, same-key same-digest reuse, same-key different-digest conflict, and unknown-delivery rejection.
- `bus-api`: 9 API tests passed, including feedback success, same-digest existing-result return, and unknown / late feedback status-code mapping.
- Workspace regression gate: full `cargo test --workspace` passed across all crates after wiring feedback contracts, domain truth, repositories, service, and API surface.
- Compile gate: `cargo check --workspace` passed with the new feedback command path linked through `contracts`, `domain`, `application`, `infra`, and `api`.

## Evidence

- Feedback contracts and fixtures: [contracts.log](/home/aris/Projects/quantalithos-bus/artifacts/test/20260530T142302Z-commit-05-a/suites/feedback/contracts.log)
- Feedback domain behavior: [domain.log](/home/aris/Projects/quantalithos-bus/artifacts/test/20260530T142302Z-commit-05-a/suites/feedback/domain.log)
- Feedback service behavior: [application.log](/home/aris/Projects/quantalithos-bus/artifacts/test/20260530T142302Z-commit-05-a/suites/feedback/application.log)
- Feedback API behavior: [api.log](/home/aris/Projects/quantalithos-bus/artifacts/test/20260530T142302Z-commit-05-a/suites/feedback/api.log)
- Workspace regression gate: [workspace.log](/home/aris/Projects/quantalithos-bus/artifacts/test/20260530T142302Z-commit-05-a/suites/feedback/workspace.log)
- Workspace compile gate: [cargo-check.log](/home/aris/Projects/quantalithos-bus/artifacts/test/20260530T142302Z-commit-05-a/suites/feedback/cargo-check.log)

## Boundary Checks

- `TC-BUS-FDB-001`: delivered delivery plus ack feedback now commits feedback truth, appends `Delivered -> Completed` history, and returns a recorded feedback receipt through the API.
- `TC-BUS-FDB-002`: reusing the same `x-idempotency-key` with the same feedback digest returns the existing committed feedback result without duplicate truth, duplicate audit, or duplicate history.
- `TC-BUS-FDB-003`: reusing the same `x-idempotency-key` with a different feedback digest records an idempotency conflict and returns `409 conflict.idempotency_request_mismatch` without mutating committed truth.
- `TC-BUS-FDB-004`: unknown delivery returns `404`, and late feedback after completion returns `409` without creating orphan feedback truth.
- `AC-FUNC-004`: ack / fail command-path feedback now produces feedback result, delivery-state side effect, history append, audit, and idempotency anchor.
- `AC-STATE-003`: feedback remains a one-shot result set; command-path ack completes delivery, command-path fail moves delivery to failed, and duplicate command retries do not reopen or restage truth.
- `AC-IDEM-001`: `RecordDeliveryFeedback` now enforces same-key same-digest existing-result reuse and same-key different-digest conflict semantics.

## Review Notes

- This boundary intentionally excludes timeout signal handling, backend delivery signal consumption, and the PH-05 / `commit-05-b` concurrency race surface. The only covered write path here is `RecordDeliveryFeedback`.
- `FeedbackSource` was introduced in domain code to reconcile the finalized command contract (`attempt_id`, `external_feedback_ref`) with the feedback uniqueness rule (`delivery_id + external_feedback_ref`) while keeping the committed feedback truth bus-owned and reference-only.
