#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
source "${SCRIPT_DIR}/../common.sh"

print_help() {
    cat <<'EOF'
Usage: generate_acceptance_index.sh --run-id <run_id> [--report-root <path>]

Generate the acceptance index skeleton for a fixed run id.

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

ensure_run_id "${run_id}"
ensure_report_root_shape "${report_root}"
run_report_dir="${report_root}/runs/${run_id}"
[[ -d "${run_report_dir}" ]] || die "run report directory does not exist: ${run_report_dir}"

acceptance_dir="${report_root}/acceptance"
ensure_directory "${acceptance_dir}"
write_text_file "${acceptance_dir}/${run_id}-index.md" \
    "# Acceptance Index" \
    "" \
    "- Run ID: ${run_id}" \
    "- Summary: ../runs/${run_id}/summary.md" \
    "- Evidence Index: ../runs/${run_id}/evidence-index.md" \
    "- Artifact Index: ../runs/${run_id}/artifact-index.md"

printf 'Generated acceptance index at %s\n' "${acceptance_dir}/${run_id}-index.md"
