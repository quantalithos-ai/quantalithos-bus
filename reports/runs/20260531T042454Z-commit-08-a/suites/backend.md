# Backend Suite

- Run ID: 20260531T042454Z-commit-08-a
- Gate Suite: bus-release-closed-loop
- Status: passed
- Case IDs: TC-BUS-BND-001, TC-BUS-BND-002, TC-BUS-BND-003
- Evidence IDs: EV-BUS-BND-001, EV-BUS-BND-002, EV-BUS-BND-003
- Duration Ms: 66
- Artifact Report: artifacts/test/20260531T042454Z-commit-08-a/suites/backend/report.json
- Stdout Log: artifacts/test/20260531T042454Z-commit-08-a/suites/backend/stdout.log
- Stderr Log: artifacts/test/20260531T042454Z-commit-08-a/suites/backend/stderr.log
- Failed Command: none

## Commands

- cargo test -p bus-domain backend::tests
- cargo test -p bus-application services::delivery::tests
