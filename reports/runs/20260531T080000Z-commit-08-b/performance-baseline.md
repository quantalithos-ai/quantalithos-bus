# Performance Baseline

- Run ID: 20260531T080000Z-commit-08-b
- Baseline Method: fixed-run suite-duration sample
- Sample Count Per Area: 1

| Area | Source Suite | p50 ms | p95 ms | max ms |
|---|---|---|---|---|
| Publication acceptance | publication | 3057 | 3057 | 3057 |
| Delivery progression | delivery | 383 | 383 | 383 |
| Feedback recording | feedback | 62 | 62 | 62 |
| Read-only output | output | 68 | 68 | 68 |
| Recovery chain | recovery | 101 | 101 | 101 |

## Notes

- This boundary records one fixed-run baseline sample per P0 area from the release-gate suite duration metrics.
- The baseline is intended to support acceptance traceability and later comparisons, not a production-capacity claim.
