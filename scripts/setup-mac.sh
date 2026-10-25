#!/bin/bash
# setup-mac.sh — one-time setup for PublishKit on macOS
#
# Installs: Node.js 20 (needed by generate-blog-data.js when publishing posts)
# WebKit is built into macOS — no separate install needed.
# Already-installed tools at a sufficient version are skipped.
#
# No admin rights needed: Node.js comes from Homebrew when Homebrew is already
# installed and writable by you, otherwise it is unpacked into ~/.local/nodejs
# and put on the PATH of your login shell.

set -e

NODE_VERSION="20.18.1"
NODE_USER_DIR="$HOME/.local/nodejs"

info()  { echo "[info]  $*"; }
ok()    { echo "[ok]    $*"; }
skip()  { echo "[skip]  $*"; }
warn()  { echo "[warn]  $*"; }

# Adds $1 to PATH in the profile a login shell reads: PublishKit runs node
# through `$SHELL -lc`, so that is where it has to be found.
add_to_login_path() {
    local dir="$1" line profile
    line="export PATH=\"$dir:\$PATH\""
    case "$(basename "${SHELL:-/bin/zsh}")" in
        zsh)  profile="$HOME/.zprofile" ;;
        bash) if [[ -f "$HOME/.bash_profile" ]]; then profile="$HOME/.bash_profile"; else profile="$HOME/.profile"; fi ;;
        *)    warn "Unknown shell $SHELL — add $dir to your PATH yourself."; return ;;
    esac
    if ! grep -qsF "$line" "$profile"; then
        printf '\n# Node.js for PublishKit\n%s\n' "$line" >> "$profile"
        ok "Added $dir to PATH in $profile"
    fi
}

# Per-user Node.js: unpack the official tarball, no installer and no sudo.
install_node_user() {
    local arch tmp
    case "$(uname -m)" in
        arm64)  arch=arm64 ;;
        x86_64) arch=x64 ;;
        *)      warn "Unsupported CPU $(uname -m) — install Node.js 20 yourself."; return 1 ;;
    esac
    info "Installing Node.js $NODE_VERSION into $NODE_USER_DIR..."
    tmp="$(mktemp -d)"
    curl -fsSL "https://nodejs.org/dist/v${NODE_VERSION}/node-v${NODE_VERSION}-darwin-${arch}.tar.gz" \
        | tar -xz -C "$tmp"
    rm -rf "$NODE_USER_DIR"
    mkdir -p "$(dirname "$NODE_USER_DIR")"
    mv "$tmp/node-v${NODE_VERSION}-darwin-${arch}" "$NODE_USER_DIR"
    rm -rf "$tmp"
    export PATH="$NODE_USER_DIR/bin:$PATH"
    add_to_login_path "$NODE_USER_DIR/bin"
}

install_node() {
    if command -v brew &>/dev/null && [[ -w "$(brew --prefix)" ]]; then
        info "Installing Node.js 20 with Homebrew..."
        brew install node@20
        brew link --force --overwrite node@20
    else
        install_node_user
    fi
}

# ── Node.js 20 ────────────────────────────────────────────────────────────────
if ! command -v node &>/dev/null; then
    install_node
    ok "Node.js installed ($(node --version))"
else
    NODE_MAJOR=$(node --version | sed 's/v\([0-9]*\).*/\1/')
    if [ "$NODE_MAJOR" -lt 18 ]; then
        warn "Node.js $(node --version) is too old — upgrading to v20..."
        install_node
        ok "Node.js upgraded ($(node --version))"
    else
        skip "Node.js already installed ($(node --version))"
    fi
fi

echo ""
echo "Setup complete."
echo "Open PublishKit from Applications (or ~/Applications)."
