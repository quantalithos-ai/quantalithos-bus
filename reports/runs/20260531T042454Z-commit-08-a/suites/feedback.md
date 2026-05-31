# Feedback Suite

- Run ID: 20260531T042454Z-commit-08-a
- Gate Suite: bus-release-closed-loop
- Status: passed
- Case IDs: TC-BUS-FDB-001, TC-BUS-FDB-002, TC-BUS-FDB-003, TC-BUS-FDB-004
- Evidence IDs: EV-BUS-FDB-001, EV-BUS-FDB-002, EV-BUS-FDB-003, EV-BUS-FDB-004
- Duration Ms: 65
- Artifact Report: artifacts/test/20260531T042454Z-commit-08-a/suites/feedback/report.json
- Stdout Log: artifacts/test/20260531T042454Z-commit-08-a/suites/feedback/stdout.log
- Stderr Log: artifacts/test/20260531T042454Z-commit-08-a/suites/feedback/stderr.log
- Failed Command: none

## Commands

- cargo test -p bus-application --test feedback
- cargo test -p bus-api record_feedback
