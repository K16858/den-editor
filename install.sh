#!/usr/bin/env bash
set -euo pipefail

BINARY_NAME="den"
INSTALL_DIR="$HOME/.local/bin"
CONFIG_DIR="$HOME/.config/$BINARY_NAME"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEFAULT_CONFIG_DIR="$SCRIPT_DIR/docs/examples/default"

# ── Prerequisites ────────────────────────────────────────────────────────────

if ! command -v cargo &>/dev/null; then
    echo "Error: cargo not found. Install it from https://rustup.rs" >&2
    exit 1
fi

# ── Build ────────────────────────────────────────────────────────────────────

echo "Building $BINARY_NAME..."
cargo build --release --manifest-path "$SCRIPT_DIR/Cargo.toml"

# ── Install binary ───────────────────────────────────────────────────────────

mkdir -p "$INSTALL_DIR"
cp "$SCRIPT_DIR/target/release/$BINARY_NAME" "$INSTALL_DIR/$BINARY_NAME"
chmod +x "$INSTALL_DIR/$BINARY_NAME"
echo "Installed: $INSTALL_DIR/$BINARY_NAME"

# ── Update PATH ──────────────────────────────────────────────────────────────

PATH_LINE="export PATH=\"\$HOME/.local/bin:\$PATH\""

add_to_shell_rc() {
    local rc="$1"
    if [[ -f "$rc" ]]; then
        if ! grep -qF "$HOME/.local/bin" "$rc"; then
            printf '\n# den editor\n%s\n' "$PATH_LINE" >> "$rc"
            echo "Added to PATH: $rc"
        fi
    fi
}

add_to_shell_rc "$HOME/.bashrc"
add_to_shell_rc "$HOME/.zshrc"

if [[ ":$PATH:" != *":$INSTALL_DIR:"* ]]; then
    echo ""
    echo "Restart your shell or run:"
    echo "   $PATH_LINE"
fi

# ── Install config files ─────────────────────────────────────────────────────

mkdir -p "$CONFIG_DIR/languages" "$CONFIG_DIR/debuggers"

copy_if_missing() {
    local src="$1"
    local dst="$2"
    if [[ ! -f "$dst" ]]; then
        cp "$src" "$dst"
        echo "Created: $dst"
    fi
}

copy_if_missing "$DEFAULT_CONFIG_DIR/colors.toml" "$CONFIG_DIR/colors.toml"

for lang_file in "$DEFAULT_CONFIG_DIR/languages/"*.toml; do
    [[ -f "$lang_file" ]] || continue
    copy_if_missing "$lang_file" "$CONFIG_DIR/languages/$(basename "$lang_file")"
done

for debugger_file in "$DEFAULT_CONFIG_DIR/debuggers/"*.toml; do
    [[ -f "$debugger_file" ]] || continue
    copy_if_missing "$debugger_file" "$CONFIG_DIR/debuggers/$(basename "$debugger_file")"
done

# ── Done ─────────────────────────────────────────────────────────────────────

echo ""
echo "Installation complete!"
echo "  Binary : $INSTALL_DIR/$BINARY_NAME"
echo "  Config : $CONFIG_DIR"
