#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
source "${SCRIPT_DIR}/../common.sh"

print_help() {
    cat <<'EOF'
Usage: generate_veto_checklist.sh --run-id <run_id> [--report-root <path>]

Generate the veto checklist for a fixed run id.

Options:
  --run-id <run_id>            Fixed run identifier.
  --report-root <path>         Report root. Defaults to reports.
  --help                       Show this help text.
EOF
}

run_id=""
report_root="reports"

while [[ $# -gt 0 ]]; do
    case "$1" in
        --run-id)
            run_id=${2:-}
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
ensure_report_root_shape "${report_root}"

repo_root=$(repo_root)
run_report_dir="${repo_root}/${report_root}/runs/${run_id}"
acceptance_dir="${repo_root}/${report_root}/acceptance"
review_dir="${repo_root}/${report_root}/review"
artifact_root="${repo_root}/$(default_artifact_root "${run_id}")"
context_file="${artifact_root}/meta/context.json"
json_output="${acceptance_dir}/${run_id}-veto.json"
markdown_output="${acceptance_dir}/veto-checklist.md"

[[ -d "${run_report_dir}" ]] || die "run report directory does not exist: ${run_report_dir}"
ensure_directory "${acceptance_dir}"
ensure_directory "${review_dir}"
ensure_file "${context_file}"

suite_status() {
    local suite_name=${1:?suite name is required}
    local report_file="${artifact_root}/suites/${suite_name}/report.json"
    ensure_file "${report_file}"
    jq -r '.status' "${report_file}"
}

doc_assessment() {
    local family=${1:?family is required}
    local evidence_doc="${run_report_dir}/evidence/EV-BUS-${family}.md"
    if [[ ! -f "${evidence_doc}" ]]; then
        printf 'missing\n'
        return 0
    fi
    awk -F': ' '/^- Assessment:/ {print $2}' "${evidence_doc}" | head -n 1
}

redaction_status() {
    local report_file="${run_report_dir}/redaction-check.md"
    if [[ ! -f "${report_file}" ]]; then
        printf 'missing\n'
        return 0
    fi
    awk -F': ' '/^- Status:/ {print $2}' "${report_file}" | head -n 1
}

has_tap_surface() {
    rg -q "tap" "${repo_root}/crates" >/dev/null 2>&1
}

core_contracts_path_dependency() {
    rg -n 'core-contracts\s*=\s*\{ path = "\.\./quantalithos-core/crates/contracts" \}' "${repo_root}/Cargo.toml" >/dev/null 2>&1
}

status_json() {
    local status=${1:?status is required}
    local reason=${2:?reason is required}
    local evidence=${3:?evidence is required}

    jq -n \
        --arg status "${status}" \
        --arg reason "${reason}" \
        --arg evidence "${evidence}" \
        '{status: $status, reason: $reason, evidence: [$evidence]}'
}

veto_001() {
    if has_tap_surface; then
        status_json \
            "not_hit" \
            "The workspace contains a tap surface and the closed-loop reports cover read-output evidence." \
            "reports/runs/${run_id}/evidence/EV-BUS-OUT.md"
    else
        status_json \
            "hit" \
            "No tap surface was found in the workspace source tree, so the P0 read-output closure remains incomplete." \
            "reports/runs/${run_id}/evidence/EV-BUS-OUT.md"
    fi
}

veto_002() {
    if core_contracts_path_dependency && jq -e '.dependency_snapshot | length > 0' "${context_file}" >/dev/null; then
        status_json \
            "not_hit" \
            "The workspace still depends on the local core-contracts path snapshot and does not redefine the shared contract boundary in reports." \
            "artifacts/test/${run_id}/meta/context.json"
    else
        status_json \
            "hit" \
            "The workspace no longer shows the required local core-contracts path dependency snapshot." \
            "artifacts/test/${run_id}/meta/context.json"
    fi
}

veto_003() {
    case "$(redaction_status)" in
        passed)
            status_json \
                "not_hit" \
                "Artifact, run-report, acceptance, and review documents passed the fixed-run redaction scan." \
                "reports/runs/${run_id}/redaction-check.md"
            ;;
        failed)
            status_json \
                "hit" \
                "The fixed-run redaction report did not pass, so the boundary scan cannot clear the sensitive-data redline." \
                "reports/runs/${run_id}/redaction-check.md"
            ;;
        *)
            status_json \
                "evidence_insufficient" \
                "The fixed-run redaction report has not been produced yet for the current review step." \
                "reports/runs/${run_id}/redaction-check.md"
            ;;
    esac
}

veto_004() {
    if [[ "$(doc_assessment CONS)" == "passed" && "$(doc_assessment OBS)" == "passed" ]]; then
        status_json \
            "not_hit" \
            "Consistency and observability review reports both confirm append-only audit and history coverage for delivery, feedback, and recovery chains." \
            "reports/runs/${run_id}/evidence/EV-BUS-CONS.md"
    else
        status_json \
            "evidence_insufficient" \
            "Traceability evidence for delivery, feedback, or recovery remains incomplete at the acceptance layer." \
            "reports/runs/${run_id}/evidence/EV-BUS-OBS.md"
    fi
}

veto_005() {
    if [[ "$(suite_status recovery)" == "passed" ]]; then
        status_json \
            "not_hit" \
            "Recovery tests cover dead-letter creation, trusted audit-chain checks, and approval-backed replay readiness." \
            "reports/runs/${run_id}/evidence/EV-BUS-REC.md"
    else
        status_json \
            "hit" \
            "Recovery suite results do not prove the replay guard chain for the fixed run." \
            "reports/runs/${run_id}/evidence/EV-BUS-REC.md"
    fi
}

veto_006() {
    case "$(doc_assessment SEC)" in
        passed)
            status_json \
                "not_hit" \
                "Authorization seam evidence shows stable rejection and access-audit coverage for sensitive read and recovery surfaces." \
                "reports/runs/${run_id}/evidence/EV-BUS-SEC.md"
            ;;
        hit)
            status_json \
                "hit" \
                "Source review found no privileged-read or access-audit enforcement for sensitive failure-summary, audit-trail, or replay-preparation surfaces." \
                "reports/runs/${run_id}/evidence/EV-BUS-SEC.md"
            ;;
        *)
            status_json \
                "evidence_insufficient" \
                "Authorization seam evidence is missing for sensitive read or recovery surfaces." \
                "reports/runs/${run_id}/evidence/EV-BUS-SEC.md"
            ;;
    esac
}

veto_007() {
    if [[ "$(suite_status semantic)" == "passed" && "$(suite_status backend)" == "passed" ]]; then
        status_json \
            "not_hit" \
            "Semantic and backend suites passed the normalized transport-boundary checks for the fixed run." \
            "reports/runs/${run_id}/evidence/EV-BUS-BND.md"
    else
        status_json \
            "hit" \
            "Backend-boundary or semantic normalization suites did not pass for the fixed run." \
            "reports/runs/${run_id}/evidence/EV-BUS-BND.md"
    fi
}

veto_008() {
    if rg -q "governance_decision_ref: None" "${repo_root}/crates/application/tests/output.rs" \
        && rg -q "governance_decision_ref, None" "${repo_root}/crates/api/src/query.rs" \
        && rg -q "governance_decision_ref: None" "${repo_root}/crates/application/src/services/read_output.rs"; then
        status_json \
            "not_hit" \
            "Failure-summary reads keep governance decision references empty and stay within the bus failure-fact boundary." \
            "reports/runs/${run_id}/evidence/EV-BUS-OUT.md"
    else
        status_json \
            "hit" \
            "Current read-output coverage does not prove that failure-summary surfaces remain decision-free." \
            "reports/runs/${run_id}/evidence/EV-BUS-OUT.md"
    fi
}

veto_009() {
    if [[ "$(doc_assessment CONS)" == "passed" ]]; then
        status_json \
            "not_hit" \
            "The consistency review preserves the no-write query boundary for read projections and output material." \
            "reports/runs/${run_id}/evidence/EV-BUS-CONS.md"
    else
        status_json \
            "evidence_insufficient" \
            "No-write evidence for query and projection surfaces is incomplete at the acceptance layer." \
            "reports/runs/${run_id}/evidence/EV-BUS-CONS.md"
    fi
}

veto_010() {
    if [[ -f "${acceptance_dir}/handoff.md" ]] \
        && [[ -f "${acceptance_dir}/risk-acceptance.md" ]] \
        && [[ -f "${acceptance_dir}/open-issues.md" ]] \
        && [[ -f "${run_report_dir}/artifact-index.md" ]] \
        && [[ -f "${run_report_dir}/evidence-index.md" ]]; then
        status_json \
            "not_hit" \
            "Fixed-run report, artifact, and acceptance handoff roots exist and can be linked together for final review." \
            "reports/runs/${run_id}/artifact-index.md"
    else
        status_json \
            "evidence_insufficient" \
            "The fixed-run report and acceptance chain is incomplete before final signoff." \
            "reports/runs/${run_id}/artifact-index.md"
    fi
}

veto_011() {
    if [[ "$(suite_status config)" == "passed" && "$(doc_assessment CFG-FAULT)" == "passed" ]]; then
        status_json \
            "not_hit" \
            "Config runtime and negative-fixture evidence still enforce the boundary policies for the fixed run." \
            "reports/runs/${run_id}/evidence/EV-BUS-CFG-FAULT.md"
    else
        status_json \
            "hit" \
            "Config runtime evidence does not clear the boundary-policy redline for the fixed run." \
            "reports/runs/${run_id}/evidence/EV-BUS-CFG-FAULT.md"
    fi
}

veto_012() {
    if [[ "$(suite_status outbox)" == "passed" ]]; then
        status_json \
            "not_hit" \
            "Committed outbox relay evidence passed duplicate handling and source-ack ordering checks for the fixed run." \
            "reports/runs/${run_id}/evidence/EV-BUS-OBX.md"
    else
        status_json \
            "hit" \
            "Outbox relay evidence does not prove committed-only ingestion for the fixed run." \
            "reports/runs/${run_id}/evidence/EV-BUS-OBX.md"
    fi
}

items_json=$(jq -n \
    --argjson veto001 "$(veto_001)" \
    --argjson veto002 "$(veto_002)" \
    --argjson veto003 "$(veto_003)" \
    --argjson veto004 "$(veto_004)" \
    --argjson veto005 "$(veto_005)" \
    --argjson veto006 "$(veto_006)" \
    --argjson veto007 "$(veto_007)" \
    --argjson veto008 "$(veto_008)" \
    --argjson veto009 "$(veto_009)" \
    --argjson veto010 "$(veto_010)" \
    --argjson veto011 "$(veto_011)" \
    --argjson veto012 "$(veto_012)" \
    '[
        {id: "VETO-BUS-001"} + $veto001,
        {id: "VETO-BUS-002"} + $veto002,
        {id: "VETO-BUS-003"} + $veto003,
        {id: "VETO-BUS-004"} + $veto004,
        {id: "VETO-BUS-005"} + $veto005,
        {id: "VETO-BUS-006"} + $veto006,
        {id: "VETO-BUS-007"} + $veto007,
        {id: "VETO-BUS-008"} + $veto008,
        {id: "VETO-BUS-009"} + $veto009,
        {id: "VETO-BUS-010"} + $veto010,
        {id: "VETO-BUS-011"} + $veto011,
        {id: "VETO-BUS-012"} + $veto012
    ]')

overall_status=$(jq -r '
    if any(.[]; .status == "hit") then
        "failed"
    elif any(.[]; .status == "evidence_insufficient") then
        "blocked"
    else
        "passed"
    end
' <<<"${items_json}")

jq -n \
    --arg run_id "${run_id}" \
    --arg overall_status "${overall_status}" \
    --arg generated_at "$(current_utc_timestamp)" \
    --arg reviewer "Codex" \
    --argjson items "${items_json}" \
    '{
        run_id: $run_id,
        overall_status: $overall_status,
        generated_at: $generated_at,
        reviewer: $reviewer,
        items: $items
    }' >"${json_output}"

status_label() {
    case "$1" in
        not_hit) printf 'Not Hit\n' ;;
        hit) printf 'Hit\n' ;;
        evidence_insufficient) printf 'Evidence Insufficient\n' ;;
        *) printf '%s\n' "$1" ;;
    esac
}

{
    printf '# Veto Checklist\n\n'
    printf -- '- Run ID: %s\n' "${run_id}"
    printf -- '- Reviewer: Codex\n'
    printf -- '- Overall Status: %s\n' "${overall_status}"
    printf -- '- Machine Review: reports/acceptance/%s-veto.json\n' "${run_id}"
    printf '\n## Veto Review\n\n'
    printf '| Veto ID | Status | Reason | Evidence |\n'
    printf '|---|---|---|---|\n'
    while IFS=$'\t' read -r veto_id veto_status veto_reason veto_evidence; do
        printf '| %s | %s | %s | %s |\n' \
            "${veto_id}" \
            "$(status_label "${veto_status}")" \
            "${veto_reason}" \
            "${veto_evidence}"
    done < <(jq -r '.items[] | [.id, .status, .reason, .evidence[0]] | @tsv' "${json_output}")
} >"${markdown_output}"

printf 'Generated veto checklist at %s\n' "${markdown_output}"
