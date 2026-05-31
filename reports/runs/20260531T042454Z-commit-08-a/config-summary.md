# Config Summary

- Run ID: 20260531T042454Z-commit-08-a
- Config Profile: operations-recovery
- Runtime Graph: store=in_memory, outbox_source=in_memory_fixture, backend=in_memory, capability=in-memory-backend, publisher=in_memory_sink, api_enabled=false, worker_enabled=false, jobs_retry_profile=conservative-local, projection=in_memory
- Secret Policy: ref_only
- Redaction Policy: required
- Reload Request: rejected
- Fixture Summary: artifacts/test/20260531T042454Z-commit-08-a/fixtures/fixture-summary.json

## Negative Cases

| Case | Expected Result | Source |
|---|---|---|
| TC-BUS-CFG-002 unsupported key | fail-fast | fixtures/config/negative/unsupported-key.json |
| TC-BUS-CFG-002 secret material fixture | fail-fast | fixtures/config/negative/raw-secret.json |
| TC-BUS-CFG-003 unavailable secret provider | fail-closed | fixtures/config/negative/secret-unavailable.json |
| TC-BUS-CFG-003 runtime reload request | rejected | fixtures/config/negative/reload-request.json |
