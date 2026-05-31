# EV-BUS-OUT

- Run ID: `20260531T035026Z-commit-07-b`
- Phase / boundary: `PH-07 / commit-07-b`
- Reviewer: `Codex`
- Scope: audit trail reads, failure summary reads, outbound event payload contracts, in-memory publisher sink, retryable publish evidence, and redaction proof for the output boundary

## Commands

```text
cargo fmt --all
cargo check --workspace
cargo test -p bus-contracts events
cargo test -p bus-application --test output
cargo test -p bus-api query
cargo test --workspace
scripts/checks/check_redaction.sh --artifact-root artifacts/test/20260531T035026Z-commit-07-b --report-root reports
```

## Result Summary

- `bus-contracts` now defines the nine outbound payload DTOs plus `BusOutboundEvent` and batch wrappers, and validates schema version, required identifiers, and reference-only publication payload references.
- `bus-application` now exposes `OutboxPublisherService` and publisher port contracts, and the output test suite covers failure summary reads, audit-trail sequencing, fake-sink event emission, retryable publish evidence, and reference-only rejection before sink dispatch.
- `bus-infra` now provides an in-memory outbound publisher adapter that captures published events, reuses receipts for duplicates, and persists published or retryable evidence without touching committed delivery or projection state.
- `bus-api` now covers the failure-summary query surface alongside transport-view reads, confirming that governance-facing output returns stable failure references while decision content remains absent.

## Evidence

- Formatting gate: [fmt.log](/home/aris/Projects/quantalithos-bus/artifacts/test/20260531T035026Z-commit-07-b/suites/output/fmt.log)
- Workspace compile gate: [cargo-check.log](/home/aris/Projects/quantalithos-bus/artifacts/test/20260531T035026Z-commit-07-b/suites/output/cargo-check.log)
- Outbound event contract tests: [contracts.log](/home/aris/Projects/quantalithos-bus/artifacts/test/20260531T035026Z-commit-07-b/suites/output/contracts.log)
- Output service and publisher tests: [application.log](/home/aris/Projects/quantalithos-bus/artifacts/test/20260531T035026Z-commit-07-b/suites/output/application.log)
- Query API tests: [api.log](/home/aris/Projects/quantalithos-bus/artifacts/test/20260531T035026Z-commit-07-b/suites/output/api.log)
- Workspace regression gate: [workspace.log](/home/aris/Projects/quantalithos-bus/artifacts/test/20260531T035026Z-commit-07-b/suites/output/workspace.log)
- Redaction check log: [check.log](/home/aris/Projects/quantalithos-bus/artifacts/test/20260531T035026Z-commit-07-b/suites/redaction/check.log)

## Boundary Checks

- `TC-BUS-OUT-003`: failure summary queries now return stable failure references through service and API coverage with actor context present on the read path.
- `TC-BUS-OUT-004`: governance-facing failure output keeps decision content absent while preserving delivery and failure references.
- `TC-BUS-OUT-005`: audit-trail reads now prove append-only monotonic sequence behavior under filtered query output.
- `TC-BUS-OUT-006`: retryable publisher failure after committed projection state now leaves the committed state untouched and records retryable publish evidence in the fake adapter.
- `AC-FUNC-006`: audit trail, failure summary, transport view, and fake-sink output are readable with stale markers preserved from `commit-07-a`.
- `AC-IF-004`: outbound event payload flow now validates topic and schema shape through the in-memory sink path and does not publish inline reference violations.
- `AC-IF-009`: SDK, observability, governance, and operator seams now have query output plus fake-consumer sink evidence for transport-view and failure-material events.

## Review Notes

- This boundary keeps downstream SDK, observability, governance, and operator implementations outside the bus repo; the fake sink is the P0 proof surface for outbound collaboration.
- Retryable publisher failure is captured as adapter evidence instead of truth rollback. The current in-memory path records published and retryable evidence together with duplicate receipt reuse.
- Release-gate aggregation, acceptance handoff files, and report indexing beyond this run remain in `PH-08`.
