#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
source "${SCRIPT_DIR}/../common.sh"

print_help() {
    cat <<'EOF'
Usage: check_report_links.sh --artifact-root <path> [--report-root <path>]

Validate that run reports avoid latest and forbidden project-layer links, and
that referenced report and artifact paths exist.

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
repo_root=$(repo_root)
run_id=$(extract_run_id_from_artifact_root "${artifact_root}")
run_report_dir="${repo_root}/${report_root}/runs/${run_id}"
acceptance_index="${repo_root}/${report_root}/acceptance/${run_id}-index.md"
review_file="${repo_root}/${report_root}/review/${run_id}-agent-review.md"

[[ -d "${run_report_dir}" ]] || die "run report directory does not exist: ${run_report_dir}"
ensure_file "${run_report_dir}/artifact-index.md"
ensure_file "${acceptance_index}"
ensure_file "${repo_root}/${report_root}/acceptance/handoff.md"
ensure_file "${repo_root}/${report_root}/acceptance/veto-checklist.md"
ensure_file "${repo_root}/${report_root}/acceptance/risk-acceptance.md"
ensure_file "${repo_root}/${report_root}/acceptance/open-issues.md"
ensure_file "${review_file}"

if ! grep -q "${artifact_root}" "${run_report_dir}/artifact-index.md"; then
    die "artifact-index.md does not point to ${artifact_root}"
fi

for forbidden in 'artifacts/test/latest' 'reports/runs/latest' 'artifacts/test/quantalithos-bus/' 'reports/quantalithos-bus/'; do
    if scan_directory_for_pattern "${run_report_dir}" "${forbidden}" >/dev/null 2>&1; then
        die "forbidden report link detected: ${forbidden}"
    fi

    if scan_directory_for_pattern "${repo_root}/${report_root}/acceptance" "${forbidden}" >/dev/null 2>&1; then
        die "forbidden acceptance link detected: ${forbidden}"
    fi

    if scan_directory_for_pattern "${repo_root}/${report_root}/review" "${forbidden}" >/dev/null 2>&1; then
        die "forbidden review link detected: ${forbidden}"
    fi
done

mapfile -t referenced_paths < <(
    {
        rg --no-filename -o 'artifacts/test/[A-Za-z0-9TZ._:/-]+' "${run_report_dir}" "${repo_root}/${report_root}/acceptance" "${review_file}" 2>/dev/null || true
        rg --no-filename -o "reports/runs/${run_id}/[A-Za-z0-9TZ._:/-]+" "${run_report_dir}" "${repo_root}/${report_root}/acceptance" "${review_file}" 2>/dev/null || true
        rg --no-filename -o "reports/acceptance/[A-Za-z0-9TZ._:/-]+" "${run_report_dir}" "${repo_root}/${report_root}/acceptance" "${review_file}" 2>/dev/null || true
        rg --no-filename -o "reports/review/[A-Za-z0-9TZ._:/-]+" "${run_report_dir}" "${repo_root}/${report_root}/acceptance" "${review_file}" 2>/dev/null || true
    } | sort -u
)

for referenced_path in "${referenced_paths[@]}"; do
    cleaned_path=${referenced_path%%[).,]}
    [[ -e "${cleaned_path}" ]] || die "referenced report path is missing: ${cleaned_path}"
done

printf 'Report link check passed for run %s\n' "${run_id}"
