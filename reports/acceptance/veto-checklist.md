# Veto Checklist

- Run ID: 20260531T080000Z-commit-08-b
- Reviewer: Codex
- Overall Status: Pass
- Machine Review: reports/acceptance/20260531T080000Z-commit-08-b-veto.json

## Veto Review

| Veto ID | Status | Reason | Evidence |
|---|---|---|---|
| VETO-BUS-001 | Pass | The workspace contains a tap surface and the closed-loop reports cover read-output evidence. | reports/runs/20260531T080000Z-commit-08-b/evidence/EV-BUS-OUT.md |
| VETO-BUS-002 | Pass | The workspace still depends on the local core-contracts path snapshot and does not redefine the shared contract boundary in reports. | artifacts/test/20260531T080000Z-commit-08-b/meta/context.json |
| VETO-BUS-003 | Pass | Artifact, run-report, acceptance, and review documents passed the fixed-run redaction scan. | reports/runs/20260531T080000Z-commit-08-b/redaction-check.md |
| VETO-BUS-004 | Pass | Consistency and observability review reports both confirm append-only audit and history coverage for delivery, feedback, and recovery chains. | reports/runs/20260531T080000Z-commit-08-b/evidence/EV-BUS-CONS.md |
| VETO-BUS-005 | Pass | Recovery tests cover dead-letter creation, trusted audit-chain checks, and approval-backed replay readiness. | reports/runs/20260531T080000Z-commit-08-b/evidence/EV-BUS-REC.md |
| VETO-BUS-006 | Pass | Authorization seam evidence shows stable rejection and access-audit coverage for sensitive read and recovery surfaces. | reports/runs/20260531T080000Z-commit-08-b/evidence/EV-BUS-SEC.md |
| VETO-BUS-007 | Pass | Semantic and backend suites passed the normalized transport-boundary checks for the fixed run. | reports/runs/20260531T080000Z-commit-08-b/evidence/EV-BUS-BND.md |
| VETO-BUS-008 | Pass | Failure-summary reads keep governance decision references empty and stay within the bus failure-fact boundary. | reports/runs/20260531T080000Z-commit-08-b/evidence/EV-BUS-OUT.md |
| VETO-BUS-009 | Pass | The consistency review preserves the no-write query boundary for read projections and output material. | reports/runs/20260531T080000Z-commit-08-b/evidence/EV-BUS-CONS.md |
| VETO-BUS-010 | Pass | Fixed-run report, artifact, and acceptance handoff roots exist and can be linked together for final review. | reports/runs/20260531T080000Z-commit-08-b/artifact-index.md |
| VETO-BUS-011 | Pass | Config runtime and negative-fixture evidence still enforce the boundary policies for the fixed run. | reports/runs/20260531T080000Z-commit-08-b/evidence/EV-BUS-CFG-FAULT.md |
| VETO-BUS-012 | Pass | Committed outbox relay evidence passed duplicate handling and source-ack ordering checks for the fixed run. | reports/runs/20260531T080000Z-commit-08-b/evidence/EV-BUS-OBX.md |
