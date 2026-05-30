#!/usr/bin/env bash
set -euo pipefail

die() {
    printf 'Error: %s\n' "$*" >&2
    exit 1
}

default_artifact_root() {
    local run_id=${1:?run_id is required}
    printf 'artifacts/test/%s' "${run_id}"
}

ensure_run_id() {
    local run_id=${1:-}

    [[ -n "${run_id}" ]] || die "run id is required"
    [[ "${run_id}" != "latest" ]] || die "run id must not be latest"
    [[ "${run_id}" != */* ]] || die "run id must not contain path separators"
}

ensure_artifact_root_shape() {
    local artifact_root=${1:-}

    [[ -n "${artifact_root}" ]] || die "artifact root is required"
    [[ "${artifact_root}" == artifacts/test/* || "${artifact_root}" == */artifacts/test/* ]] || die "artifact root must use artifacts/test/<run_id>"
    [[ "${artifact_root}" != *"/latest"* && "${artifact_root}" != "artifacts/test/latest" ]] || die "artifact root must not use latest"
    [[ "${artifact_root}" != *"artifacts/test/quantalithos-bus/"* ]] || die "artifact root must not add a project layer"
}

ensure_report_root_shape() {
    local report_root=${1:-}

    [[ -n "${report_root}" ]] || die "report root is required"
    [[ "${report_root}" == reports || "${report_root}" == */reports ]] || die "report root must be reports"
    [[ "${report_root}" != *"/latest"* && "${report_root}" != "reports/latest" ]] || die "report root must not use latest"
    [[ "${report_root}" != *"/reports/quantalithos-bus"* && "${report_root}" != "reports/quantalithos-bus"* ]] || die "report root must not add a project layer"
}

extract_run_id_from_artifact_root() {
    local artifact_root=${1:?artifact root is required}
    basename "${artifact_root}"
}

ensure_directory() {
    local dir_path=${1:?directory path is required}
    mkdir -p "${dir_path}"
}

scan_directory_for_pattern() {
    local target_dir=${1:?target directory is required}
    local pattern=${2:?pattern is required}

    if [[ ! -d "${target_dir}" ]]; then
        return 1
    fi

    if command -v rg >/dev/null 2>&1; then
        rg -n --hidden --glob '!README.md' "${pattern}" "${target_dir}"
    else
        grep -RIn --exclude='README.md' "${pattern}" "${target_dir}"
    fi
}

write_text_file() {
    local target_file=${1:?target file is required}
    shift

    ensure_directory "$(dirname "${target_file}")"
    printf '%s\n' "$@" > "${target_file}"
}
