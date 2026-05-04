#!/bin/sh
set -e

REPO="jaresty/fp"
INSTALL_DIR="/usr/local/bin"
BINARY_NAME="fp"

# Detect OS and architecture
OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
  Darwin)
    case "$ARCH" in
      arm64)
        ASSET="fp-macos-arm64"
        ;;
      x86_64)
        ASSET="fp-macos-x86_64"
        ;;
      *)
        echo "Unsupported macOS architecture: $ARCH" >&2
        exit 1
        ;;
    esac
    ;;
  Linux)
    case "$ARCH" in
      x86_64)
        ASSET="fp-linux-x86_64"
        ;;
      *)
        echo "Unsupported Linux architecture: $ARCH" >&2
        exit 1
        ;;
    esac
    ;;
  *)
    echo "Unsupported operating system: $OS" >&2
    exit 1
    ;;
esac

# Resolve download URL from latest release
LATEST_URL="https://github.com/${REPO}/releases/latest/download/${ASSET}"

echo "Downloading ${ASSET} from ${LATEST_URL}..."

TMP="$(mktemp)"
if command -v curl > /dev/null 2>&1; then
  curl -fsSL "$LATEST_URL" -o "$TMP"
elif command -v wget > /dev/null 2>&1; then
  wget -qO "$TMP" "$LATEST_URL"
else
  echo "Error: curl or wget is required" >&2
  exit 1
fi

chmod +x "$TMP"

# Install — use sudo if needed
if [ -w "$INSTALL_DIR" ]; then
  mv "$TMP" "${INSTALL_DIR}/${BINARY_NAME}"
else
  echo "Installing to ${INSTALL_DIR}/${BINARY_NAME} (requires sudo)..."
  sudo mv "$TMP" "${INSTALL_DIR}/${BINARY_NAME}"
fi

echo "Installed fp to ${INSTALL_DIR}/${BINARY_NAME}"
fp --version 2>/dev/null || true
