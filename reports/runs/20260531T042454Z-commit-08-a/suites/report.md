# Report Suite

- Run ID: 20260531T042454Z-commit-08-a
- Gate Suite: bus-release-report
- Status: passed
- Case IDs: TC-BUS-RED-002
- Evidence IDs: RP-BUS-SUM-001
- Duration Ms: 766
- Artifact Report: artifacts/test/20260531T042454Z-commit-08-a/suites/report/report.json
- Stdout Log: artifacts/test/20260531T042454Z-commit-08-a/suites/report/stdout.log
- Stderr Log: artifacts/test/20260531T042454Z-commit-08-a/suites/report/stderr.log
- Failed Command: none

## Commands

- bash scripts/reports/generate_reports.sh --run-id 20260531T042454Z-commit-08-a --artifact-root artifacts/test/20260531T042454Z-commit-08-a --report-root reports
- bash scripts/reports/generate_acceptance_index.sh --run-id 20260531T042454Z-commit-08-a --report-root reports
- bash scripts/checks/check_artifact_layout.sh --artifact-root artifacts/test/20260531T042454Z-commit-08-a
- bash scripts/checks/check_report_links.sh --artifact-root artifacts/test/20260531T042454Z-commit-08-a --report-root reports
- bash scripts/checks/check_config_summary.sh --run-id 20260531T042454Z-commit-08-a --report-root reports
