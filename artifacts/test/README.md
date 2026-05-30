# Artifacts Root

Machine-generated evidence for L0-bus test runs must be written under
`artifacts/test/<run_id>`.

Required path rules:
- Use `artifacts/test/<run_id>`.
- Do not add a project layer such as `artifacts/test/quantalithos-bus/<run_id>`.
- Do not use `artifacts/test/latest` for formal evidence.

Expected run layout:

```text
artifacts/test/<run_id>/
  meta/context.json
  evidence-index.json
  fixtures/fixture-summary.json
  suites/<suite>/report.json
  suites/<suite>/stdout.log
  suites/<suite>/stderr.log
```
