# EV-BUS-PUB

- Run ID: `20260530T090153Z-commit-02-b`
- Phase / boundary: `PH-02 / commit-02-b`
- Reviewer: `Codex`
- Scope: publication acceptance write path, in-memory repository / UoW, idempotency anchor, audit, minimal API

## Commands

```text
cargo test -p bus-contracts -p bus-domain -p bus-application -p bus-infra -p bus-api
cargo check
```

## Result Summary

- `bus-api`: 6 publication write-path tests passed.
- `bus-contracts`: 6 DTO / protocol validation tests passed.
- `bus-domain`: 7 publication domain tests passed.
- `bus-application`: compile checked.
- `bus-infra`: compile checked.

## Evidence

- Raw test log: [cargo-test.log](/home/aris/Projects/quantalithos-bus/artifacts/test/20260530T090153Z-commit-02-b/suites/publication/cargo-test.log)
- Raw compile log: [cargo-check.log](/home/aris/Projects/quantalithos-bus/artifacts/test/20260530T090153Z-commit-02-b/suites/publication/cargo-check.log)

## Covered Acceptance Points

- `TC-BUS-PUB-001`: accepted publication commits truth, audit, and idempotency anchor.
- `TC-BUS-PUB-002`: missing `core_event_ref` returns validation failure and does not create accepted truth.
- `TC-BUS-PUB-003`: payload-body-like input returns `422`, commits rejected truth, and does not persist the forbidden body in committed acceptance or audit objects.
- `TC-BUS-PUB-004`: terminal immutability remains covered by domain unit tests; duplicate same-digest requests return the existing result without rewriting truth.
- Rollback gate: staged publication truth and idempotency anchor are absent after injected audit append failure.

## Review Notes

- The write path is limited to publication acceptance, audit, and idempotency. Delivery, semantic derivation, outbox relay, and publisher work remain out of scope for this boundary.
- The boundary rejection path currently uses API error mapping (`422`) while still committing rejected truth and audit, matching the PH-02 publication boundary expectation.
