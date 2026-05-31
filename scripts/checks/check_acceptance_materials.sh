#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
source "${SCRIPT_DIR}/../common.sh"

print_help() {
    cat <<'EOF'
Usage: check_acceptance_materials.sh --run-id <run_id> [--report-root <path>]

Validate the fixed-run acceptance materials, veto review, and final signoff
status for commit-08-b.

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
acceptance_dir="${repo_root}/${report_root}/acceptance"
review_dir="${repo_root}/${report_root}/review"
handoff_json="${acceptance_dir}/${run_id}-handoff.json"
veto_json="${acceptance_dir}/${run_id}-veto.json"
index_file="${acceptance_dir}/${run_id}-index.md"
review_file="${review_dir}/${run_id}-agent-review.md"

for required_file in \
    "${handoff_json}" \
    "${veto_json}" \
    "${index_file}" \
    "${acceptance_dir}/handoff.md" \
    "${acceptance_dir}/veto-checklist.md" \
    "${acceptance_dir}/risk-acceptance.md" \
    "${acceptance_dir}/open-issues.md" \
    "${review_file}"; do
    ensure_file "${required_file}"
done

for forbidden_marker in 'pending review' 'deferred to commit-08-b'; do
    if rg -n --hidden --glob '!README.md' "${forbidden_marker}" \
        "${index_file}" \
        "${acceptance_dir}/handoff.md" \
        "${acceptance_dir}/veto-checklist.md" \
        "${acceptance_dir}/risk-acceptance.md" \
        "${acceptance_dir}/open-issues.md" >/dev/null 2>&1; then
        die "acceptance reports still contain placeholder marker: ${forbidden_marker}"
    fi
done

item_count=$(jq '.items | length' "${veto_json}")
[[ "${item_count}" -eq 12 ]] || die "veto review must contain 12 items"

missing_ids=$(
    jq -r '
        ["VETO-BUS-001","VETO-BUS-002","VETO-BUS-003","VETO-BUS-004","VETO-BUS-005","VETO-BUS-006","VETO-BUS-007","VETO-BUS-008","VETO-BUS-009","VETO-BUS-010","VETO-BUS-011","VETO-BUS-012"]
        - (.items | map(.id))
        | join(",")
    ' "${veto_json}"
)
[[ -z "${missing_ids}" ]] || die "veto review is missing ids: ${missing_ids}"

jq -e '.items | all(.status == "not_hit")' "${veto_json}" >/dev/null \
    || die "one or more veto items are not cleared for the fixed run"

handoff_conclusion=$(jq -r '.final_conclusion' "${handoff_json}")
handoff_readiness=$(jq -r '.signoff_readiness' "${handoff_json}")

[[ "${handoff_conclusion}" == "pass" || "${handoff_conclusion}" == "conditional_pass" ]] \
    || die "acceptance handoff is not in a releasable conclusion state: ${handoff_conclusion}"
[[ "${handoff_readiness}" == "ready" ]] \
    || die "acceptance handoff is not ready for signoff: ${handoff_readiness}"

if [[ "${handoff_conclusion}" == "conditional_pass" ]]; then
    grep -q 'Status: accepted' "${acceptance_dir}/risk-acceptance.md" \
        || die "conditional pass requires accepted risk-acceptance content"
fi

printf 'Acceptance materials check passed for run %s\n' "${run_id}"
