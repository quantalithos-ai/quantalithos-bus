#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
source "${SCRIPT_DIR}/../common.sh"

print_help() {
    cat <<'EOF'
Usage: run_ci_gate.sh --run-id <run_id> [--artifact-root <path>] [--config-profile <profile>]

Run the main CI gate skeleton for the L0-bus workspace.

Options:
  --run-id <run_id>            Fixed run identifier.
  --artifact-root <path>       Artifact root. Defaults to artifacts/test/<run_id>.
  --config-profile <profile>   Config profile. Defaults to integration-test.
  --help                       Show this help text.
EOF
}

run_id=""
artifact_root=""
config_profile="integration-test"

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
        --config-profile)
            config_profile=${2:-}
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
[[ -n "${config_profile}" ]] || die "config profile is required"

printf 'CI gate skeleton\n'
printf 'run_id=%s\n' "${run_id}"
printf 'artifact_root=%s\n' "${artifact_root}"
printf 'config_profile=%s\n' "${config_profile}"
printf 'suites=bus-integration-full,bus-worker-consumer,bus-job-runner,bus-report-smoke\n'
