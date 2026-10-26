#!/usr/bin/env bash
set -euo pipefail

REPO="K16858/den-editor"
BINARY_NAME="den"
INSTALL_DIR="$HOME/.local/bin"
CONFIG_DIR="$HOME/.config/$BINARY_NAME"
API_BASE="https://api.github.com/repos/$REPO"
RAW_BASE="https://raw.githubusercontent.com/$REPO"

if [[ "${OSTYPE:-}" == darwin* ]]; then
    ASSET_NAME="den-macos-aarch64"
elif [[ "${OSTYPE:-}" == linux* ]]; then
    ASSET_NAME="den-linux-x86_64"
else
    echo "Unsupported OS: ${OSTYPE:-unknown}" >&2
    exit 1
fi

if ! command -v curl >/dev/null 2>&1; then
    echo "curl is required." >&2
    exit 1
fi

if ! command -v sha256sum >/dev/null 2>&1; then
    echo "sha256sum is required." >&2
    exit 1
fi

LATEST_TAG="$(
    curl -fsSL "$API_BASE/releases/latest" | sed -n 's/.*"tag_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' | head -n 1
)"
if [[ -z "$LATEST_TAG" ]]; then
    echo "Failed to resolve latest release tag." >&2
    exit 1
fi

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

BIN_URL="https://github.com/$REPO/releases/download/$LATEST_TAG/$ASSET_NAME"
CHECKSUM_URL="https://github.com/$REPO/releases/download/$LATEST_TAG/sha256sums.txt"
BIN_TMP="$TMP_DIR/$ASSET_NAME"
SUM_TMP="$TMP_DIR/sha256sums.txt"

echo "Downloading $ASSET_NAME ($LATEST_TAG)..."
curl -fL "$BIN_URL" -o "$BIN_TMP"
curl -fL "$CHECKSUM_URL" -o "$SUM_TMP"

EXPECTED_SUM="$(awk -v name="$ASSET_NAME" '$2 == name { print $1 }' "$SUM_TMP")"
if [[ -z "$EXPECTED_SUM" ]]; then
    echo "Checksum entry not found for $ASSET_NAME." >&2
    exit 1
fi

ACTUAL_SUM="$(sha256sum "$BIN_TMP" | awk '{print $1}')"
if [[ "$EXPECTED_SUM" != "$ACTUAL_SUM" ]]; then
    echo "Checksum mismatch for $ASSET_NAME." >&2
    exit 1
fi

mkdir -p "$INSTALL_DIR"
cp "$BIN_TMP" "$INSTALL_DIR/$BINARY_NAME"
chmod +x "$INSTALL_DIR/$BINARY_NAME"

PATH_LINE='export PATH="$HOME/.local/bin:$PATH"'
add_to_shell_rc() {
    local rc="$1"
    if [[ -f "$rc" ]] && ! grep -qF "$HOME/.local/bin" "$rc"; then
        printf '\n# den editor\n%s\n' "$PATH_LINE" >> "$rc"
        echo "Added to PATH: $rc"
    fi
}

add_to_shell_rc "$HOME/.bashrc"
add_to_shell_rc "$HOME/.zshrc"

mkdir -p "$CONFIG_DIR/languages" "$CONFIG_DIR/debuggers"
copy_if_missing() {
    local url="$1"
    local dst="$2"
    if [[ ! -f "$dst" ]]; then
        curl -fsSL "$url" -o "$dst"
        echo "Created: $dst"
    fi
}

copy_if_missing "$RAW_BASE/$LATEST_TAG/docs/examples/default/colors.toml" "$CONFIG_DIR/colors.toml"

for f in c go javascript markdown python rust; do
    copy_if_missing "$RAW_BASE/$LATEST_TAG/docs/examples/default/languages/$f.toml" "$CONFIG_DIR/languages/$f.toml"
done

for f in go python rust; do
    copy_if_missing "$RAW_BASE/$LATEST_TAG/docs/examples/default/debuggers/$f.toml" "$CONFIG_DIR/debuggers/$f.toml"
done

echo ""
echo "Installation complete!"
echo "  Version: $LATEST_TAG"
echo "  Binary : $INSTALL_DIR/$BINARY_NAME"
echo "  Config : $CONFIG_DIR"
