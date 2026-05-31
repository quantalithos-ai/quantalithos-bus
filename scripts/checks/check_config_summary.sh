#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
source "${SCRIPT_DIR}/../common.sh"

print_help() {
    cat <<'EOF'
Usage: check_config_summary.sh --run-id <run_id> [--report-root <path>]

Validate that config-summary.md records the config profile, runtime graph, and
release-gate control outcomes.

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
summary_file="${report_root}/runs/${run_id}/config-summary.md"

ensure_file "${summary_file}"
grep -q 'Config Profile:' "${summary_file}" || die "config summary must record Config Profile"
grep -q 'Runtime Graph:' "${summary_file}" || die "config summary must record Runtime Graph"
grep -q 'Secret Policy:' "${summary_file}" || die "config summary must record Secret Policy"
grep -q 'Redaction Policy:' "${summary_file}" || die "config summary must record Redaction Policy"
grep -q 'Reload Request:' "${summary_file}" || die "config summary must record Reload Request"

printf 'Config summary check passed for run %s\n' "${run_id}"
