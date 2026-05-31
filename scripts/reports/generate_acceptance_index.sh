#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
source "${SCRIPT_DIR}/../common.sh"

print_help() {
    cat <<'EOF'
Usage: generate_acceptance_index.sh --run-id <run_id> [--report-root <path>]

Generate the acceptance index for a fixed run id.

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
handoff_json="${acceptance_dir}/${run_id}-handoff.json"
veto_json="${acceptance_dir}/${run_id}-veto.json"
index_file="${acceptance_dir}/${run_id}-index.md"

[[ -d "${run_report_dir}" ]] || die "run report directory does not exist: ${run_report_dir}"
ensure_directory "${acceptance_dir}"
ensure_directory "${review_dir}"
ensure_file "${handoff_json}"
ensure_file "${veto_json}"
ensure_file "${acceptance_dir}/handoff.md"
ensure_file "${acceptance_dir}/veto-checklist.md"
ensure_file "${acceptance_dir}/risk-acceptance.md"
ensure_file "${acceptance_dir}/open-issues.md"
ensure_file "${review_dir}/${run_id}-agent-review.md"

final_conclusion=$(jq -r '.final_conclusion' "${handoff_json}")
signoff_readiness=$(jq -r '.signoff_readiness' "${handoff_json}")
veto_status=$(jq -r '.overall_status' "${veto_json}")

write_text_file "${index_file}" \
    "# Acceptance Index" \
    "" \
    "- Run ID: ${run_id}" \
    "- Final Conclusion: ${final_conclusion}" \
    "- Signoff Readiness: ${signoff_readiness}" \
    "- Veto Review Status: ${veto_status}" \
    "- Run Summary: reports/runs/${run_id}/summary.md" \
    "- Gate Results: reports/runs/${run_id}/gate-results.md" \
    "- Coverage Matrix: reports/runs/${run_id}/coverage-matrix.md" \
    "- Evidence Index: reports/runs/${run_id}/evidence-index.md" \
    "- Performance Baseline: reports/runs/${run_id}/performance-baseline.md" \
    "- Config Summary: reports/runs/${run_id}/config-summary.md" \
    "- Redaction Report: reports/runs/${run_id}/redaction-check.md" \
    "- Artifact Index: reports/runs/${run_id}/artifact-index.md" \
    "- Acceptance Handoff: reports/acceptance/handoff.md" \
    "- Veto Checklist: reports/acceptance/veto-checklist.md" \
    "- Risk Acceptance: reports/acceptance/risk-acceptance.md" \
    "- Open Issues: reports/acceptance/open-issues.md" \
    "- Agent Review: reports/review/${run_id}-agent-review.md"

printf 'Generated acceptance index at %s\n' "${index_file}"
