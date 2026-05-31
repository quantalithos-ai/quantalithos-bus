# Coverage Matrix

| Area | Case IDs | Acceptance Coverage | Evidence | Source Suite | Status |
|---|---|---|---|---|---|
| Publication | TC-BUS-PUB-001, TC-BUS-PUB-002, TC-BUS-PUB-003, TC-BUS-PUB-004 | AC-FUNC-001, AC-RED-001, AC-RED-002, AC-STATE-001, AC-TX-001 | EV-BUS-PUB-001~004 | publication | passed |
| Semantic | TC-BUS-SEM-001, TC-BUS-SEM-002 | AC-FUNC-002, AC-FUNC-008, AC-STATE-002 | EV-BUS-SEM-001~002 | semantic | passed |
| Delivery | TC-BUS-DLV-001, TC-BUS-DLV-002, TC-BUS-DLV-003, TC-BUS-DLV-004 | AC-FUNC-003, AC-STATE-002, AC-TX-003 | EV-BUS-DLV-001~004 | delivery | passed |
| Feedback | TC-BUS-FDB-001, TC-BUS-FDB-002, TC-BUS-FDB-003, TC-BUS-FDB-004 | AC-FUNC-004, AC-IDEM-001, AC-CONC-002 | EV-BUS-FDB-001~004 | feedback | passed |
| Recovery | TC-BUS-REC-001, TC-BUS-REC-002, TC-BUS-REC-003, TC-BUS-REC-004 | AC-FUNC-005, AC-STATE-004 | EV-BUS-REC-001~004 | recovery | passed |
| Output | TC-BUS-OUT-001, TC-BUS-OUT-002, TC-BUS-OUT-003, TC-BUS-OUT-004, TC-BUS-OUT-005, TC-BUS-OUT-006 | AC-FUNC-006, AC-IF-002, AC-IF-004, AC-IF-009, AC-NFR-005, AC-NFR-008 | EV-BUS-OUT-001~006 | output | passed |
| Outbox | TC-BUS-OBX-001, TC-BUS-OBX-002 | AC-FUNC-007, AC-IF-003, AC-IF-008, AC-TX-002, AC-IDEM-002 | EV-BUS-OBX-001~002 | outbox | passed |
| Backend Boundary | TC-BUS-BND-001, TC-BUS-BND-002, TC-BUS-BND-003 | AC-FUNC-008, AC-IF-007, AC-NFR-004 | EV-BUS-BND-001~003 | backend | passed |
| Config Runtime | TC-BUS-CFG-001, TC-BUS-CFG-002, TC-BUS-CFG-003 | AC-FUNC-009, AC-NFR-007, AC-EVID-006 | EV-BUS-CFG-001~003 | config | passed |
| Performance Baseline | fixed-run baseline sample | AC-NFR-001 | EV-BUS-PERF-001 | publication, delivery, feedback, output, recovery | passed |
| Authorization Seam | source review + service / API coverage | AC-NFR-003 | EV-BUS-SEC-001 | output, recovery | passed |
| Consistency And UoW | deterministic write-order checks | AC-TX-001, AC-TX-004, AC-NFR-005 | EV-BUS-CONS-001 | publication, feedback, outbox, output | passed |
| Idempotency And Concurrency | duplicate / conflict coverage | AC-IDEM-001, AC-CONC-001, AC-CONC-002, AC-NFR-006 | EV-BUS-IDEM-001 | publication, feedback, outbox | passed |
| Recovery Fault Injection | dependency-unavailable and recovery guards | AC-NFR-004, AC-CONC-002 | EV-BUS-REC-FAULT-001 | backend, recovery | passed |
| Config Failure Mode | negative config fixtures | AC-NFR-007 | EV-BUS-CFG-FAULT-001 | config | passed |
| Observability And Audit | append-only audit and operator read material | AC-NFR-005, AC-NFR-008, AC-EVID-001, AC-EVID-002 | EV-BUS-OBS-001 | output, recovery | passed |
| Redaction And Reports | fixed-run artifact and report integrity | AC-FUNC-010, AC-NFR-002, AC-NFR-009, AC-EVID-003, AC-EVID-004, AC-EVID-005, AC-EVID-007 | RP-BUS-RED-001, RP-BUS-SUM-001 | redaction, report | passed |
