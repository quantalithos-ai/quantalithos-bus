# EV-BUS-REC

- Run ID: `20260531T015941Z-commit-06-a`
- Phase / boundary: `PH-06 / commit-06-a`
- Reviewer: `Codex`
- Scope: recovery protocol contracts, recovery metadata, retry / dead-letter / replay domain objects, delivery recovery transitions, and recovery guard policy

## Commands

```text
cargo fmt --all
cargo test -p bus-contracts
cargo test -p bus-domain
cargo check --workspace
cargo test --workspace
```

## Result Summary

- `bus-contracts`: 32 contract tests passed, including request-retry / dead-letter / replay command DTO roundtrips and recovery receipt serialization.
- `bus-domain`: 43 domain tests passed, including `RetryPlan::create(...)` max-attempt initialization, failed-delivery dead-letter transitions, dead-letter trusted-chain checks, and draft-to-ready replay preparation guards.
- Workspace compile gate: `cargo check --workspace` passed after wiring recovery metadata, DTOs, domain objects, `DeliveryRecord.mark_dead_lettered(...)`, and extended application error mapping.
- Workspace regression gate: full `cargo test --workspace` passed across all crates after adding recovery protocol types and recovery-domain state transitions.

## Evidence

- Recovery contracts and fixtures: [contracts.log](/home/aris/Projects/quantalithos-bus/artifacts/test/20260531T015941Z-commit-06-a/suites/recovery/contracts.log)
- Recovery domain behavior: [domain.log](/home/aris/Projects/quantalithos-bus/artifacts/test/20260531T015941Z-commit-06-a/suites/recovery/domain.log)
- Workspace compile gate: [cargo-check.log](/home/aris/Projects/quantalithos-bus/artifacts/test/20260531T015941Z-commit-06-a/suites/recovery/cargo-check.log)
- Workspace regression gate: [workspace.log](/home/aris/Projects/quantalithos-bus/artifacts/test/20260531T015941Z-commit-06-a/suites/recovery/workspace.log)

## Boundary Checks

- `TC-BUS-REC-001`: failed delivery plus retry policy plus `max_attempts` now creates a scheduled retry plan whose `remaining_attempts` is initialized from `max_attempts`.
- `TC-BUS-REC-002`: dead-letter domain coverage now requires a failed delivery and existing failure material before `DeadLetterEntry::from_failed_delivery(...)` and `DeliveryRecord.mark_dead_lettered(...)` can succeed.
- `TC-BUS-REC-003`: replay preparation guard coverage now rejects closed dead letters, mismatched audit-chain references, and blank replay approvals before any ready state can be produced.
- `TC-BUS-REC-004`: replay preparation domain coverage now follows `Draft -> Ready` through `ReplayPreparation::prepare(...)` and `mark_ready(ReplayApprovalRef, ActorContext)`.
- `AC-FUNC-005`: retry / DLQ / replay protocol DTOs, recovery status enums, recovery guard policy, and minimal delivery recovery transitions are now present and validated at the domain boundary.
- `AC-STATE-004`: `RetryPlanStatus`, `DeadLetterStatus`, and `ReplayPreparationStatus` now follow the corrected state model, including `max_attempts -> remaining_attempts`, `Open / Reviewing / Closed`, and `Draft -> Ready / Rejected / Superseded`.
- `VETO-BUS-005`: replay preparation now requires trusted dead-letter and audit-chain material; the domain layer rejects replay-ready transitions that bypass approval or trusted-chain guard rails.

## Review Notes

- This boundary intentionally excludes recovery application services, repository write paths, approval-chain loading, idempotency anchors, and retry / DLQ / replay orchestration. Those remain for `PH-06 / commit-06-b`.
- The recovery DTOs and domain layer use normalized public names: `RetryPlanStatus::Scheduled / Exhausted / Cancelled`, `DeadLetterStatus::Open / Reviewing / Closed`, and `ReplayPreparationStatus::Draft / Ready / Rejected / Superseded`.
- The Step 8 dead-letter response example still shows legacy `"created"` in prose, but the typed implementation follows the normalized status enum mandated by the implementation plan and state matrix.
