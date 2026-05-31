# EV-BUS-OUT

- Run ID: `20260531T032048Z-commit-07-a`
- Phase / boundary: `PH-07 / commit-07-a`
- Reviewer: `Codex`
- Scope: read-only query DTOs and views, projection persistence, stale and missing consistency markers, and query no-write guard coverage

## Commands

```text
cargo fmt --all
cargo check --workspace
cargo test -p bus-application --test output
cargo test -p bus-api query
cargo test --workspace
```

## Result Summary

- `bus-contracts` now exposes the seven query DTOs plus view payloads required for publication acceptance, delivery status and history, transport view, failure summary, audit trail, and backend health reads.
- `bus-domain` now models projection freshness and read-only write intent, including stale marker handling and rejection of truth-writing intents from the query boundary.
- `bus-application`, `bus-infra`, and `bus-api` now serve committed transport projections without a write unit of work, return stale and missing markers without rebuild, and keep publication material snapshots available for later read models.
- Workspace regression coverage passed after wiring read projection repositories, delivery and feedback lookups, audit listing, and query API error mapping across the bus crates.

## Evidence

- Formatting gate: [fmt.log](/home/aris/Projects/quantalithos-bus/artifacts/test/20260531T032048Z-commit-07-a/suites/output/fmt.log)
- Workspace compile gate: [cargo-check.log](/home/aris/Projects/quantalithos-bus/artifacts/test/20260531T032048Z-commit-07-a/suites/output/cargo-check.log)
- Query service boundary tests: [application.log](/home/aris/Projects/quantalithos-bus/artifacts/test/20260531T032048Z-commit-07-a/suites/output/application.log)
- Query API tests: [api.log](/home/aris/Projects/quantalithos-bus/artifacts/test/20260531T032048Z-commit-07-a/suites/output/api.log)
- Workspace regression gate: [workspace.log](/home/aris/Projects/quantalithos-bus/artifacts/test/20260531T032048Z-commit-07-a/suites/output/workspace.log)

## Boundary Checks

- `TC-BUS-OUT-001`: current transport projection queries now return committed read-only view data and the service path does not write or rebuild projection truth.
- `TC-BUS-OUT-002`: stale projections now surface `ConsistencyMarker::Stale`, missing projections return not-found, and both paths avoid write-side recovery or rebuild.
- `AC-RED-005`: query execution is structurally read-only because the read-output service is wired without a write unit of work and rejects truth-writing projection intents.
- `AC-FUNC-006` partial: transport view and failure-summary projection contracts, audit listing support, and backend-health query DTOs exist, but tap output, outbound publisher, and redaction evidence remain outside this boundary.

## Review Notes

- `commit-07-a` intentionally stops at the query and projection read boundary. It does not yet implement outbound publisher emission, tap output materialization, or redaction evidence generation; those remain in `PH-07 / commit-07-b`.
- The explicit no-write guarantee is enforced in two places: the query use case has no write unit-of-work dependency, and the domain read-output policy rejects any projection write intent that would attempt to mutate bus truth.
- Publication acceptance writes now persist a material snapshot alongside acceptance truth so later query paths can reconstruct stable read DTOs without reaching back into external sources.
