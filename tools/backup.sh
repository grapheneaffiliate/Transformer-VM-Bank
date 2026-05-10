#!/usr/bin/env bash
# PSL backup tool — see docs/runbooks/dr-restore.md.
#
# Modes:
#   tools/backup.sh                   # take a fresh backup (default cron mode)
#   tools/backup.sh --verify <dir>    # verify integrity of a restored dir
#   tools/backup.sh --verify-latest   # pull latest backup, verify, throw away
#   tools/backup.sh --restore-latest --target <dir>   # pull latest into <dir>
#   tools/backup.sh --list            # list backups available in remote
#
# Storage:
#   Hot tier:  ${PSL_BACKUP_HOT_URI}   (e.g. s3://psl-backups-hot/)
#   Cold tier: ${PSL_BACKUP_COLD_URI}  (e.g. s3://psl-backups-cold/glacier/)
# Both required; the script aborts if either is unset, because a single-tier
# backup is a known-bad pattern (see DR runbook § "If backups are gone too").
#
# Manifest format (written alongside each backup):
#   { "height": <u64>,
#     "state_root_blake3": "<hex>",
#     "taken_at_unix":     <u64>,
#     "psl_version":       "<git-sha>",
#     "files": { "path": "<blake3>", ... } }
#
# Verification recomputes BLAKE3 over each file and re-derives the state root,
# then compares against the manifest. Mismatches are loud and exit non-zero.

set -euo pipefail

STATE_DIR="${PSL_STATE_DIR:-/var/lib/psl}"
HOT="${PSL_BACKUP_HOT_URI:?PSL_BACKUP_HOT_URI must be set}"
COLD="${PSL_BACKUP_COLD_URI:?PSL_BACKUP_COLD_URI must be set}"
NOW="$(date -u +%Y-%m-%dT%H-%M-%SZ)"
WORK="$(mktemp -d)"
trap 'rm -rf "${WORK}"' EXIT

log()  { printf '[backup %s] %s\n' "$(date -u +%H:%M:%S)" "$*" >&2; }
die()  { log "FATAL: $*"; exit 1; }

cmd_backup() {
    log "snapshotting ${STATE_DIR} (height: $(psl-admin current-height))"
    psl-admin pause-writes
    trap 'psl-admin resume-writes' EXIT

    local archive="${WORK}/psl-state-${NOW}.tar"
    tar -cf "${archive}" -C "${STATE_DIR}" .

    local manifest="${WORK}/manifest-${NOW}.json"
    psl-admin emit-backup-manifest \
        --state-dir "${STATE_DIR}" \
        --archive "${archive}" \
        > "${manifest}"

    log "uploading to hot tier ${HOT}"
    aws s3 cp "${archive}"  "${HOT}${NOW}/state.tar"
    aws s3 cp "${manifest}" "${HOT}${NOW}/manifest.json"

    log "uploading to cold tier ${COLD}"
    aws s3 cp "${archive}"  "${COLD}${NOW}/state.tar"  --storage-class GLACIER
    aws s3 cp "${manifest}" "${COLD}${NOW}/manifest.json"

    psl-admin resume-writes
    trap - EXIT

    # Update Prometheus pushgateway so PSLBackupAge alert clears.
    cat <<EOF | curl --data-binary @- "http://pushgw:9091/metrics/job/psl_backup"
psl_backup_age_seconds 0
psl_backup_last_height $(psl-admin current-height)
EOF
    log "done. backup at ${HOT}${NOW}/"
}

cmd_verify_dir() {
    local dir="$1"
    [[ -d "${dir}" ]] || die "not a directory: ${dir}"
    local manifest="${dir}/manifest.json"
    [[ -f "${manifest}" ]] || die "no manifest.json in ${dir}"
    log "verifying ${dir} against ${manifest}"
    psl-admin verify-backup --dir "${dir}" --manifest "${manifest}" \
        || die "verification FAILED — backup is corrupt or tampered with"
    log "verification OK"
}

cmd_verify_latest() {
    local latest
    latest="$(aws s3 ls "${HOT}" | awk '{print $2}' | sort | tail -n1 | tr -d /)"
    [[ -n "${latest}" ]] || die "no backups in ${HOT}"
    log "pulling latest backup ${latest}"
    aws s3 sync "${HOT}${latest}/" "${WORK}/${latest}/"
    tar -xf "${WORK}/${latest}/state.tar" -C "${WORK}/${latest}/"
    cmd_verify_dir "${WORK}/${latest}"
}

cmd_restore_latest() {
    local target="$1"
    [[ -d "${target}" ]] || die "target ${target} does not exist"
    [[ -z "$(ls -A "${target}")" ]] || die "target ${target} is not empty"
    local latest
    latest="$(aws s3 ls "${HOT}" | awk '{print $2}' | sort | tail -n1 | tr -d /)"
    [[ -n "${latest}" ]] || die "no backups in ${HOT}"
    log "restoring ${latest} into ${target}"
    aws s3 sync "${HOT}${latest}/" "${WORK}/${latest}/"
    tar -xf "${WORK}/${latest}/state.tar" -C "${target}/"
    cp "${WORK}/${latest}/manifest.json" "${target}/manifest.json"
    cmd_verify_dir "${target}"
}

cmd_list() {
    aws s3 ls "${HOT}"
}

case "${1:-backup}" in
    backup)         cmd_backup ;;
    --verify)       cmd_verify_dir "${2:?dir required}" ;;
    --verify-latest) cmd_verify_latest ;;
    --restore-latest)
        shift
        target=""
        while [[ $# -gt 0 ]]; do
            case "$1" in
                --target) target="$2"; shift 2 ;;
                *)        die "unknown arg: $1" ;;
            esac
        done
        [[ -n "${target}" ]] || die "--target required"
        cmd_restore_latest "${target}"
        ;;
    --list) cmd_list ;;
    *)      die "unknown mode: $1" ;;
esac
