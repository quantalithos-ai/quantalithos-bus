#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
source "${SCRIPT_DIR}/../common.sh"

print_help() {
    cat <<'EOF'
Usage: check_redaction.sh --artifact-root <path> [--report-root <path>]

Scan artifacts and reports for obvious forbidden-body markers.

Options:
  --artifact-root <path>       Artifact root for the fixed run id.
  --report-root <path>         Report root. Defaults to reports.
  --help                       Show this help text.
EOF
}

artifact_root=""
report_root="reports"

while [[ $# -gt 0 ]]; do
    case "$1" in
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

ensure_artifact_root_shape "${artifact_root}"
ensure_report_root_shape "${report_root}"
run_id=$(extract_run_id_from_artifact_root "${artifact_root}")
run_report_dir="${report_root}/runs/${run_id}"
redaction_report="${run_report_dir}/redaction-check.md"
ensure_directory "${run_report_dir}"

forbidden_patterns=(
    'BEGIN PRIVATE KEY'
    'payload body'
    'raw secret'
    'backend private body'
    'governance decision body'
)

for pattern in "${forbidden_patterns[@]}"; do
    if scan_directory_for_pattern "${artifact_root}" "${pattern}" >/dev/null 2>&1; then
        write_text_file "${redaction_report}" \
            "# Redaction Check" \
            "" \
            "- Run ID: ${run_id}" \
            "- Status: failed" \
            "- Matched Pattern: ${pattern}"
        die "forbidden content detected in artifacts: ${pattern}"
    fi

    if scan_directory_for_pattern "${run_report_dir}" "${pattern}" >/dev/null 2>&1; then
        write_text_file "${redaction_report}" \
            "# Redaction Check" \
            "" \
            "- Run ID: ${run_id}" \
            "- Status: failed" \
            "- Matched Pattern: ${pattern}"
        die "forbidden content detected in run reports: ${pattern}"
    fi

    if scan_directory_for_pattern "${report_root}/acceptance" "${pattern}" >/dev/null 2>&1; then
        write_text_file "${redaction_report}" \
            "# Redaction Check" \
            "" \
            "- Run ID: ${run_id}" \
            "- Status: failed" \
            "- Matched Pattern: ${pattern}"
        die "forbidden content detected in acceptance reports: ${pattern}"
    fi
done

write_text_file "${redaction_report}" \
    "# Redaction Check" \
    "" \
    "- Run ID: ${run_id}" \
    "- Status: passed" \
    "- Scope: artifacts + run reports + acceptance reports"

printf 'Redaction check passed for run %s\n' "${run_id}"
