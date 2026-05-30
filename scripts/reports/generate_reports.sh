#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
source "${SCRIPT_DIR}/../common.sh"

print_help() {
    cat <<'EOF'
Usage: generate_reports.sh --run-id <run_id> [--artifact-root <path>] [--report-root <path>]

Generate the run report skeleton for a fixed run id.

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

ensure_run_id "${run_id}"
[[ -n "${artifact_root}" ]] || artifact_root=$(default_artifact_root "${run_id}")
ensure_artifact_root_shape "${artifact_root}"
ensure_report_root_shape "${report_root}"
[[ -d "${artifact_root}" ]] || die "artifact root does not exist: ${artifact_root}"

report_dir="${report_root}/runs/${run_id}"
ensure_directory "${report_dir}/suites"
ensure_directory "${report_dir}/evidence"

write_text_file "${report_dir}/summary.md" \
    "# Run Summary" \
    "" \
    "- Run ID: ${run_id}" \
    "- Artifact Root: ${artifact_root}" \
    "- Status: draft"

write_text_file "${report_dir}/evidence-index.md" \
    "# Evidence Index" \
    "" \
    "- Run ID: ${run_id}" \
    "- Source Artifact Root: ${artifact_root}" \
    "- Status: draft"

write_text_file "${report_dir}/gate-results.md" \
    "# Gate Results" \
    "" \
    "- Run ID: ${run_id}" \
    "- Result: draft"

write_text_file "${report_dir}/coverage-matrix.md" \
    "# Coverage Matrix" \
    "" \
    "- Run ID: ${run_id}" \
    "- Status: draft"

write_text_file "${report_dir}/config-summary.md" \
    "# Config Summary" \
    "" \
    "- Run ID: ${run_id}" \
    "- Config Profile: draft" \
    "- Redaction Policy: enforced"

write_text_file "${report_dir}/redaction-check.md" \
    "# Redaction Check" \
    "" \
    "- Run ID: ${run_id}" \
    "- Status: pending"

write_text_file "${report_dir}/artifact-index.md" \
    "# Artifact Index" \
    "" \
    "- Artifact Root: ${artifact_root}" \
    "- Status: draft"

write_text_file "${report_dir}/suites/bootstrap.md" \
    "# Bootstrap Suite" \
    "" \
    "- Run ID: ${run_id}" \
    "- Status: draft"

write_text_file "${report_dir}/suites/config.md" \
    "# Config Suite" \
    "" \
    "- Run ID: ${run_id}" \
    "- Status: draft"

write_text_file "${report_dir}/evidence/EV-BUS-CFG.md" \
    "# EV-BUS-CFG" \
    "" \
    "- Run ID: ${run_id}" \
    "- Status: draft"

printf 'Generated report skeleton at %s\n' "${report_dir}"
