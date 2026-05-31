# Delivery Suite

- Run ID: 20260531T080000Z-commit-08-b
- Gate Suite: bus-release-closed-loop
- Status: passed
- Case IDs: TC-BUS-DLV-001, TC-BUS-DLV-002, TC-BUS-DLV-003, TC-BUS-DLV-004
- Evidence IDs: EV-BUS-DLV-001, EV-BUS-DLV-002, EV-BUS-DLV-003, EV-BUS-DLV-004
- Duration Ms: 383
- Artifact Report: artifacts/test/20260531T080000Z-commit-08-b/suites/delivery/report.json
- Stdout Log: artifacts/test/20260531T080000Z-commit-08-b/suites/delivery/stdout.log
- Stderr Log: artifacts/test/20260531T080000Z-commit-08-b/suites/delivery/stderr.log
- Failed Command: none

## Commands

- cargo test -p bus-domain delivery::tests
- cargo test -p bus-jobs delivery_progression_job_runner
