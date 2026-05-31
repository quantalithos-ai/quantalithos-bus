# EV-BUS-SEC

- Run ID: 20260531T080000Z-commit-08-b
- Evidence IDs: EV-BUS-SEC-001
- Assessment: passed
- Related Acceptance: AC-NFR-003, VETO-BUS-006
- Reviewed Sources: crates/contracts/src/queries.rs, crates/application/src/services/read_output.rs, crates/application/src/services/recovery.rs, crates/application/tests/output.rs, crates/application/tests/recovery.rs
- Summary: Source review found privileged-read authorization references, stable rejection coverage, and access-audit seams for failure-summary, audit-trail, and replay-preparation surfaces.

## Review Notes

- `GetFailureSummaryQuery` and `GetBusAuditTrailQuery` now carry optional `authorization_ref` fields for trusted privileged-read seams.
- `ReadOutputService` routes sensitive reads through `authorize_sensitive_read(...)` and persists append-only access audit entries for granted and rejected requests.
- Replay preparation now rejects actors without privileged role hints and records an access audit before returning a stable boundary violation.
- Tap surface assessment for this fixed run: present.
