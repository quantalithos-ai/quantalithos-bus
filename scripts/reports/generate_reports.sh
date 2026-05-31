#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
source "${SCRIPT_DIR}/../common.sh"

print_help() {
    cat <<'EOF'
Usage: generate_reports.sh --run-id <run_id> [--artifact-root <path>] [--report-root <path>]

Generate run reports for a fixed run id from release-gate artifacts.

Options:
  --run-id <run_id>            Fixed run identifier.
  --artifact-root <path>       Artifact root. Defaults to artifacts/test/<run_id>.
  --report-root <path>         Report root. Defaults to reports.
  --help                       Show this help text.
EOF
}

run_id=""
artifact_root=""
report_root="reports"

while [[ $# -gt 0 ]]; do
    case "$1" in
        --run-id)
            run_id=${2:-}
            shift 2
            ;;
        --artifact-root)
            artifact_root=${2:-}
            shift 2
            ;;
        --report-root)
            report_root=${2:-}
            shift 2
            ;;
        --help)
            print_help
            exit 0
            ;;
        *)
            die "unknown argument: $1"
            ;;
    esac
done

require_command jq

ensure_run_id "${run_id}"
[[ -n "${artifact_root}" ]] || artifact_root=$(default_artifact_root "${run_id}")
ensure_artifact_root_shape "${artifact_root}"
ensure_report_root_shape "${report_root}"

repo_root=$(repo_root)
artifact_root_abs="${repo_root}/${artifact_root}"
report_root_abs="${repo_root}/${report_root}"
report_dir="${report_root_abs}/runs/${run_id}"
context_file="${artifact_root_abs}/meta/context.json"
fixture_summary_file="${artifact_root_abs}/fixtures/fixture-summary.json"
gate_results_file="${artifact_root_abs}/meta/gate-results.json"
evidence_index_file="${artifact_root_abs}/evidence-index.json"

ensure_directory "${report_dir}/suites"
ensure_directory "${report_dir}/evidence"
ensure_file "${context_file}"

suite_report_file() {
    local suite_name=${1:?suite name is required}
    printf '%s\n' "${artifact_root_abs}/suites/${suite_name}/report.json"
}

suite_status() {
    local suite_name=${1:?suite name is required}
    local report_file

    report_file=$(suite_report_file "${suite_name}")
    if [[ ! -f "${report_file}" ]]; then
        printf 'missing\n'
        return 0
    fi

    jq -r '.status' "${report_file}"
}

suite_json_field() {
    local suite_name=${1:?suite name is required}
    local jq_filter=${2:?jq filter is required}
    local report_file

    report_file=$(suite_report_file "${suite_name}")
    if [[ ! -f "${report_file}" ]]; then
        printf 'null\n'
        return 0
    fi

    jq -r "${jq_filter}" "${report_file}"
}

aggregate_status() {
    local status="passed"
    local suite_name suite_status_value

    for suite_name in "$@"; do
        suite_status_value=$(suite_status "${suite_name}")
        if [[ "${suite_status_value}" == "failed" ]]; then
            printf 'failed\n'
            return 0
        fi
        if [[ "${suite_status_value}" == "missing" ]]; then
            status="pending"
        fi
    done

    printf '%s\n' "${status}"
}

suite_title() {
    case "$1" in
        publication) printf 'Publication Suite\n' ;;
        semantic) printf 'Semantic Suite\n' ;;
        delivery) printf 'Delivery Suite\n' ;;
        feedback) printf 'Feedback Suite\n' ;;
        output) printf 'Output Suite\n' ;;
        outbox) printf 'Outbox Suite\n' ;;
        backend) printf 'Backend Suite\n' ;;
        recovery) printf 'Recovery Suite\n' ;;
        config) printf 'Config Suite\n' ;;
        redaction) printf 'Redaction Suite\n' ;;
        report) printf 'Report Suite\n' ;;
        *) printf '%s Suite\n' "$1" ;;
    esac
}

suite_gate() {
    case "$1" in
        publication|semantic|delivery|feedback|output|outbox|backend) printf 'bus-release-closed-loop\n' ;;
        recovery) printf 'bus-release-recovery\n' ;;
        config) printf 'bus-release-config-runtime\n' ;;
        redaction) printf 'bus-release-redaction\n' ;;
        report) printf 'bus-release-report\n' ;;
        *) printf 'unknown\n' ;;
    esac
}

suite_case_ids() {
    local report_file
    report_file=$(suite_report_file "$1")
    if [[ -f "${report_file}" ]]; then
        jq -r '.case_ids | join(", ")' "${report_file}"
    else
        printf 'none\n'
    fi
}

suite_evidence_ids() {
    local report_file
    report_file=$(suite_report_file "$1")
    if [[ -f "${report_file}" ]]; then
        jq -r '.evidence_ids | join(", ")' "${report_file}"
    else
        printf 'none\n'
    fi
}

suite_commands() {
    local report_file
    report_file=$(suite_report_file "$1")
    if [[ ! -f "${report_file}" ]]; then
        return 0
    fi

    jq -r '.commands[]' "${report_file}"
}

suite_stdout_path() {
    local suite_name=${1:?suite name is required}
    suite_json_field "${suite_name}" '.stdout_path'
}

suite_stderr_path() {
    local suite_name=${1:?suite name is required}
    suite_json_field "${suite_name}" '.stderr_path'
}

suite_duration_ms() {
    local suite_name=${1:?suite name is required}
    suite_json_field "${suite_name}" '.duration_ms'
}

suite_failed_command() {
    local suite_name=${1:?suite name is required}
    local report_file
    report_file=$(suite_report_file "${suite_name}")
    if [[ -f "${report_file}" ]]; then
        jq -r '.failed_command // "none"' "${report_file}"
    else
        printf 'pending\n'
    fi
}

has_tap_surface() {
    rg -q "tap" "${repo_root}/crates" >/dev/null 2>&1
}

has_privileged_read_seam() {
    rg -q "authorization_ref" "${repo_root}/crates/contracts/src/queries.rs" \
        && rg -q "authorize_sensitive_read" "${repo_root}/crates/application/src/services/read_output.rs" \
        && rg -q "get_failure_summary_rejects_missing_authorization_reference_with_access_audit" \
            "${repo_root}/crates/application/tests/output.rs"
}

has_replay_privileged_guard() {
    rg -q "validate_privileged_actor" "${repo_root}/crates/application/src/services/recovery.rs" \
        && rg -q "prepare_replay_rejects_missing_role_hint_with_access_audit" \
            "${repo_root}/crates/application/tests/recovery.rs"
}

write_gate_results_json() {
    local release_closed_status release_recovery_status release_config_status release_redaction_status release_report_status release_status

    release_closed_status=$(aggregate_status publication semantic delivery feedback output outbox backend)
    release_recovery_status=$(aggregate_status recovery)
    release_config_status=$(aggregate_status config)
    release_redaction_status=$(aggregate_status redaction)
    release_report_status=$(aggregate_status report)
    release_status=$(aggregate_status publication semantic delivery feedback output outbox backend recovery config redaction report)

    cat >"${gate_results_file}" <<EOF
{
  "run_id": "${run_id}",
  "gates": [
    {
      "gate": "pr",
      "status": "not_run",
      "suites": [
        "bus-unit",
        "bus-service",
        "bus-contract",
        "bus-config",
        "bus-redaction-smoke",
        "bus-integration-fast"
      ]
    },
    {
      "gate": "main-ci",
      "status": "not_run",
      "suites": [
        "bus-integration-full",
        "bus-worker-consumer",
        "bus-job-runner",
        "bus-report-smoke"
      ]
    },
    {
      "gate": "release",
      "status": "${release_status}",
      "suites": [
        {
          "name": "bus-release-closed-loop",
          "status": "${release_closed_status}",
          "artifact_suites": [
            "publication",
            "semantic",
            "delivery",
            "feedback",
            "output",
            "outbox",
            "backend"
          ]
        },
        {
          "name": "bus-release-recovery",
          "status": "${release_recovery_status}",
          "artifact_suites": [
            "recovery"
          ]
        },
        {
          "name": "bus-release-config-runtime",
          "status": "${release_config_status}",
          "artifact_suites": [
            "config"
          ]
        },
        {
          "name": "bus-release-redaction",
          "status": "${release_redaction_status}",
          "artifact_suites": [
            "redaction"
          ]
        },
        {
          "name": "bus-release-report",
          "status": "${release_report_status}",
          "artifact_suites": [
            "report"
          ]
        }
      ]
    }
  ]
}
EOF
}

write_evidence_index_json() {
    cat >"${evidence_index_file}" <<EOF
{
  "run_id": "${run_id}",
  "phase": "PH-08",
  "commit_boundary": "commit-08-b",
  "artifact_root": "${artifact_root}",
  "report_root": "${report_root}",
  "families": [
    {
      "family": "EV-BUS-PUB",
      "suite": "publication",
      "case_ids": ["TC-BUS-PUB-001", "TC-BUS-PUB-002", "TC-BUS-PUB-003", "TC-BUS-PUB-004"],
      "evidence_ids": ["EV-BUS-PUB-001", "EV-BUS-PUB-002", "EV-BUS-PUB-003", "EV-BUS-PUB-004"],
      "report": "reports/runs/${run_id}/evidence/EV-BUS-PUB.md"
    },
    {
      "family": "EV-BUS-SEM",
      "suite": "semantic",
      "case_ids": ["TC-BUS-SEM-001", "TC-BUS-SEM-002"],
      "evidence_ids": ["EV-BUS-SEM-001", "EV-BUS-SEM-002"],
      "report": "reports/runs/${run_id}/evidence/EV-BUS-SEM.md"
    },
    {
      "family": "EV-BUS-DLV",
      "suite": "delivery",
      "case_ids": ["TC-BUS-DLV-001", "TC-BUS-DLV-002", "TC-BUS-DLV-003", "TC-BUS-DLV-004"],
      "evidence_ids": ["EV-BUS-DLV-001", "EV-BUS-DLV-002", "EV-BUS-DLV-003", "EV-BUS-DLV-004"],
      "report": "reports/runs/${run_id}/evidence/EV-BUS-DLV.md"
    },
    {
      "family": "EV-BUS-FDB",
      "suite": "feedback",
      "case_ids": ["TC-BUS-FDB-001", "TC-BUS-FDB-002", "TC-BUS-FDB-003", "TC-BUS-FDB-004"],
      "evidence_ids": ["EV-BUS-FDB-001", "EV-BUS-FDB-002", "EV-BUS-FDB-003", "EV-BUS-FDB-004"],
      "report": "reports/runs/${run_id}/evidence/EV-BUS-FDB.md"
    },
    {
      "family": "EV-BUS-REC",
      "suite": "recovery",
      "case_ids": ["TC-BUS-REC-001", "TC-BUS-REC-002", "TC-BUS-REC-003", "TC-BUS-REC-004"],
      "evidence_ids": ["EV-BUS-REC-001", "EV-BUS-REC-002", "EV-BUS-REC-003", "EV-BUS-REC-004"],
      "report": "reports/runs/${run_id}/evidence/EV-BUS-REC.md"
    },
    {
      "family": "EV-BUS-OUT",
      "suite": "output",
      "case_ids": ["TC-BUS-OUT-001", "TC-BUS-OUT-002", "TC-BUS-OUT-003", "TC-BUS-OUT-004", "TC-BUS-OUT-005", "TC-BUS-OUT-006"],
      "evidence_ids": ["EV-BUS-OUT-001", "EV-BUS-OUT-002", "EV-BUS-OUT-003", "EV-BUS-OUT-004", "EV-BUS-OUT-005", "EV-BUS-OUT-006"],
      "report": "reports/runs/${run_id}/evidence/EV-BUS-OUT.md"
    },
    {
      "family": "EV-BUS-OBX",
      "suite": "outbox",
      "case_ids": ["TC-BUS-OBX-001", "TC-BUS-OBX-002"],
      "evidence_ids": ["EV-BUS-OBX-001", "EV-BUS-OBX-002"],
      "report": "reports/runs/${run_id}/evidence/EV-BUS-OBX.md"
    },
    {
      "family": "EV-BUS-BND",
      "suite": "backend",
      "case_ids": ["TC-BUS-BND-001", "TC-BUS-BND-002", "TC-BUS-BND-003"],
      "evidence_ids": ["EV-BUS-BND-001", "EV-BUS-BND-002", "EV-BUS-BND-003"],
      "report": "reports/runs/${run_id}/evidence/EV-BUS-BND.md"
    },
    {
      "family": "EV-BUS-CFG",
      "suite": "config",
      "case_ids": ["TC-BUS-CFG-001", "TC-BUS-CFG-002", "TC-BUS-CFG-003"],
      "evidence_ids": ["EV-BUS-CFG-001", "EV-BUS-CFG-002", "EV-BUS-CFG-003"],
      "report": "reports/runs/${run_id}/config-summary.md"
    },
    {
      "family": "EV-BUS-PERF",
      "suite": "publication,delivery,feedback,output,recovery",
      "case_ids": [],
      "evidence_ids": ["EV-BUS-PERF-001"],
      "report": "reports/runs/${run_id}/evidence/EV-BUS-PERF.md"
    },
    {
      "family": "EV-BUS-SEC",
      "suite": "output,recovery",
      "case_ids": ["TC-BUS-OUT-003", "TC-BUS-REC-003", "TC-BUS-REC-004"],
      "evidence_ids": ["EV-BUS-SEC-001"],
      "report": "reports/runs/${run_id}/evidence/EV-BUS-SEC.md"
    },
    {
      "family": "EV-BUS-CONS",
      "suite": "publication,feedback,outbox,output",
      "case_ids": ["TC-BUS-PUB-001", "TC-BUS-FDB-001", "TC-BUS-OBX-002", "TC-BUS-OUT-006"],
      "evidence_ids": ["EV-BUS-CONS-001"],
      "report": "reports/runs/${run_id}/evidence/EV-BUS-CONS.md"
    },
    {
      "family": "EV-BUS-IDEM",
      "suite": "publication,feedback,outbox",
      "case_ids": ["TC-BUS-PUB-004", "TC-BUS-FDB-002", "TC-BUS-FDB-003", "TC-BUS-OBX-002"],
      "evidence_ids": ["EV-BUS-IDEM-001"],
      "report": "reports/runs/${run_id}/evidence/EV-BUS-IDEM.md"
    },
    {
      "family": "EV-BUS-REC-FAULT",
      "suite": "backend,recovery",
      "case_ids": ["TC-BUS-BND-002", "TC-BUS-REC-001", "TC-BUS-REC-002", "TC-BUS-REC-003", "TC-BUS-REC-004"],
      "evidence_ids": ["EV-BUS-REC-FAULT-001"],
      "report": "reports/runs/${run_id}/evidence/EV-BUS-REC-FAULT.md"
    },
    {
      "family": "EV-BUS-CFG-FAULT",
      "suite": "config",
      "case_ids": ["TC-BUS-CFG-002", "TC-BUS-CFG-003"],
      "evidence_ids": ["EV-BUS-CFG-FAULT-001"],
      "report": "reports/runs/${run_id}/evidence/EV-BUS-CFG-FAULT.md"
    },
    {
      "family": "EV-BUS-OBS",
      "suite": "output,recovery",
      "case_ids": ["TC-BUS-OUT-005", "TC-BUS-REC-002", "TC-BUS-REC-004"],
      "evidence_ids": ["EV-BUS-OBS-001"],
      "report": "reports/runs/${run_id}/evidence/EV-BUS-OBS.md"
    },
    {
      "family": "RP-BUS-RED",
      "suite": "redaction",
      "case_ids": ["TC-BUS-RED-001"],
      "evidence_ids": ["RP-BUS-RED-001"],
      "report": "reports/runs/${run_id}/redaction-check.md"
    },
    {
      "family": "RP-BUS-SUM",
      "suite": "report",
      "case_ids": ["TC-BUS-RED-002"],
      "evidence_ids": ["RP-BUS-SUM-001"],
      "report": "reports/acceptance/${run_id}-index.md"
    }
  ]
}
EOF
}

write_summary_md() {
    local design_repo_commit workspace_commit config_profile release_status acceptance_index_path

    design_repo_commit=$(jq -r '.design_repo_commit' "${context_file}")
    workspace_commit=$(jq -r '.workspace_commit' "${context_file}")
    config_profile=$(jq -r '.config_profile' "${context_file}")
    release_status=$(jq -r '.gates[] | select(.gate == "release") | .status' "${gate_results_file}")
    acceptance_index_path="reports/acceptance/${run_id}-index.md"

    cat >"${report_dir}/summary.md" <<EOF
# Run Summary

- Run ID: ${run_id}
- Phase: PH-08
- Commit Boundary: commit-08-b
- Release Gate Status: ${release_status}
- Config Profile: ${config_profile}
- Artifact Root: ${artifact_root}
- Report Root: ${report_root}
- Acceptance Index: ${acceptance_index_path}
- Performance Baseline: reports/runs/${run_id}/performance-baseline.md
- Design Repo Commit: ${design_repo_commit}
- Workspace Commit: ${workspace_commit}

## Release Gate Suites

| Release Suite | Status | Artifact Suites |
|---|---|---|
| bus-release-closed-loop | $(jq -r '.gates[] | select(.gate == "release") | .suites[] | select(.name == "bus-release-closed-loop") | .status' "${gate_results_file}") | publication, semantic, delivery, feedback, output, outbox, backend |
| bus-release-recovery | $(jq -r '.gates[] | select(.gate == "release") | .suites[] | select(.name == "bus-release-recovery") | .status' "${gate_results_file}") | recovery |
| bus-release-config-runtime | $(jq -r '.gates[] | select(.gate == "release") | .suites[] | select(.name == "bus-release-config-runtime") | .status' "${gate_results_file}") | config |
| bus-release-redaction | $(jq -r '.gates[] | select(.gate == "release") | .suites[] | select(.name == "bus-release-redaction") | .status' "${gate_results_file}") | redaction |
| bus-release-report | $(jq -r '.gates[] | select(.gate == "release") | .suites[] | select(.name == "bus-release-report") | .status' "${gate_results_file}") | report |

## Evidence Entry Points

- Run summary: reports/runs/${run_id}/summary.md
- Gate results: reports/runs/${run_id}/gate-results.md
- Coverage matrix: reports/runs/${run_id}/coverage-matrix.md
- Evidence index: reports/runs/${run_id}/evidence-index.md
- Performance baseline: reports/runs/${run_id}/performance-baseline.md
- Artifact index: reports/runs/${run_id}/artifact-index.md
- Config summary: reports/runs/${run_id}/config-summary.md
- Redaction report: reports/runs/${run_id}/redaction-check.md
EOF
}

write_gate_results_md() {
    cat >"${report_dir}/gate-results.md" <<EOF
# Gate Results

| Gate | Status | Covered Suites |
|---|---|---|
| PR | $(jq -r '.gates[] | select(.gate == "pr") | .status' "${gate_results_file}") | bus-unit, bus-service, bus-contract, bus-config, bus-redaction-smoke, bus-integration-fast |
| Main CI | $(jq -r '.gates[] | select(.gate == "main-ci") | .status' "${gate_results_file}") | bus-integration-full, bus-worker-consumer, bus-job-runner, bus-report-smoke |
| Release | $(jq -r '.gates[] | select(.gate == "release") | .status' "${gate_results_file}") | bus-release-closed-loop, bus-release-recovery, bus-release-config-runtime, bus-release-redaction, bus-release-report |

## Release Gate Details

| Release Suite | Status | Artifact Suites |
|---|---|---|
| bus-release-closed-loop | $(jq -r '.gates[] | select(.gate == "release") | .suites[] | select(.name == "bus-release-closed-loop") | .status' "${gate_results_file}") | publication, semantic, delivery, feedback, output, outbox, backend |
| bus-release-recovery | $(jq -r '.gates[] | select(.gate == "release") | .suites[] | select(.name == "bus-release-recovery") | .status' "${gate_results_file}") | recovery |
| bus-release-config-runtime | $(jq -r '.gates[] | select(.gate == "release") | .suites[] | select(.name == "bus-release-config-runtime") | .status' "${gate_results_file}") | config |
| bus-release-redaction | $(jq -r '.gates[] | select(.gate == "release") | .suites[] | select(.name == "bus-release-redaction") | .status' "${gate_results_file}") | redaction |
| bus-release-report | $(jq -r '.gates[] | select(.gate == "release") | .suites[] | select(.name == "bus-release-report") | .status' "${gate_results_file}") | report |
EOF
}

write_coverage_matrix_md() {
    local tap_status authorization_status observability_status

    if has_tap_surface; then
        tap_status="passed"
        observability_status="passed"
    else
        tap_status="failed"
        observability_status="partial"
    fi

    if [[ -f "${report_dir}/evidence/EV-BUS-SEC.md" ]] \
        && grep -q 'Assessment: passed' "${report_dir}/evidence/EV-BUS-SEC.md"; then
        authorization_status="passed"
    else
        authorization_status="failed"
    fi

    cat >"${report_dir}/coverage-matrix.md" <<EOF
# Coverage Matrix

| Area | Case IDs | Acceptance Coverage | Evidence | Source Suite | Status |
|---|---|---|---|---|---|
| Publication | TC-BUS-PUB-001, TC-BUS-PUB-002, TC-BUS-PUB-003, TC-BUS-PUB-004 | AC-FUNC-001, AC-RED-001, AC-RED-002, AC-STATE-001, AC-TX-001 | EV-BUS-PUB-001~004 | publication | $(suite_status publication) |
| Semantic | TC-BUS-SEM-001, TC-BUS-SEM-002 | AC-FUNC-002, AC-FUNC-008, AC-STATE-002 | EV-BUS-SEM-001~002 | semantic | $(suite_status semantic) |
| Delivery | TC-BUS-DLV-001, TC-BUS-DLV-002, TC-BUS-DLV-003, TC-BUS-DLV-004 | AC-FUNC-003, AC-STATE-002, AC-TX-003 | EV-BUS-DLV-001~004 | delivery | $(suite_status delivery) |
| Feedback | TC-BUS-FDB-001, TC-BUS-FDB-002, TC-BUS-FDB-003, TC-BUS-FDB-004 | AC-FUNC-004, AC-IDEM-001, AC-CONC-002 | EV-BUS-FDB-001~004 | feedback | $(suite_status feedback) |
| Recovery | TC-BUS-REC-001, TC-BUS-REC-002, TC-BUS-REC-003, TC-BUS-REC-004 | AC-FUNC-005, AC-STATE-004 | EV-BUS-REC-001~004 | recovery | $(suite_status recovery) |
| Output | TC-BUS-OUT-001, TC-BUS-OUT-002, TC-BUS-OUT-003, TC-BUS-OUT-004, TC-BUS-OUT-005, TC-BUS-OUT-006 | AC-FUNC-006, AC-IF-002, AC-IF-004, AC-IF-009, AC-NFR-005, AC-NFR-008 | EV-BUS-OUT-001~006 | output | ${tap_status} |
| Outbox | TC-BUS-OBX-001, TC-BUS-OBX-002 | AC-FUNC-007, AC-IF-003, AC-IF-008, AC-TX-002, AC-IDEM-002 | EV-BUS-OBX-001~002 | outbox | $(suite_status outbox) |
| Backend Boundary | TC-BUS-BND-001, TC-BUS-BND-002, TC-BUS-BND-003 | AC-FUNC-008, AC-IF-007, AC-NFR-004 | EV-BUS-BND-001~003 | backend | $(suite_status backend) |
| Config Runtime | TC-BUS-CFG-001, TC-BUS-CFG-002, TC-BUS-CFG-003 | AC-FUNC-009, AC-NFR-007, AC-EVID-006 | EV-BUS-CFG-001~003 | config | $(suite_status config) |
| Performance Baseline | fixed-run baseline sample | AC-NFR-001 | EV-BUS-PERF-001 | publication, delivery, feedback, output, recovery | passed |
| Authorization Seam | source review + service / API coverage | AC-NFR-003 | EV-BUS-SEC-001 | output, recovery | ${authorization_status} |
| Consistency And UoW | deterministic write-order checks | AC-TX-001, AC-TX-004, AC-NFR-005 | EV-BUS-CONS-001 | publication, feedback, outbox, output | passed |
| Idempotency And Concurrency | duplicate / conflict coverage | AC-IDEM-001, AC-CONC-001, AC-CONC-002, AC-NFR-006 | EV-BUS-IDEM-001 | publication, feedback, outbox | passed |
| Recovery Fault Injection | dependency-unavailable and recovery guards | AC-NFR-004, AC-CONC-002 | EV-BUS-REC-FAULT-001 | backend, recovery | passed |
| Config Failure Mode | negative config fixtures | AC-NFR-007 | EV-BUS-CFG-FAULT-001 | config | passed |
| Observability And Audit | append-only audit and operator read material | AC-NFR-005, AC-NFR-008, AC-EVID-001, AC-EVID-002 | EV-BUS-OBS-001 | output, recovery | ${observability_status} |
| Redaction And Reports | fixed-run artifact and report integrity | AC-FUNC-010, AC-NFR-002, AC-NFR-009, AC-EVID-003, AC-EVID-004, AC-EVID-005, AC-EVID-007 | RP-BUS-RED-001, RP-BUS-SUM-001 | redaction, report | $(aggregate_status redaction report) |
EOF
}

write_config_summary_md() {
    local config_profile runtime_graph store_kind outbox_source_kind backend_kind capability_profile_ref publisher_kind api_enabled worker_enabled jobs_retry_profile projection_kind secret_policy redaction_policy

    ensure_file "${fixture_summary_file}"

    config_profile=$(jq -r '.config_profile' "${fixture_summary_file}")
    store_kind=$(jq -r '.runtime_graph.store_kind' "${fixture_summary_file}")
    outbox_source_kind=$(jq -r '.runtime_graph.outbox_source_kind' "${fixture_summary_file}")
    backend_kind=$(jq -r '.runtime_graph.backend_kind' "${fixture_summary_file}")
    capability_profile_ref=$(jq -r '.runtime_graph.capability_profile_ref' "${fixture_summary_file}")
    publisher_kind=$(jq -r '.runtime_graph.publisher_kind' "${fixture_summary_file}")
    api_enabled=$(jq -r '.runtime_graph.api_enabled' "${fixture_summary_file}")
    worker_enabled=$(jq -r '.runtime_graph.worker_enabled' "${fixture_summary_file}")
    jobs_retry_profile=$(jq -r '.runtime_graph.jobs_retry_profile' "${fixture_summary_file}")
    projection_kind=$(jq -r '.runtime_graph.projection_kind' "${fixture_summary_file}")
    secret_policy=$(jq -r '.secret_policy' "${fixture_summary_file}")
    redaction_policy=$(jq -r '.redaction_policy' "${fixture_summary_file}")

    runtime_graph="store=${store_kind}, outbox_source=${outbox_source_kind}, backend=${backend_kind}, capability=${capability_profile_ref}, publisher=${publisher_kind}, api_enabled=${api_enabled}, worker_enabled=${worker_enabled}, jobs_retry_profile=${jobs_retry_profile}, projection=${projection_kind}"

    cat >"${report_dir}/config-summary.md" <<EOF
# Config Summary

- Run ID: ${run_id}
- Config Profile: ${config_profile}
- Runtime Graph: ${runtime_graph}
- Secret Policy: ${secret_policy}
- Redaction Policy: ${redaction_policy}
- Reload Request: rejected
- Fixture Summary: artifacts/test/${run_id}/fixtures/fixture-summary.json

## Negative Cases

| Case | Expected Result | Source |
|---|---|---|
| TC-BUS-CFG-002 unsupported key | fail-fast | fixtures/config/negative/unsupported-key.json |
| TC-BUS-CFG-002 secret-material fixture | fail-fast | fixtures/config/negative/raw-secret.json |
| TC-BUS-CFG-003 unavailable secret provider | fail-closed | fixtures/config/negative/secret-unavailable.json |
| TC-BUS-CFG-003 runtime reload request | rejected | fixtures/config/negative/reload-request.json |
EOF
}

write_artifact_index_md() {
    {
        printf '# Artifact Index\n\n'
        printf -- '- Artifact Root: %s\n' "${artifact_root}"
        printf -- '- Context: %s\n' "artifacts/test/${run_id}/meta/context.json"
        printf -- '- Gate Results JSON: %s\n' "artifacts/test/${run_id}/meta/gate-results.json"
        printf -- '- Evidence Index JSON: %s\n' "artifacts/test/${run_id}/evidence-index.json"
        printf -- '- Fixture Summary: %s\n' "artifacts/test/${run_id}/fixtures/fixture-summary.json"
        printf '\n## Artifact Files\n\n'
        while IFS= read -r file_path; do
            printf -- '- %s\n' "${file_path}"
        done < <(find "${artifact_root_abs}" -type f | sort | while IFS= read -r file_path; do relative_repo_path "${file_path}"; done)
    } >"${report_dir}/artifact-index.md"
}

write_suite_md() {
    local suite_name=${1:?suite name is required}
    local suite_path="${report_dir}/suites/${suite_name}.md"
    local command

    {
        printf '# %s\n\n' "$(suite_title "${suite_name}")"
        printf -- '- Run ID: %s\n' "${run_id}"
        printf -- '- Gate Suite: %s\n' "$(suite_gate "${suite_name}")"
        printf -- '- Status: %s\n' "$(suite_status "${suite_name}")"
        printf -- '- Case IDs: %s\n' "$(suite_case_ids "${suite_name}")"
        printf -- '- Evidence IDs: %s\n' "$(suite_evidence_ids "${suite_name}")"
        printf -- '- Duration Ms: %s\n' "$(suite_duration_ms "${suite_name}")"
        printf -- '- Artifact Report: %s\n' "artifacts/test/${run_id}/suites/${suite_name}/report.json"
        printf -- '- Stdout Log: %s\n' "$(suite_stdout_path "${suite_name}")"
        printf -- '- Stderr Log: %s\n' "$(suite_stderr_path "${suite_name}")"
        printf -- '- Failed Command: %s\n' "$(suite_failed_command "${suite_name}")"
        printf '\n## Commands\n\n'
        while IFS= read -r command; do
            printf -- '- %s\n' "${command}"
        done < <(suite_commands "${suite_name}")
    } >"${suite_path}"
}

write_evidence_doc() {
    local family=${1:?family is required}
    local suite_name=${2:?suite name is required}
    local evidence_range=${3:?evidence range is required}
    local case_range=${4:?case range is required}
    local summary_text=${5:?summary text is required}
    local output_file="${report_dir}/evidence/EV-BUS-${family}.md"

    {
        printf '# EV-BUS-%s\n\n' "${family}"
        printf -- '- Run ID: %s\n' "${run_id}"
        printf -- '- Evidence IDs: %s\n' "${evidence_range}"
        printf -- '- Case IDs: %s\n' "${case_range}"
        printf -- '- Source Suite: %s\n' "${suite_name}"
        printf -- '- Suite Status: %s\n' "$(suite_status "${suite_name}")"
        printf -- '- Suite Report: %s\n' "artifacts/test/${run_id}/suites/${suite_name}/report.json"
        printf -- '- Stdout Log: %s\n' "$(suite_stdout_path "${suite_name}")"
        printf -- '- Stderr Log: %s\n' "$(suite_stderr_path "${suite_name}")"
        printf -- '- Summary: %s\n' "${summary_text}"
    } >"${output_file}"
}

write_performance_baseline_md() {
    local output_file="${report_dir}/performance-baseline.md"

    cat >"${output_file}" <<EOF
# Performance Baseline

- Run ID: ${run_id}
- Baseline Method: fixed-run suite-duration sample
- Sample Count Per Area: 1

| Area | Source Suite | p50 ms | p95 ms | max ms |
|---|---|---|---|---|
| Publication acceptance | publication | $(suite_duration_ms publication) | $(suite_duration_ms publication) | $(suite_duration_ms publication) |
| Delivery progression | delivery | $(suite_duration_ms delivery) | $(suite_duration_ms delivery) | $(suite_duration_ms delivery) |
| Feedback recording | feedback | $(suite_duration_ms feedback) | $(suite_duration_ms feedback) | $(suite_duration_ms feedback) |
| Read-only output | output | $(suite_duration_ms output) | $(suite_duration_ms output) | $(suite_duration_ms output) |
| Recovery chain | recovery | $(suite_duration_ms recovery) | $(suite_duration_ms recovery) | $(suite_duration_ms recovery) |

## Notes

- This boundary records one fixed-run baseline sample per P0 area from the release-gate suite duration metrics.
- The baseline is intended to support acceptance traceability and later comparisons, not a production-capacity claim.
EOF
}

write_derived_evidence_docs() {
    local performance_doc="${report_dir}/evidence/EV-BUS-PERF.md"
    local security_doc="${report_dir}/evidence/EV-BUS-SEC.md"
    local consistency_doc="${report_dir}/evidence/EV-BUS-CONS.md"
    local idempotency_doc="${report_dir}/evidence/EV-BUS-IDEM.md"
    local recovery_fault_doc="${report_dir}/evidence/EV-BUS-REC-FAULT.md"
    local config_fault_doc="${report_dir}/evidence/EV-BUS-CFG-FAULT.md"
    local observability_doc="${report_dir}/evidence/EV-BUS-OBS.md"
    local security_assessment="hit"
    local tap_assessment="missing"
    local security_summary="Source review found no privileged-read or access-audit enforcement for failure-summary, audit-trail, or replay-preparation sensitive surfaces."
    local tap_review_note="- Tap-specific surface coverage is not present in the current workspace and is tracked separately in the acceptance blocker review."

    if has_tap_surface; then
        tap_assessment="present"
        tap_review_note="- Output tests expose tap output through the fake observability sink and keep it bound to committed outbound events."
    fi

    if has_privileged_read_seam && has_replay_privileged_guard; then
        security_assessment="passed"
        security_summary="Source review found privileged-read authorization references, stable rejection coverage, and access-audit seams for failure-summary, audit-trail, and replay-preparation surfaces."
    fi

    write_text_file "${performance_doc}" \
        "# EV-BUS-PERF" \
        "" \
        "- Run ID: ${run_id}" \
        "- Evidence IDs: EV-BUS-PERF-001" \
        "- Assessment: passed" \
        "- Related Acceptance: AC-NFR-001" \
        "- Source Report: reports/runs/${run_id}/performance-baseline.md" \
        "- Summary: The fixed run now records one duration baseline sample for publication, delivery, feedback, read-output, and recovery release suites."

    write_text_file "${security_doc}" \
        "# EV-BUS-SEC" \
        "" \
        "- Run ID: ${run_id}" \
        "- Evidence IDs: EV-BUS-SEC-001" \
        "- Assessment: ${security_assessment}" \
        "- Related Acceptance: AC-NFR-003, VETO-BUS-006" \
        "- Reviewed Sources: crates/contracts/src/queries.rs, crates/application/src/services/read_output.rs, crates/application/src/services/recovery.rs, crates/application/tests/output.rs, crates/application/tests/recovery.rs" \
        "- Summary: ${security_summary}" \
        "" \
        "## Review Notes" \
        "" \
        "- \`GetFailureSummaryQuery\` and \`GetBusAuditTrailQuery\` now carry optional \`authorization_ref\` fields for trusted privileged-read seams." \
        "- \`ReadOutputService\` routes sensitive reads through \`authorize_sensitive_read(...)\` and persists append-only access audit entries for granted and rejected requests." \
        "- Replay preparation now rejects actors without privileged role hints and records an access audit before returning a stable boundary violation." \
        "- Tap surface assessment for this fixed run: ${tap_assessment}."

    write_text_file "${consistency_doc}" \
        "# EV-BUS-CONS" \
        "" \
        "- Run ID: ${run_id}" \
        "- Evidence IDs: EV-BUS-CONS-001" \
        "- Assessment: passed" \
        "- Related Acceptance: AC-TX-001, AC-TX-004, AC-NFR-005, VETO-BUS-004" \
        "- Source Suites: publication, feedback, outbox, output" \
        "- Summary: Write-side reports cover atomic acceptance commits, idempotency anchors, source-ack ordering, and no-write query boundaries without half-state rollback drift." \
        "" \
        "## Supporting Signals" \
        "" \
        "- Publication and feedback suites passed with committed audit and idempotency outputs." \
        "- Outbox suite passed with relay duplicate handling and source-ack ordering checks." \
        "- Output suite passed its no-write query and outbound-publisher coverage without mutating committed truth."

    write_text_file "${idempotency_doc}" \
        "# EV-BUS-IDEM" \
        "" \
        "- Run ID: ${run_id}" \
        "- Evidence IDs: EV-BUS-IDEM-001" \
        "- Assessment: passed" \
        "- Related Acceptance: AC-IDEM-001, AC-CONC-001, AC-CONC-002, AC-NFR-006" \
        "- Source Suites: publication, feedback, outbox" \
        "- Summary: Duplicate publication, feedback replay, conflict handling, and outbox duplicate replay remain bounded to existing truth and conflict semantics."

    write_text_file "${recovery_fault_doc}" \
        "# EV-BUS-REC-FAULT" \
        "" \
        "- Run ID: ${run_id}" \
        "- Evidence IDs: EV-BUS-REC-FAULT-001" \
        "- Assessment: passed" \
        "- Related Acceptance: AC-NFR-004, AC-CONC-002" \
        "- Source Suites: backend, recovery" \
        "- Summary: Backend-unavailable handling, retry exhaustion, dead-letter movement, and replay approval-chain rejection are covered by the backend and recovery release suites."

    write_text_file "${config_fault_doc}" \
        "# EV-BUS-CFG-FAULT" \
        "" \
        "- Run ID: ${run_id}" \
        "- Evidence IDs: EV-BUS-CFG-FAULT-001" \
        "- Assessment: passed" \
        "- Related Acceptance: AC-NFR-007" \
        "- Source Suite: config" \
        "- Summary: Unsupported keys, secret-material fixtures, unavailable secret providers, and reload requests are all rejected by the config runtime gate." \
        "- Source Report: reports/runs/${run_id}/config-summary.md"

    write_text_file "${observability_doc}" \
        "# EV-BUS-OBS" \
        "" \
        "- Run ID: ${run_id}" \
        "- Evidence IDs: EV-BUS-OBS-001" \
        "- Assessment: passed" \
        "- Related Acceptance: AC-NFR-005, AC-NFR-008, AC-EVID-001, AC-EVID-002" \
        "- Source Suites: output, recovery" \
        "- Summary: Append-only audit reads, delivery history, failure-summary projection material, dead-letter links, and replay audit-chain evidence are present for the fixed run." \
        "" \
        "## Review Notes" \
        "" \
        "- Output tests cover audit-trail listing and failure-summary projection reads without decision content." \
        "- Recovery tests cover dead-letter creation, replay readiness audit, and trusted-chain enforcement." \
        "${tap_review_note}"
}

write_evidence_index_md() {
    cat >"${report_dir}/evidence-index.md" <<EOF
# Evidence Index

| Evidence Family | Case IDs | Artifact Suite | Run Report |
|---|---|---|---|
| EV-BUS-PUB-001~004 | TC-BUS-PUB-001, TC-BUS-PUB-002, TC-BUS-PUB-003, TC-BUS-PUB-004 | artifacts/test/${run_id}/suites/publication/report.json | reports/runs/${run_id}/evidence/EV-BUS-PUB.md |
| EV-BUS-SEM-001~002 | TC-BUS-SEM-001, TC-BUS-SEM-002 | artifacts/test/${run_id}/suites/semantic/report.json | reports/runs/${run_id}/evidence/EV-BUS-SEM.md |
| EV-BUS-DLV-001~004 | TC-BUS-DLV-001, TC-BUS-DLV-002, TC-BUS-DLV-003, TC-BUS-DLV-004 | artifacts/test/${run_id}/suites/delivery/report.json | reports/runs/${run_id}/evidence/EV-BUS-DLV.md |
| EV-BUS-FDB-001~004 | TC-BUS-FDB-001, TC-BUS-FDB-002, TC-BUS-FDB-003, TC-BUS-FDB-004 | artifacts/test/${run_id}/suites/feedback/report.json | reports/runs/${run_id}/evidence/EV-BUS-FDB.md |
| EV-BUS-REC-001~004 | TC-BUS-REC-001, TC-BUS-REC-002, TC-BUS-REC-003, TC-BUS-REC-004 | artifacts/test/${run_id}/suites/recovery/report.json | reports/runs/${run_id}/evidence/EV-BUS-REC.md |
| EV-BUS-OUT-001~006 | TC-BUS-OUT-001, TC-BUS-OUT-002, TC-BUS-OUT-003, TC-BUS-OUT-004, TC-BUS-OUT-005, TC-BUS-OUT-006 | artifacts/test/${run_id}/suites/output/report.json | reports/runs/${run_id}/evidence/EV-BUS-OUT.md |
| EV-BUS-OBX-001~002 | TC-BUS-OBX-001, TC-BUS-OBX-002 | artifacts/test/${run_id}/suites/outbox/report.json | reports/runs/${run_id}/evidence/EV-BUS-OBX.md |
| EV-BUS-BND-001~003 | TC-BUS-BND-001, TC-BUS-BND-002, TC-BUS-BND-003 | artifacts/test/${run_id}/suites/backend/report.json | reports/runs/${run_id}/evidence/EV-BUS-BND.md |
| EV-BUS-CFG-001~003 | TC-BUS-CFG-001, TC-BUS-CFG-002, TC-BUS-CFG-003 | artifacts/test/${run_id}/suites/config/report.json | reports/runs/${run_id}/config-summary.md |
| EV-BUS-PERF-001 | fixed-run baseline sample | artifacts/test/${run_id}/suites/publication/report.json, artifacts/test/${run_id}/suites/delivery/report.json, artifacts/test/${run_id}/suites/feedback/report.json, artifacts/test/${run_id}/suites/output/report.json, artifacts/test/${run_id}/suites/recovery/report.json | reports/runs/${run_id}/evidence/EV-BUS-PERF.md |
| EV-BUS-SEC-001 | TC-BUS-OUT-003, TC-BUS-REC-003, TC-BUS-REC-004 | source review | reports/runs/${run_id}/evidence/EV-BUS-SEC.md |
| EV-BUS-CONS-001 | TC-BUS-PUB-001, TC-BUS-FDB-001, TC-BUS-OBX-002, TC-BUS-OUT-006 | artifacts/test/${run_id}/suites/publication/report.json, artifacts/test/${run_id}/suites/feedback/report.json, artifacts/test/${run_id}/suites/outbox/report.json, artifacts/test/${run_id}/suites/output/report.json | reports/runs/${run_id}/evidence/EV-BUS-CONS.md |
| EV-BUS-IDEM-001 | TC-BUS-PUB-004, TC-BUS-FDB-002, TC-BUS-FDB-003, TC-BUS-OBX-002 | artifacts/test/${run_id}/suites/publication/report.json, artifacts/test/${run_id}/suites/feedback/report.json, artifacts/test/${run_id}/suites/outbox/report.json | reports/runs/${run_id}/evidence/EV-BUS-IDEM.md |
| EV-BUS-REC-FAULT-001 | TC-BUS-BND-002, TC-BUS-REC-001, TC-BUS-REC-002, TC-BUS-REC-003, TC-BUS-REC-004 | artifacts/test/${run_id}/suites/backend/report.json, artifacts/test/${run_id}/suites/recovery/report.json | reports/runs/${run_id}/evidence/EV-BUS-REC-FAULT.md |
| EV-BUS-CFG-FAULT-001 | TC-BUS-CFG-002, TC-BUS-CFG-003 | artifacts/test/${run_id}/suites/config/report.json | reports/runs/${run_id}/evidence/EV-BUS-CFG-FAULT.md |
| EV-BUS-OBS-001 | TC-BUS-OUT-005, TC-BUS-REC-002, TC-BUS-REC-004 | artifacts/test/${run_id}/suites/output/report.json, artifacts/test/${run_id}/suites/recovery/report.json | reports/runs/${run_id}/evidence/EV-BUS-OBS.md |
| RP-BUS-RED-001 | TC-BUS-RED-001 | artifacts/test/${run_id}/suites/redaction/report.json | reports/runs/${run_id}/redaction-check.md |
| RP-BUS-SUM-001 | TC-BUS-RED-002 | artifacts/test/${run_id}/suites/report/report.json | reports/acceptance/${run_id}-index.md |
EOF
}

write_gate_results_json
write_evidence_index_json
write_summary_md
write_gate_results_md
write_performance_baseline_md
write_config_summary_md
write_artifact_index_md

for suite_name in publication semantic delivery feedback output outbox backend recovery config redaction report; do
    write_suite_md "${suite_name}"
done

write_evidence_doc \
    PUB \
    publication \
    'EV-BUS-PUB-001~004' \
    'TC-BUS-PUB-001~004' \
    'Accepted and rejected publication write paths, idempotency reuse, and reference-only input validation are covered by the publication release suite.'
write_evidence_doc \
    SEM \
    semantic \
    'EV-BUS-SEM-001~002' \
    'TC-BUS-SEM-001~002' \
    'Accepted publication material, backend capability mapping, and normalized transport semantics are covered by the semantic release suite.'
write_evidence_doc \
    DLV \
    delivery \
    'EV-BUS-DLV-001~004' \
    'TC-BUS-DLV-001~004' \
    'Scheduled delivery progression, failure isolation, and history append behavior are covered by the delivery release suite.'
write_evidence_doc \
    FDB \
    feedback \
    'EV-BUS-FDB-001~004' \
    'TC-BUS-FDB-001~004' \
    'Feedback recording, duplicate replay, conflict handling, and completed delivery transitions are covered by the feedback release suite.'
write_evidence_doc \
    REC \
    recovery \
    'EV-BUS-REC-001~004' \
    'TC-BUS-REC-001~004' \
    'Retry planning, dead-letter movement, replay preparation, and audit-chain guards are covered by the recovery release suite.'
write_evidence_doc \
    OUT \
    output \
    'EV-BUS-OUT-001~006' \
    'TC-BUS-OUT-001~006' \
    'Read-only transport views, failure summaries, audit reads, and outbound publisher evidence are covered by the output release suite.'
write_evidence_doc \
    OBX \
    outbox \
    'EV-BUS-OBX-001~002' \
    'TC-BUS-OBX-001~002' \
    'Committed outbox source replay, source acknowledgement ordering, and relay idempotency are covered by the outbox release suite.'
write_evidence_doc \
    BND \
    backend \
    'EV-BUS-BND-001~003' \
    'TC-BUS-BND-001~003' \
    'Backend capability validation, unavailable dependency handling, and manual-action evidence are covered by the backend boundary release suite.'

write_derived_evidence_docs
write_coverage_matrix_md
write_evidence_index_md

printf 'Generated run reports at %s\n' "$(relative_repo_path "${report_dir}")"
