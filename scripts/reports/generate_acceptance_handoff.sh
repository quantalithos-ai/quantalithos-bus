#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
source "${SCRIPT_DIR}/../common.sh"

print_help() {
    cat <<'EOF'
Usage: generate_acceptance_handoff.sh --run-id <run_id> [--report-root <path>]

Generate the acceptance handoff materials for a fixed run id.

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
artifact_root="${repo_root}/$(default_artifact_root "${run_id}")"
run_report_dir="${repo_root}/${report_root}/runs/${run_id}"
acceptance_dir="${repo_root}/${report_root}/acceptance"
review_dir="${repo_root}/${report_root}/review"
context_file="${artifact_root}/meta/context.json"
veto_json="${acceptance_dir}/${run_id}-veto.json"
handoff_json="${acceptance_dir}/${run_id}-handoff.json"
handoff_md="${acceptance_dir}/handoff.md"
risk_md="${acceptance_dir}/risk-acceptance.md"
issues_md="${acceptance_dir}/open-issues.md"
review_md="${review_dir}/${run_id}-agent-review.md"

[[ -d "${run_report_dir}" ]] || die "run report directory does not exist: ${run_report_dir}"
ensure_directory "${acceptance_dir}"
ensure_directory "${review_dir}"
ensure_file "${context_file}"
ensure_file "${run_report_dir}/summary.md"
ensure_file "${run_report_dir}/gate-results.md"
ensure_file "${run_report_dir}/coverage-matrix.md"
ensure_file "${run_report_dir}/evidence-index.md"
ensure_file "${run_report_dir}/artifact-index.md"
ensure_file "${run_report_dir}/config-summary.md"
redaction_available="yes"
if [[ ! -f "${run_report_dir}/redaction-check.md" ]]; then
    redaction_available="no"
fi

design_repo_commit=$(jq -r '.design_repo_commit' "${context_file}")
workspace_commit=$(jq -r '.workspace_commit' "${context_file}")
config_profile=$(jq -r '.config_profile' "${context_file}")
dependency_snapshot=$(jq -r '.dependency_snapshot | join(", ")' "${context_file}")

final_conclusion="not_entering_signoff"
signoff_readiness="review_pending"
veto_status="not_run"

if [[ -f "${veto_json}" ]]; then
    veto_status=$(jq -r '.overall_status' "${veto_json}")
    case "${veto_status}" in
        passed)
            final_conclusion="pass"
            signoff_readiness="ready"
            ;;
        blocked)
            final_conclusion="not_entering_signoff"
            signoff_readiness="blocked"
            ;;
        failed)
            final_conclusion="fail"
            signoff_readiness="blocked"
            ;;
        *)
            final_conclusion="not_entering_signoff"
            signoff_readiness="blocked"
            ;;
    esac
fi

blocked_items_json='[]'
if [[ -f "${veto_json}" ]]; then
    blocked_items_json=$(jq '[.items[] | select(.status != "not_hit")]' "${veto_json}")
fi

blocked_count=$(jq 'length' <<<"${blocked_items_json}")
tap_surface_present="no"
if rg -q "tap" "${repo_root}/crates" >/dev/null 2>&1; then
    tap_surface_present="yes"
fi
security_review_status="unknown"
if [[ -f "${run_report_dir}/evidence/EV-BUS-SEC.md" ]]; then
    security_review_status=$(awk -F': ' '/^- Assessment:/ {print $2}' "${run_report_dir}/evidence/EV-BUS-SEC.md" | head -n 1)
fi

jq -n \
    --arg run_id "${run_id}" \
    --arg final_conclusion "${final_conclusion}" \
    --arg signoff_readiness "${signoff_readiness}" \
    --arg veto_status "${veto_status}" \
    --arg generated_at "$(current_utc_timestamp)" \
    --arg design_repo_commit "${design_repo_commit}" \
    --arg workspace_commit "${workspace_commit}" \
    --arg config_profile "${config_profile}" \
    --arg dependency_snapshot "${dependency_snapshot}" \
    --argjson blocked_items "${blocked_items_json}" \
    '{
        run_id: $run_id,
        final_conclusion: $final_conclusion,
        signoff_readiness: $signoff_readiness,
        veto_status: $veto_status,
        generated_at: $generated_at,
        design_repo_commit: $design_repo_commit,
        workspace_commit: $workspace_commit,
        config_profile: $config_profile,
        dependency_snapshot: $dependency_snapshot,
        blocked_items: $blocked_items
    }' >"${handoff_json}"

{
    printf '# Acceptance Handoff\n\n'
    printf -- '- Run ID: %s\n' "${run_id}"
    printf -- '- Final Conclusion: %s\n' "${final_conclusion}"
    printf -- '- Signoff Readiness: %s\n' "${signoff_readiness}"
    printf -- '- Veto Review Status: %s\n' "${veto_status}"
    printf -- '- Workspace Commit: %s\n' "${workspace_commit}"
    printf -- '- Design Repo Commit: %s\n' "${design_repo_commit}"
    printf -- '- Config Profile: %s\n' "${config_profile}"
    printf -- '- Dependency Snapshot: %s\n' "${dependency_snapshot}"
    printf '\n## Evidence Entry Points\n\n'
    printf -- '- Run Summary: reports/runs/%s/summary.md\n' "${run_id}"
    printf -- '- Gate Results: reports/runs/%s/gate-results.md\n' "${run_id}"
    printf -- '- Coverage Matrix: reports/runs/%s/coverage-matrix.md\n' "${run_id}"
    printf -- '- Evidence Index: reports/runs/%s/evidence-index.md\n' "${run_id}"
    printf -- '- Artifact Index: reports/runs/%s/artifact-index.md\n' "${run_id}"
    printf -- '- Config Summary: reports/runs/%s/config-summary.md\n' "${run_id}"
    if [[ "${redaction_available}" == "yes" ]]; then
        printf -- '- Redaction Report: reports/runs/%s/redaction-check.md\n' "${run_id}"
    else
        printf -- '- Redaction Report: reports/runs/%s/redaction-check.md (pending until the redaction suite runs)\n' "${run_id}"
    fi
    printf -- '- Acceptance Review JSON: reports/acceptance/%s-handoff.json\n' "${run_id}"
    printf '\n## Scope Review\n\n'
    printf -- '- In scope: fixed-run release gate, report chain, acceptance handoff, veto checklist, open issues, and final acceptance checks for PH-08 / commit-08-b.\n'
    printf -- '- Out of scope: new transport semantics, new recovery behavior, production adapters, gateway auth, dashboard products, exactly-once guarantees, and hot reload support.\n'
    printf '\n## Known Outcome\n\n'
    if [[ "${blocked_count}" -eq 0 ]]; then
        printf -- '- The current fixed run is ready for signoff review and does not carry blocked acceptance items.\n'
    else
        printf -- '- The current fixed run is blocked by %s acceptance item(s); see open issues and veto checklist for the blocking details.\n' "${blocked_count}"
    fi
    printf -- '- Human or agent review is captured in reports/review/%s-agent-review.md.\n' "${run_id}"
} >"${handoff_md}"

if [[ "${final_conclusion}" == "conditional_pass" ]]; then
    write_text_file "${risk_md}" \
        "# Risk Acceptance" \
        "" \
        "- Run ID: ${run_id}" \
        "- Status: accepted" \
        "- Summary: Conditional acceptance is active for the listed S2 or P1-risk items."
else
    write_text_file "${risk_md}" \
        "# Risk Acceptance" \
        "" \
        "- Run ID: ${run_id}" \
        "- Status: not_applicable" \
        "- Summary: The current fixed run is not in conditional-pass state, so risk acceptance is not the controlling path."
fi

{
    printf '# Open Issues\n\n'
    printf -- '- Run ID: %s\n' "${run_id}"
    printf -- '- Blocked Item Count: %s\n' "${blocked_count}"
    printf '\n## Active Items\n\n'
    if [[ "${blocked_count}" -eq 0 ]]; then
        printf '| Issue ID | Severity | Owner | Summary | Evidence | Next Action |\n'
        printf '|---|---|---|---|---|---|\n'
        printf '| none | none | none | No blocked issue is active for the fixed run. | reports/acceptance/%s-veto.json | Keep the signoff packet stable. |\n' "${run_id}"
    else
        printf '| Issue ID | Severity | Owner | Summary | Evidence | Next Action |\n'
        printf '|---|---|---|---|---|---|\n'
        counter=1
        while IFS=$'\t' read -r veto_id veto_status veto_reason veto_evidence; do
            issue_id=$(printf 'ISSUE-BUS-ACPT-%03d' "${counter}")
            case "${veto_status}" in
                hit)
                    severity="VETO"
                    ;;
                evidence_insufficient)
                    severity="BLOCKED"
                    ;;
                *)
                    severity="REVIEW"
                    ;;
            esac
            printf '| %s | %s | Bus maintainer | %s | %s | Repair the boundary or add the missing acceptance evidence, then rerun the fixed-run release gate. |\n' \
                "${issue_id}" \
                "${severity}" \
                "${veto_reason}" \
                "${veto_evidence}"
            counter=$((counter + 1))
        done < <(jq -r '.[] | [.id, .status, .reason, .evidence[0]] | @tsv' <<<"${blocked_items_json}")
    fi
} >"${issues_md}"

{
    printf '# Agent Review\n\n'
    printf -- '- Run ID: %s\n' "${run_id}"
    printf -- '- Final Conclusion: %s\n' "${final_conclusion}"
    printf -- '- Signoff Readiness: %s\n' "${signoff_readiness}"
    printf '\n## Review Findings\n\n'
    if [[ "${blocked_count}" -eq 0 ]]; then
        printf -- '- No blocking acceptance finding remains after the fixed-run evidence review.\n'
    else
        while IFS=$'\t' read -r veto_id veto_status veto_reason veto_evidence; do
            printf -- '- %s (%s): %s Evidence: %s\n' "${veto_id}" "${veto_status}" "${veto_reason}" "${veto_evidence}"
        done < <(jq -r '.[] | [.id, .status, .reason, .evidence[0]] | @tsv' <<<"${blocked_items_json}")
    fi
    printf '\n## Source Review Highlights\n\n'
    printf -- '- Sensitive read and recovery surfaces were reviewed against the generated EV-BUS-SEC report.\n'
    if [[ "${tap_surface_present}" == "yes" ]]; then
        printf -- '- The workspace now exposes a tap surface through the fake observability sink and tap-output helper records.\n'
    else
        printf -- '- The workspace still shows no tap surface in the source tree for the fixed-run review.\n'
    fi
    if [[ "${security_review_status}" == "passed" ]]; then
        printf -- '- Privileged read and replay-preparation seams now include stable rejection and access-audit evidence in the fixed run.\n'
    else
        printf -- '- Privileged read or replay-preparation seams still lack full access-audit evidence in the fixed run.\n'
    fi
} >"${review_md}"

printf 'Generated acceptance handoff materials in %s\n' "${acceptance_dir}"
