# EV-BUS-SEM-DLV-BND

- Run ID: `20260530T095936Z-commit-03-a`
- Phase / boundary: `PH-03 / commit-03-a`
- Reviewer: `Codex`
- Scope: semantic DTOs, backend capability fixture, transport semantic policy, delivery lifecycle to `Delivered / Failed`, attempt, history

## Commands

```text
cargo fmt
cargo test -p bus-contracts
cargo test -p bus-domain transport_semantic
cargo test -p bus-domain backend_capability_policy
cargo test -p bus-domain delivery_
cargo check
```

## Result Summary

- `bus-contracts`: 10 contract and fixture tests passed, including `GetDeliveryStatusQuery`, `DeliveryStatusView`, `RunDeliveryProgressionJob`, and backend capability fixture roundtrips.
- `bus-domain` semantic slice: 3 transport semantic tests passed.
- `bus-domain` backend slice: 3 backend capability policy tests passed.
- `bus-domain` delivery slice: 8 lifecycle, attempt, and history tests passed.
- Workspace compile gate: `cargo check` passed.

## Evidence

- Semantic contracts: [contracts.log](/home/aris/Projects/quantalithos-bus/artifacts/test/20260530T095936Z-commit-03-a/suites/semantic/contracts.log)
- Semantic domain: [domain.log](/home/aris/Projects/quantalithos-bus/artifacts/test/20260530T095936Z-commit-03-a/suites/semantic/domain.log)
- Backend domain: [domain.log](/home/aris/Projects/quantalithos-bus/artifacts/test/20260530T095936Z-commit-03-a/suites/backend/domain.log)
- Delivery domain: [domain.log](/home/aris/Projects/quantalithos-bus/artifacts/test/20260530T095936Z-commit-03-a/suites/delivery/domain.log)
- Workspace compile gate: [cargo-check.log](/home/aris/Projects/quantalithos-bus/artifacts/test/20260530T095936Z-commit-03-a/suites/delivery/cargo-check.log)

## Boundary Checks

- `TC-BUS-SEM-001`: accepted material plus backend capability derives platform semantic without exposing raw backend parameters.
- `TC-BUS-SEM-002`: suspicious backend-private capability input is rejected at semantic derivation and policy boundary.
- `TC-BUS-DLV-001` subset: scheduled delivery progresses through `Dispatching / Delivered`, records attempt data, and keeps feedback out of this boundary.
- `TC-BUS-DLV-003` subset: illegal delivery transitions such as skipping dispatching or reopening a delivered record are rejected.
- `TC-BUS-BND-001` subset: in-memory backend capability mapping is accepted as the P0 default path.

## Review Notes

- This boundary intentionally stops at `Delivered / Failed`. `FeedbackResult`, `RecordDeliveryFeedback`, and `DeliveryRecord::mark_completed(...)` remain out of scope for `PH-05 / commit-05-a`.
- `cargo test -p bus-contracts` still includes the existing publication contract regression cases because the crate-level fixture suite is shared; the new PH-03 contract DTOs and fixtures are covered inside the same log.
