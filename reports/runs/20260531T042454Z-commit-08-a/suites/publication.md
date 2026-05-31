# Publication Suite

- Run ID: 20260531T042454Z-commit-08-a
- Gate Suite: bus-release-closed-loop
- Status: passed
- Case IDs: TC-BUS-PUB-001, TC-BUS-PUB-002, TC-BUS-PUB-003, TC-BUS-PUB-004
- Evidence IDs: EV-BUS-PUB-001, EV-BUS-PUB-002, EV-BUS-PUB-003, EV-BUS-PUB-004
- Duration Ms: 358
- Artifact Report: artifacts/test/20260531T042454Z-commit-08-a/suites/publication/report.json
- Stdout Log: artifacts/test/20260531T042454Z-commit-08-a/suites/publication/stdout.log
- Stderr Log: artifacts/test/20260531T042454Z-commit-08-a/suites/publication/stderr.log
- Failed Command: none

## Commands

- cargo fmt --all --check
- cargo check --workspace
- cargo test -p bus-contracts accept_publication
- cargo test -p bus-domain publication::tests
- cargo test -p bus-api accept_publication
