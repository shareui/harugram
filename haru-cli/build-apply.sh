#!/usr/bin/env bash
set -euo pipefail

BINARY_NAME="haru"
INSTALL_DIR="/usr/local/bin"
TARGET_PATH="target/release/${BINARY_NAME}"

cd "$(dirname "$0")"

echo "Building ${BINARY_NAME} (release)..."

if cargo build --release; then
	if [ ! -f "${TARGET_PATH}" ]; then
		echo "Error: build succeeded but binary not found at ${TARGET_PATH}"
		exit 1
	fi

	if [ ! -w "${INSTALL_DIR}" ]; then
		echo "Installing to ${INSTALL_DIR} (requires sudo)..."
		sudo install -m 755 "${TARGET_PATH}" "${INSTALL_DIR}/${BINARY_NAME}"
	else
		install -m 755 "${TARGET_PATH}" "${INSTALL_DIR}/${BINARY_NAME}"
	fi

	echo "Successfully!"
else
	echo "Build failed."
	exit 1
fi
