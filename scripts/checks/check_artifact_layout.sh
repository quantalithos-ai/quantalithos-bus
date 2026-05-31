#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
source "${SCRIPT_DIR}/../common.sh"

print_help() {
    cat <<'EOF'
Usage: check_artifact_layout.sh --artifact-root <path>

Validate that release-gate artifacts follow artifacts/test/<run_id> without
latest or an extra project layer, and that the expected evidence files exist.

Options:
  --artifact-root <path>       Artifact root to validate.
  --help                       Show this help text.
EOF
}

artifact_root=""

while [[ $# -gt 0 ]]; do
    case "$1" in
        --artifact-root)
            artifact_root=${2:-}
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

if [[ -d "artifacts/test/latest" ]]; then
    die "artifacts/test/latest is not allowed"
fi

if [[ -d "artifacts/test/quantalithos-bus" ]]; then
    die "artifacts/test/quantalithos-bus introduces a forbidden project layer"
fi

[[ -d "${artifact_root}" ]] || die "artifact root does not exist: ${artifact_root}"
ensure_file "${artifact_root}/meta/context.json"
ensure_file "${artifact_root}/meta/gate-results.json"
ensure_file "${artifact_root}/evidence-index.json"
ensure_file "${artifact_root}/fixtures/fixture-summary.json"

for suite_name in publication semantic delivery feedback output outbox backend recovery config redaction report; do
    ensure_file "${artifact_root}/suites/${suite_name}/report.json"
    ensure_file "${artifact_root}/suites/${suite_name}/stdout.log"
    ensure_file "${artifact_root}/suites/${suite_name}/stderr.log"
done

printf 'Artifact layout check passed for %s\n' "${artifact_root}"
