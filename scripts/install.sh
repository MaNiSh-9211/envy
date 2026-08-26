#!/bin/sh
set -eu

REPO="MaNiSh-9211/envy"
VERSION="${ENVY_VERSION:-latest}"

OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
  Darwin) os="darwin" ;;
  Linux) os="linux" ;;
  *)
    echo "envy: unsupported OS '$OS' — use scripts/install.ps1 on Windows" >&2
    exit 1
    ;;
esac

case "$ARCH" in
  x86_64|amd64) cpu="amd64" ;;
  arm64|aarch64) cpu="arm64" ;;
  *)
    echo "envy: unsupported architecture '$ARCH'" >&2
    exit 1
    ;;
esac

ASSET="envy-${os}-${cpu}"

if [ "$VERSION" = "latest" ]; then
  URL="https://github.com/$REPO/releases/latest/download/$ASSET"
else
  URL="https://github.com/$REPO/releases/download/$VERSION/$ASSET"
fi

DEST="${ENVY_INSTALL_DIR:-$HOME/.local/bin}"
mkdir -p "$DEST"

echo "envy: installing $ASSET -> $DEST/envy"
curl -fsSL "$URL" -o "$DEST/envy"
chmod +x "$DEST/envy"

echo "envy installed."
case ":$PATH:" in
  *":$DEST:"*) ;;
  *) echo "note: add $DEST to your PATH to use 'envy' anywhere" ;;
esac
