# Backend Suite

- Run ID: 20260531T080000Z-commit-08-b
- Gate Suite: bus-release-closed-loop
- Status: passed
- Case IDs: TC-BUS-BND-001, TC-BUS-BND-002, TC-BUS-BND-003
- Evidence IDs: EV-BUS-BND-001, EV-BUS-BND-002, EV-BUS-BND-003
- Duration Ms: 64
- Artifact Report: artifacts/test/20260531T080000Z-commit-08-b/suites/backend/report.json
- Stdout Log: artifacts/test/20260531T080000Z-commit-08-b/suites/backend/stdout.log
- Stderr Log: artifacts/test/20260531T080000Z-commit-08-b/suites/backend/stderr.log
- Failed Command: none

## Commands

- cargo test -p bus-domain backend::tests
- cargo test -p bus-application services::delivery::tests
