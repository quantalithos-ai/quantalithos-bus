# Outbox Suite

- Run ID: 20260531T042454Z-commit-08-a
- Gate Suite: bus-release-closed-loop
- Status: passed
- Case IDs: TC-BUS-OBX-001, TC-BUS-OBX-002
- Evidence IDs: EV-BUS-OBX-001, EV-BUS-OBX-002
- Duration Ms: 70
- Artifact Report: artifacts/test/20260531T042454Z-commit-08-a/suites/outbox/report.json
- Stdout Log: artifacts/test/20260531T042454Z-commit-08-a/suites/outbox/stdout.log
- Stderr Log: artifacts/test/20260531T042454Z-commit-08-a/suites/outbox/stderr.log
- Failed Command: none

## Commands

- cargo test -p bus-infra source::tests
- cargo test -p bus-jobs outbox_relay_job_runner
