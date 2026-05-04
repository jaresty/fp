#!/bin/sh
set -e

REPO="jaresty/fp"
BINARY_NAME="fp"
SYSTEM_INSTALL=0

# Parse flags
for arg in "$@"; do
  case "$arg" in
    --system) SYSTEM_INSTALL=1 ;;
    *) echo "Unknown flag: $arg" >&2; exit 1 ;;
  esac
done

# Detect OS and architecture
OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
  Darwin)
    case "$ARCH" in
      arm64)  ASSET="fp-macos-arm64" ;;
      x86_64) ASSET="fp-macos-x86_64" ;;
      *) echo "Unsupported macOS architecture: $ARCH" >&2; exit 1 ;;
    esac
    DEFAULT_INSTALL_DIR="$HOME/bin"
    ;;
  Linux)
    case "$ARCH" in
      x86_64) ASSET="fp-linux-x86_64" ;;
      *) echo "Unsupported Linux architecture: $ARCH" >&2; exit 1 ;;
    esac
    DEFAULT_INSTALL_DIR="$HOME/.local/bin"
    ;;
  *)
    echo "Unsupported operating system: $OS" >&2
    exit 1
    ;;
esac

if [ "$SYSTEM_INSTALL" = "1" ]; then
  INSTALL_DIR="/usr/local/bin"
else
  INSTALL_DIR="$DEFAULT_INSTALL_DIR"
fi

# Resolve download URL from latest release
LATEST_URL="https://github.com/${REPO}/releases/latest/download/${ASSET}"

echo "Downloading ${ASSET}..."

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

# Create install dir if it doesn't exist (for user-local installs)
if [ "$SYSTEM_INSTALL" = "0" ]; then
  mkdir -p "$INSTALL_DIR"
fi

# Install — use sudo only for system installs
if [ "$SYSTEM_INSTALL" = "1" ]; then
  if [ -w "$INSTALL_DIR" ]; then
    mv "$TMP" "${INSTALL_DIR}/${BINARY_NAME}"
  else
    echo "Installing to ${INSTALL_DIR}/${BINARY_NAME} (requires sudo)..."
    sudo mv "$TMP" "${INSTALL_DIR}/${BINARY_NAME}"
  fi
else
  mv "$TMP" "${INSTALL_DIR}/${BINARY_NAME}"
fi

echo "Installed fp to ${INSTALL_DIR}/${BINARY_NAME}"

# Remind user to add to PATH if not already there
case ":$PATH:" in
  *":${INSTALL_DIR}:"*) ;;
  *)
    echo ""
    echo "Add ${INSTALL_DIR} to your PATH:"
    case "$SHELL" in
      */zsh)  echo "  echo 'export PATH=\"\$HOME/${INSTALL_DIR#$HOME/}:\$PATH\"' >> ~/.zshrc && source ~/.zshrc" ;;
      */fish) echo "  fish_add_path ${INSTALL_DIR}" ;;
      *)      echo "  echo 'export PATH=\"\$HOME/${INSTALL_DIR#$HOME/}:\$PATH\"' >> ~/.bashrc && source ~/.bashrc" ;;
    esac
    ;;
esac
