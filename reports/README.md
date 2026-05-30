# Reports Root

Human-readable reports for L0-bus test runs live under `reports/`.

Required path rules:
- Run reports go to `reports/runs/<run_id>`.
- Acceptance handoff material goes to `reports/acceptance`.
- Review notes go to `reports/review`.
- Do not add a project layer such as `reports/quantalithos-bus`.
- Do not use `reports/runs/latest` for formal evidence.

Report generation scripts live under `scripts/reports/` and must not be placed in
`reports/`.
