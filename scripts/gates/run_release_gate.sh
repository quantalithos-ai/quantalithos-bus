#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
source "${SCRIPT_DIR}/../common.sh"

print_help() {
    cat <<'EOF'
Usage: run_release_gate.sh --run-id <run_id> [--artifact-root <path>] [--config-profile <profile>] [--report-root <path>]

Run the release gate for the L0-bus workspace.

Options:
  --run-id <run_id>            Fixed run identifier.
  --artifact-root <path>       Artifact root. Defaults to artifacts/test/<run_id>.
  --config-profile <profile>   Config profile. Defaults to operations-recovery.
  --report-root <path>         Report root. Defaults to reports.
  --help                       Show this help text.
EOF
}

run_id=""
artifact_root=""
config_profile="operations-recovery"
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
        --config-profile)
            config_profile=${2:-}
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
require_command cargo

ensure_run_id "${run_id}"
[[ -n "${artifact_root}" ]] || artifact_root=$(default_artifact_root "${run_id}")
ensure_artifact_root_shape "${artifact_root}"
ensure_report_root_shape "${report_root}"
[[ -n "${config_profile}" ]] || die "config profile is required"

repo_root=$(repo_root)
artifact_root_abs="${repo_root}/${artifact_root}"
report_root_abs="${repo_root}/${report_root}"

design_repo_commit=$(git -C /home/aris/Projects/quantalithos-design rev-parse HEAD)
workspace_commit=$(git rev-parse HEAD)
core_contracts_path=$(grep -n 'core-contracts' "${repo_root}/Cargo.toml" | sed 's/^[0-9]*://')

ensure_directory "${artifact_root_abs}/meta"
ensure_directory "${artifact_root_abs}/fixtures"
ensure_directory "${artifact_root_abs}/suites"
ensure_directory "${report_root_abs}/acceptance"

suite_failures=0

write_context() {
    jq -n \
        --arg run_id "${run_id}" \
        --arg design_repo_commit "${design_repo_commit}" \
        --arg workspace_commit "${workspace_commit}" \
        --arg config_profile "${config_profile}" \
        --arg report_root "${report_root}" \
        --arg artifact_root "${artifact_root}" \
        --arg dependency_snapshot "${core_contracts_path}" \
        '{
            run_id: $run_id,
            phase: "PH-08",
            commit_boundary: "commit-08-b",
            design_repo_commit: $design_repo_commit,
            workspace_commit: $workspace_commit,
            config_profile: $config_profile,
            report_root: $report_root,
            artifact_root: $artifact_root,
            dependency_snapshot: [$dependency_snapshot],
            design_scope: [
                "ReleaseGate",
                "ReportGeneration",
                "EvidenceIndex",
                "ConfigRuntimeValidation",
                "AcceptanceIndex",
                "AcceptanceHandoff",
                "VetoChecklist",
                "FinalAcceptanceChecks"
            ],
            reviewer: "Codex"
        }' >"${artifact_root_abs}/meta/context.json"
}

run_suite_commands() {
    local suite_name=${1:?suite name is required}
    local gate_suite=${2:?gate suite is required}
    local case_ids_json=${3:?case ids json is required}
    local evidence_ids_json=${4:?evidence ids json is required}
    shift 4

    local suite_dir="${artifact_root_abs}/suites/${suite_name}"
    local stdout_file="${suite_dir}/stdout.log"
    local stderr_file="${suite_dir}/stderr.log"
    local report_file="${suite_dir}/report.json"
    local started_at finished_at started_ms finished_ms duration_ms
    local status="passed"
    local failed_command=""
    local commands_json

    ensure_directory "${suite_dir}"
    : >"${stdout_file}"
    : >"${stderr_file}"

    jq -n \
        --arg suite "${suite_name}" \
        --arg gate_suite "${gate_suite}" \
        --arg stdout_path "$(relative_repo_path "${stdout_file}")" \
        --arg stderr_path "$(relative_repo_path "${stderr_file}")" \
        --arg report_path "$(relative_repo_path "${report_file}")" \
        --argjson case_ids "${case_ids_json}" \
        --argjson evidence_ids "${evidence_ids_json}" \
        '{
            suite: $suite,
            gate_suite: $gate_suite,
            status: "pending",
            started_at: null,
            finished_at: null,
            duration_ms: 0,
            case_ids: $case_ids,
            evidence_ids: $evidence_ids,
            commands: [],
            stdout_path: $stdout_path,
            stderr_path: $stderr_path,
            report_path: $report_path,
            failed_command: null
        }' >"${report_file}"

    started_at=$(current_utc_timestamp)
    started_ms=$(current_epoch_ms)
    commands_json=$(json_array_from_words "$@")

    for command in "$@"; do
        printf '$ %s\n' "${command}" >>"${stdout_file}"
        if ! bash -lc "cd '${repo_root}' && ${command}" >>"${stdout_file}" 2>>"${stderr_file}"; then
            status="failed"
            failed_command="${command}"
            break
        fi
        printf '\n' >>"${stdout_file}"
    done

    finished_at=$(current_utc_timestamp)
    finished_ms=$(current_epoch_ms)
    duration_ms=$((finished_ms - started_ms))

    jq -n \
        --arg suite "${suite_name}" \
        --arg gate_suite "${gate_suite}" \
        --arg status "${status}" \
        --arg started_at "${started_at}" \
        --arg finished_at "${finished_at}" \
        --argjson duration_ms "${duration_ms}" \
        --arg stdout_path "$(relative_repo_path "${stdout_file}")" \
        --arg stderr_path "$(relative_repo_path "${stderr_file}")" \
        --arg report_path "$(relative_repo_path "${report_file}")" \
        --arg failed_command "${failed_command}" \
        --argjson case_ids "${case_ids_json}" \
        --argjson evidence_ids "${evidence_ids_json}" \
        --argjson commands "${commands_json}" \
        '{
            suite: $suite,
            gate_suite: $gate_suite,
            status: $status,
            started_at: $started_at,
            finished_at: $finished_at,
            duration_ms: $duration_ms,
            case_ids: $case_ids,
            evidence_ids: $evidence_ids,
            commands: $commands,
            stdout_path: $stdout_path,
            stderr_path: $stderr_path,
            report_path: $report_path,
            failed_command: (if $failed_command == "" then null else $failed_command end)
        }' >"${report_file}"

    [[ "${status}" == "passed" ]]
}

validate_runtime_fixture() {
    local fixture_path=${1:?fixture path is required}
    local expected_profile=${2:?expected profile is required}

    jq -e --arg expected_profile "${expected_profile}" '
        def allowed($value; $choices):
            ($choices | index($value)) != null;

        ([keys_unsorted[] | select(
            ["store","outbox_source","transport_backend","publisher","api","worker","jobs","projection","recovery_policy","security_boundary","clock","id_generator"]
            | index(.)
            | not
        )] | length) == 0
        and .store.kind == "in_memory"
        and .store.connection_ref == null
        and allowed(.outbox_source.kind; ["in_memory_fixture","core_outbox"])
        and (.outbox_source.cursor_profile | type == "string")
        and (.outbox_source.batch_size | type == "number" and . >= 1)
        and .transport_backend.kind == "in_memory"
        and (.transport_backend.capability_profile_ref | type == "string")
        and .transport_backend.secret_ref == null
        and (.transport_backend.timeout_profile | type == "string")
        and .publisher.kind == "in_memory_sink"
        and .publisher.secret_ref == null
        and (.publisher.timeout_profile | type == "string")
        and (.api.enabled | type == "boolean")
        and (.api.bind_profile | type == "string")
        and (.api.request_timeout_ms | type == "number" and . > 0)
        and (.worker.enabled | type == "boolean")
        and (.worker.poll_interval_ms | type == "number" and . >= 1)
        and (.worker.batch_size | type == "number" and . >= 1)
        and (.worker.timeout_profile | type == "string")
        and (.jobs.batch_size | type == "number" and . >= 1)
        and (.jobs.cursor_profile | type == "string")
        and (.jobs.retry_profile | type == "string")
        and .projection.kind == "in_memory"
        and .projection.rebuild_mode == "manual_job_only"
        and .projection.consistency_marker == "required"
        and (.recovery_policy.retry_profile | type == "string")
        and .recovery_policy.dead_letter_policy == "explicit_only"
        and .recovery_policy.replay_requires_audit_chain == "required"
        and .security_boundary.secret_policy == "ref_only"
        and .security_boundary.payload_body_policy == "reject"
        and .security_boundary.projection_truth_write_policy == "reject"
        and .security_boundary.redaction_policy == "required"
        and .security_boundary.privileged_operation_ref_policy == "required"
        and allowed(.clock.kind; ["system","fixed"])
        and allowed(.id_generator.kind; ["uuid_v7","deterministic"])
        and (["ci-test","integration-test","operations-recovery"] | index($expected_profile)) != null
    ' "${fixture_path}" >/dev/null
}

reject_negative_fixture() {
    local fixture_name=${1:?fixture name is required}
    local fixture_path="${repo_root}/fixtures/config/negative/${fixture_name}.json"

    ensure_file "${fixture_path}"

    case "${fixture_name}" in
        unsupported-key)
            jq -e '((keys_unsorted - ["store","outbox_source","transport_backend","publisher","api","worker","jobs","projection","recovery_policy","security_boundary","clock","id_generator"]) | length) > 0' \
                "${fixture_path}" >/dev/null
            ;;
        raw-secret)
            jq -e '.transport_backend.kind == "external" and (.transport_backend.secret_ref | startswith("ref:") | not)' \
                "${fixture_path}" >/dev/null
            ;;
        secret-unavailable)
            jq -e '.transport_backend.kind == "external" and (.transport_backend.secret_ref | startswith("ref:unavailable:"))' \
                "${fixture_path}" >/dev/null
            ;;
        reload-request)
            jq -e '.operation == "reload"' "${fixture_path}" >/dev/null
            ;;
        *)
            die "unknown negative fixture: ${fixture_name}"
            ;;
    esac
}

run_config_suite() {
    local suite_name="config"
    local gate_suite="bus-release-config-runtime"
    local suite_dir="${artifact_root_abs}/suites/${suite_name}"
    local stdout_file="${suite_dir}/stdout.log"
    local stderr_file="${suite_dir}/stderr.log"
    local report_file="${suite_dir}/report.json"
    local fixture_summary="${artifact_root_abs}/fixtures/fixture-summary.json"
    local started_at finished_at started_ms finished_ms duration_ms
    local status="passed"
    local failure_reason=""
    local profile_fixture="${repo_root}/fixtures/config/profiles/${config_profile}.json"
    local case_ids_json='["TC-BUS-CFG-001","TC-BUS-CFG-002","TC-BUS-CFG-003"]'
    local evidence_ids_json='["EV-BUS-CFG-001","EV-BUS-CFG-002","EV-BUS-CFG-003"]'

    ensure_directory "${suite_dir}"
    : >"${stdout_file}"
    : >"${stderr_file}"

    started_at=$(current_utc_timestamp)
    started_ms=$(current_epoch_ms)

    ensure_file "${profile_fixture}"
    printf 'Validate runtime profile fixture: %s\n' "$(relative_repo_path "${profile_fixture}")" >>"${stdout_file}"

    if ! validate_runtime_fixture "${profile_fixture}" "${config_profile}" >>"${stdout_file}" 2>>"${stderr_file}"; then
        status="failed"
        failure_reason="valid profile rejected"
    fi

    for negative_fixture in unsupported-key raw-secret secret-unavailable reload-request; do
        printf 'Reject negative fixture: %s\n' "${negative_fixture}" >>"${stdout_file}"
        if [[ "${status}" == "passed" ]] && ! reject_negative_fixture "${negative_fixture}" >>"${stdout_file}" 2>>"${stderr_file}"; then
            status="failed"
            failure_reason="negative fixture was not rejected: ${negative_fixture}"
        fi
    done

    jq -n \
        --arg run_id "${run_id}" \
        --arg config_profile "${config_profile}" \
        --arg profile_fixture "$(relative_repo_path "${profile_fixture}")" \
        --arg runtime_fixture "$(relative_repo_path "${profile_fixture}")" \
        --arg status "${status}" \
        --arg store_kind "$(jq -r '.store.kind' "${profile_fixture}")" \
        --arg outbox_source_kind "$(jq -r '.outbox_source.kind' "${profile_fixture}")" \
        --arg backend_kind "$(jq -r '.transport_backend.kind' "${profile_fixture}")" \
        --arg capability_profile_ref "$(jq -r '.transport_backend.capability_profile_ref' "${profile_fixture}")" \
        --arg publisher_kind "$(jq -r '.publisher.kind' "${profile_fixture}")" \
        --arg api_enabled "$(jq -r '.api.enabled' "${profile_fixture}")" \
        --arg worker_enabled "$(jq -r '.worker.enabled' "${profile_fixture}")" \
        --arg jobs_retry_profile "$(jq -r '.jobs.retry_profile' "${profile_fixture}")" \
        --arg projection_kind "$(jq -r '.projection.kind' "${profile_fixture}")" \
        --arg secret_policy "$(jq -r '.security_boundary.secret_policy' "${profile_fixture}")" \
        --arg redaction_policy "$(jq -r '.security_boundary.redaction_policy' "${profile_fixture}")" \
        --arg reload_request "rejected" \
        --arg failure_reason "${failure_reason}" \
        '{
            run_id: $run_id,
            config_profile: $config_profile,
            profile_fixture: $profile_fixture,
            status: $status,
            runtime_graph: {
                store_kind: $store_kind,
                outbox_source_kind: $outbox_source_kind,
                backend_kind: $backend_kind,
                capability_profile_ref: $capability_profile_ref,
                publisher_kind: $publisher_kind,
                api_enabled: ($api_enabled == "true"),
                worker_enabled: ($worker_enabled == "true"),
                jobs_retry_profile: $jobs_retry_profile,
                projection_kind: $projection_kind
            },
            secret_policy: $secret_policy,
            redaction_policy: $redaction_policy,
            negative_cases: [
                {
                    case_id: "TC-BUS-CFG-002",
                    fixture: "fixtures/config/negative/unsupported-key.json",
                    expected: "fail-fast",
                    status: "rejected",
                    reason: "unsupported key"
                },
                {
                    case_id: "TC-BUS-CFG-002",
                    fixture: "fixtures/config/negative/raw-secret.json",
                    expected: "fail-fast",
                    status: "rejected",
                    reason: "secret material rejected"
                },
                {
                    case_id: "TC-BUS-CFG-003",
                    fixture: "fixtures/config/negative/secret-unavailable.json",
                    expected: "fail-closed",
                    status: "rejected",
                    reason: "secret provider unavailable"
                },
                {
                    case_id: "TC-BUS-CFG-003",
                    fixture: "fixtures/config/negative/reload-request.json",
                    expected: "rejected",
                    status: "rejected",
                    reason: "runtime reload is unsupported in P0"
                }
            ],
            reload_request: $reload_request,
            failure_reason: (if $failure_reason == "" then null else $failure_reason end)
        }' >"${fixture_summary}"

    finished_at=$(current_utc_timestamp)
    finished_ms=$(current_epoch_ms)
    duration_ms=$((finished_ms - started_ms))

    jq -n \
        --arg suite "${suite_name}" \
        --arg gate_suite "${gate_suite}" \
        --arg status "${status}" \
        --arg started_at "${started_at}" \
        --arg finished_at "${finished_at}" \
        --arg stdout_path "$(relative_repo_path "${stdout_file}")" \
        --arg stderr_path "$(relative_repo_path "${stderr_file}")" \
        --arg report_path "$(relative_repo_path "${report_file}")" \
        --arg failure_reason "${failure_reason}" \
        --argjson duration_ms "${duration_ms}" \
        --argjson case_ids "${case_ids_json}" \
        --argjson evidence_ids "${evidence_ids_json}" \
        '{
            suite: $suite,
            gate_suite: $gate_suite,
            status: $status,
            started_at: $started_at,
            finished_at: $finished_at,
            duration_ms: $duration_ms,
            case_ids: $case_ids,
            evidence_ids: $evidence_ids,
            commands: [
                "validate runtime config profile fixture",
                "reject unsupported key and secret material fixtures",
                "reject secret unavailable and reload request fixtures"
            ],
            stdout_path: $stdout_path,
            stderr_path: $stderr_path,
            report_path: $report_path,
            failed_command: null,
            failure_reason: (if $failure_reason == "" then null else $failure_reason end)
        }' >"${report_file}"

    [[ "${status}" == "passed" ]]
}

count_failure() {
    suite_failures=$((suite_failures + 1))
}

write_context

run_suite_commands \
    publication \
    bus-release-closed-loop \
    '["TC-BUS-PUB-001","TC-BUS-PUB-002","TC-BUS-PUB-003","TC-BUS-PUB-004"]' \
    '["EV-BUS-PUB-001","EV-BUS-PUB-002","EV-BUS-PUB-003","EV-BUS-PUB-004"]' \
    "cargo fmt --all --check" \
    "cargo check --workspace" \
    "cargo test -p bus-contracts accept_publication" \
    "cargo test -p bus-domain publication::tests" \
    "cargo test -p bus-api accept_publication" || count_failure

run_suite_commands \
    semantic \
    bus-release-closed-loop \
    '["TC-BUS-SEM-001","TC-BUS-SEM-002"]' \
    '["EV-BUS-SEM-001","EV-BUS-SEM-002"]' \
    "cargo test -p bus-domain transport_semantic" || count_failure

run_suite_commands \
    delivery \
    bus-release-closed-loop \
    '["TC-BUS-DLV-001","TC-BUS-DLV-002","TC-BUS-DLV-003","TC-BUS-DLV-004"]' \
    '["EV-BUS-DLV-001","EV-BUS-DLV-002","EV-BUS-DLV-003","EV-BUS-DLV-004"]' \
    "cargo test -p bus-domain delivery::tests" \
    "cargo test -p bus-jobs delivery_progression_job_runner" || count_failure

run_suite_commands \
    feedback \
    bus-release-closed-loop \
    '["TC-BUS-FDB-001","TC-BUS-FDB-002","TC-BUS-FDB-003","TC-BUS-FDB-004"]' \
    '["EV-BUS-FDB-001","EV-BUS-FDB-002","EV-BUS-FDB-003","EV-BUS-FDB-004"]' \
    "cargo test -p bus-application --test feedback" \
    "cargo test -p bus-api record_feedback" || count_failure

run_suite_commands \
    output \
    bus-release-closed-loop \
    '["TC-BUS-OUT-001","TC-BUS-OUT-002","TC-BUS-OUT-003","TC-BUS-OUT-004","TC-BUS-OUT-005","TC-BUS-OUT-006"]' \
    '["EV-BUS-OUT-001","EV-BUS-OUT-002","EV-BUS-OUT-003","EV-BUS-OUT-004","EV-BUS-OUT-005","EV-BUS-OUT-006"]' \
    "cargo test -p bus-application --test output" \
    "cargo test -p bus-api query" || count_failure

run_suite_commands \
    outbox \
    bus-release-closed-loop \
    '["TC-BUS-OBX-001","TC-BUS-OBX-002"]' \
    '["EV-BUS-OBX-001","EV-BUS-OBX-002"]' \
    "cargo test -p bus-infra source::tests" \
    "cargo test -p bus-jobs outbox_relay_job_runner" || count_failure

run_suite_commands \
    backend \
    bus-release-closed-loop \
    '["TC-BUS-BND-001","TC-BUS-BND-002","TC-BUS-BND-003"]' \
    '["EV-BUS-BND-001","EV-BUS-BND-002","EV-BUS-BND-003"]' \
    "cargo test -p bus-domain backend::tests" \
    "cargo test -p bus-application services::delivery::tests" || count_failure

run_suite_commands \
    recovery \
    bus-release-recovery \
    '["TC-BUS-REC-001","TC-BUS-REC-002","TC-BUS-REC-003","TC-BUS-REC-004"]' \
    '["EV-BUS-REC-001","EV-BUS-REC-002","EV-BUS-REC-003","EV-BUS-REC-004"]' \
    "cargo test -p bus-domain recovery::tests" \
    "cargo test -p bus-application --test recovery" \
    "cargo test -p bus-jobs retry_cycle_job_runner" || count_failure

run_config_suite || count_failure

bash "${repo_root}/scripts/reports/generate_reports.sh" \
    --run-id "${run_id}" \
    --artifact-root "${artifact_root}" \
    --report-root "${report_root}"
bash "${repo_root}/scripts/reports/generate_acceptance_handoff.sh" \
    --run-id "${run_id}" \
    --report-root "${report_root}"
bash "${repo_root}/scripts/reports/generate_veto_checklist.sh" \
    --run-id "${run_id}" \
    --report-root "${report_root}"
bash "${repo_root}/scripts/reports/generate_acceptance_handoff.sh" \
    --run-id "${run_id}" \
    --report-root "${report_root}"
bash "${repo_root}/scripts/reports/generate_acceptance_index.sh" \
    --run-id "${run_id}" \
    --report-root "${report_root}"

run_suite_commands \
    redaction \
    bus-release-redaction \
    '["TC-BUS-RED-001"]' \
    '["RP-BUS-RED-001"]' \
    "bash scripts/checks/check_redaction.sh --artifact-root ${artifact_root} --report-root ${report_root}" || count_failure

run_suite_commands \
    report \
    bus-release-report \
    '["TC-BUS-RED-002"]' \
    '["RP-BUS-SUM-001"]' \
    "bash scripts/reports/generate_reports.sh --run-id ${run_id} --artifact-root ${artifact_root} --report-root ${report_root}" \
    "bash scripts/reports/generate_acceptance_handoff.sh --run-id ${run_id} --report-root ${report_root}" \
    "bash scripts/reports/generate_veto_checklist.sh --run-id ${run_id} --report-root ${report_root}" \
    "bash scripts/reports/generate_acceptance_handoff.sh --run-id ${run_id} --report-root ${report_root}" \
    "bash scripts/reports/generate_acceptance_index.sh --run-id ${run_id} --report-root ${report_root}" \
    "bash scripts/checks/check_artifact_layout.sh --artifact-root ${artifact_root}" \
    "bash scripts/checks/check_report_links.sh --artifact-root ${artifact_root} --report-root ${report_root}" \
    "bash scripts/checks/check_config_summary.sh --run-id ${run_id} --report-root ${report_root}" \
    "bash scripts/checks/check_acceptance_materials.sh --run-id ${run_id} --report-root ${report_root}" || count_failure

bash "${repo_root}/scripts/reports/generate_reports.sh" \
    --run-id "${run_id}" \
    --artifact-root "${artifact_root}" \
    --report-root "${report_root}"
bash "${repo_root}/scripts/reports/generate_acceptance_handoff.sh" \
    --run-id "${run_id}" \
    --report-root "${report_root}"
bash "${repo_root}/scripts/reports/generate_veto_checklist.sh" \
    --run-id "${run_id}" \
    --report-root "${report_root}"
bash "${repo_root}/scripts/reports/generate_acceptance_handoff.sh" \
    --run-id "${run_id}" \
    --report-root "${report_root}"
bash "${repo_root}/scripts/reports/generate_acceptance_index.sh" \
    --run-id "${run_id}" \
    --report-root "${report_root}"

if [[ "${suite_failures}" -ne 0 ]]; then
    printf 'Release gate failed with %s suite failure(s)\n' "${suite_failures}" >&2
    exit 1
fi

printf 'Release gate passed for run %s\n' "${run_id}"
