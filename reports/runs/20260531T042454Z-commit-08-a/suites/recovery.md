# Recovery Suite

- Run ID: 20260531T042454Z-commit-08-a
- Gate Suite: bus-release-recovery
- Status: passed
- Case IDs: TC-BUS-REC-001, TC-BUS-REC-002, TC-BUS-REC-003, TC-BUS-REC-004
- Evidence IDs: EV-BUS-REC-001, EV-BUS-REC-002, EV-BUS-REC-003, EV-BUS-REC-004
- Duration Ms: 104
- Artifact Report: artifacts/test/20260531T042454Z-commit-08-a/suites/recovery/report.json
- Stdout Log: artifacts/test/20260531T042454Z-commit-08-a/suites/recovery/stdout.log
- Stderr Log: artifacts/test/20260531T042454Z-commit-08-a/suites/recovery/stderr.log
- Failed Command: none

## Commands

- cargo test -p bus-domain recovery::tests
- cargo test -p bus-application --test recovery
- cargo test -p bus-jobs retry_cycle_job_runner
