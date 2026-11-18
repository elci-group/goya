#!/usr/bin/env bash
# Sash Phase 0 Artifact Transfer Script
# Transfers Sash binaries from Linux build host to Windows test machines

set -euo pipefail

# Configuration
LINUX_ARTIFACT_BASE="/tmp"
WINDOWS_TARGET_BASE="C:/sash-test/artifacts"
# Windows machine connection (adjust per environment)
# Options: ssh, winrm, smb, manual
TRANSFER_METHOD="${TRANSFER_METHOD:-ssh}"
SSH_USER="${SSH_USER:-sash-test}"
SSH_HOST="${SSH_HOST:-win-test-x64}"
WINRM_USER="${WINRM_USER:-sash-test}"
WINRM_HOST="${WINRM_HOST:-win-test-x64}"
SMB_SHARE="${SMB_SHARE:-//win-test-x64/sash-test/artifacts}"

# Artifacts to transfer (from SASH-ENT-002)
declare -A ARTIFACTS=(
    ["x64/static-window"]="/tmp/sash-ent002/static-window/App.exe"
    ["x64/interactive-cut"]="/tmp/sash-ent002/interactive-cut/App.exe"
    ["i686/static-window"]="/tmp/sash-stamp-i686/App.exe"
    ["i686/interactive-cut"]="/tmp/sash-stamp-ix86/App.exe"
    ["arm64/static-window"]="/tmp/sash-stamp-arm64/App.exe"
    ["arm64/interactive-cut"]="/tmp/sash-stamp-iarm64/App.exe"
)

# Verify all source artifacts exist
verify_sources() {
    echo "Verifying source artifacts..."
    local missing=0
    for key in "${!ARTIFACTS[@]}"; do
        local src="${ARTIFACTS[$key]}"
        if [[ -f "$src" ]]; then
            local sha256=$(sha256sum "$src" | cut -d' ' -f1)
            local size=$(stat -c%s "$src")
            echo "  ✓ $key: $src (${size} bytes, sha256: ${sha256:0:16}...)"
        else
            echo "  ✗ $key: $src NOT FOUND"
            missing=1
        fi
    done
    if [[ $missing -eq 1 ]]; then
        echo "ERROR: Some artifacts missing. Run 'sash build' first or update paths."
        return 1
    fi
    return 0
}

# Transfer via SSH/SCP
transfer_ssh() {
    local arch_profile="$1"
    local src="${ARTIFACTS[$arch_profile]}"
    local dst_dir="${WINDOWS_TARGET_BASE}/${arch_profile}"
    local dst_file="${dst_dir}/App.exe"

    echo "Transferring $arch_profile via SSH to ${SSH_USER}@${SSH_HOST}..."

    # Create directory on Windows (via SSH)
    ssh "${SSH_USER}@${SSH_HOST}" "mkdir -p \"${dst_dir}\"" || {
        echo "  Failed to create remote directory"
        return 1
    }

    # Copy file
    scp "$src" "${SSH_USER}@${SSH_HOST}:${dst_file}" || {
        echo "  SCP failed"
        return 1
    }

    # Verify on remote
    local remote_sha256=$(ssh "${SSH_USER}@${SSH_HOST}" "certutil -hashfile \"${dst_file}\" SHA256" | tail -n +2 | head -n1 | tr -d ' \r\n')
    local local_sha256=$(sha256sum "$src" | cut -d' ' -f1)

    if [[ "$remote_sha256" == "$local_sha256" ]]; then
        echo "  ✓ Transfer verified (SHA256 match)"
        return 0
    else
        echo "  ✗ SHA256 mismatch! Local: $local_sha256, Remote: $remote_sha256"
        return 1
    fi
}

# Transfer via WinRM (PowerShell Remoting)
transfer_winrm() {
    local arch_profile="$1"
    local src="${ARTIFACTS[$arch_profile]}"
    local dst_dir="${WINDOWS_TARGET_BASE}/${arch_profile}"
    local dst_file="${dst_dir}\\App.exe"

    echo "Transferring $arch_profile via WinRM to ${WINRM_HOST}..."

    # Read file as base64
    local b64=$(base64 -w0 "$src")
    local size=$(stat -c%s "$src")

    # PowerShell script to write file
    local ps_script="
        \$dstDir = '${dst_dir}'
        \$dstFile = '${dst_file}'
        \$b64 = '${b64}'
        \$expectedSize = ${size}

        if (-not (Test-Path \$dstDir)) {
            New-Item -ItemType Directory -Path \$dstDir -Force | Out-Null
        }

        \$bytes = [Convert]::FromBase64String(\$b64)
        [IO.File]::WriteAllBytes(\$dstFile, \$bytes)

        \$actualSize = (Get-Item \$dstFile).Length
        if (\$actualSize -ne \$expectedSize) {
            Write-Error \"Size mismatch: expected \$expectedSize, got \$actualSize\"
            exit 1
        }

        \$sha256 = (Get-FileHash -Algorithm SHA256 \$dstFile).Hash.ToLower()
        Write-Output \"SHA256:\$sha256\"
    "

    local result=$(pwsh -Command "$ps_script" 2>&1) || {
        echo "  WinRM transfer failed: $result"
        return 1
    }

    local remote_sha256=$(echo "$result" | grep '^SHA256:' | cut -d: -f2)
    local local_sha256=$(sha256sum "$src" | cut -d' ' -f1 | tr '[:upper:]' '[:lower:]')

    if [[ "$remote_sha256" == "$local_sha256" ]]; then
        echo "  ✓ Transfer verified (SHA256 match)"
        return 0
    else
        echo "  ✗ SHA256 mismatch! Local: $local_sha256, Remote: $remote_sha256"
        return 1
    fi
}

# Transfer via SMB share
transfer_smb() {
    local arch_profile="$1"
    local src="${ARTIFACTS[$arch_profile]}"
    local dst_dir="${SMB_SHARE}/${arch_profile}"

    echo "Transferring $arch_profile via SMB to ${SMB_SHARE}..."

    # Mount if needed (assumes already mounted or using credentials)
    mkdir -p "$dst_dir" || true

    cp "$src" "${dst_dir}/App.exe" || {
        echo "  SMB copy failed"
        return 1
    }

    local remote_sha256=$(sha256sum "${dst_dir}/App.exe" | cut -d' ' -f1)
    local local_sha256=$(sha256sum "$src" | cut -d' ' -f1)

    if [[ "$remote_sha256" == "$local_sha256" ]]; then
        echo "  ✓ Transfer verified (SHA256 match)"
        return 0
    else
        echo "  ✗ SHA256 mismatch!"
        return 1
    fi
}

# Main transfer function
transfer_all() {
    local method="$1"
    local failed=0

    for arch_profile in "${!ARTIFACTS[@]}"; do
        case "$method" in
            ssh) transfer_ssh "$arch_profile" ;;
            winrm) transfer_winrm "$arch_profile" ;;
            smb) transfer_smb "$arch_profile" ;;
            *) echo "Unknown method: $method"; return 1 ;;
        esac

        if [[ $? -ne 0 ]]; then
            failed=1
        fi
    done

    return $failed
}

# Usage
usage() {
    cat <<EOF
Usage: $0 [OPTIONS]

Transfer Sash Phase 0 artifacts from Linux build host to Windows test machines.

Options:
    -m, --method METHOD    Transfer method: ssh, winrm, smb (default: ssh)
    -v, --verify-only      Only verify source artifacts exist
    -h, --help             Show this help

Environment Variables:
    TRANSFER_METHOD        Transfer method (ssh|winrm|smb)
    SSH_USER, SSH_HOST     SSH connection details
    WINRM_USER, WINRM_HOST WinRM connection details
    SMB_SHARE              SMB share path (e.g., //host/share)

Examples:
    $0                                    # Transfer via SSH
    $0 --method winrm                     # Transfer via WinRM
    $0 --verify-only                      # Just check sources
EOF
}

main() {
    local method="$TRANSFER_METHOD"
    local verify_only=false

    while [[ $# -gt 0 ]]; do
        case $1 in
            -m|--method) method="$2"; shift 2 ;;
            -v|--verify-only) verify_only=true; shift ;;
            -h|--help) usage; exit 0 ;;
            *) echo "Unknown option: $1"; usage; exit 1 ;;
        esac
    done

    echo "=== Sash Phase 0 Artifact Transfer ==="
    echo "Method: $method"
    echo ""

    if ! verify_sources; then
        exit 1
    fi

    if [[ "$verify_only" == "true" ]]; then
        echo "Verification complete. All sources present."
        exit 0
    fi

    echo "Starting transfer..."
    if transfer_all "$method"; then
        echo ""
        echo "=== ALL TRANSFERS SUCCESSFUL ==="
        exit 0
    else
        echo ""
        echo "=== SOME TRANSFERS FAILED ==="
        exit 1
    fi
}

main "$@"