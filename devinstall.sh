#!/usr/bin/env bash
set -euo pipefail

BIN_NAME="mpd-overlay"
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TARGET_HOME="${HOME:-$(getent passwd "$(id -u)" | cut -d: -f6)}"

echo "Building ${BIN_NAME}..."
cd "${repo_root}"
cargo build --release

BIN_DIR="${TARGET_HOME}/.local/bin"
mkdir -p "${BIN_DIR}"
install -m 755 "target/release/${BIN_NAME}" "${BIN_DIR}/${BIN_NAME}"

echo "Installed ${BIN_NAME} to ${BIN_DIR}/${BIN_NAME}"
echo "Running ${BIN_NAME} -e to install and enable its systemd user service"
"${BIN_DIR}/${BIN_NAME}" -e
