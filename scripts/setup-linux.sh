#!/usr/bin/env bash
# setup-linux.sh — one-time setup for PublishKit on Linux (Debian/Ubuntu/Fedora/Arch)
#
# Installs: WebKitGTK runtime (required by Dioxus desktop), Node.js 20
# Already-installed tools are skipped automatically.
#
# Usage (from the extracted release archive), as yourself — no sudo:
#   ./setup-linux.sh
#
# Everything goes into your home folder: Node.js into ~/.local/nodejs, the
# launcher and icon under ~/.local/share. Only a missing WebKitGTK needs a
# system package; the script then asks for your password through sudo, or tells
# you which package to ask your administrator for.

set -e

NODE_VERSION="20.18.1"
NODE_USER_DIR="$HOME/.local/nodejs"

info()  { echo -e "\033[34m[info]\033[0m  $*"; }
ok()    { echo -e "\033[32m[ok]\033[0m    $*"; }
skip()  { echo -e "\033[33m[skip]\033[0m  $*"; }
warn()  { echo -e "\033[33m[warn]\033[0m  $*"; }
err()   { echo -e "\033[31m[error]\033[0m $*"; exit 1; }

# Started with sudo out of habit: run as the person who typed it, so the files
# land in their home folder rather than root's.
if [[ $EUID -eq 0 && -n "$SUDO_USER" && "$SUDO_USER" != root ]]; then
  info "Running as $SUDO_USER — this script does not need sudo."
  exec sudo -u "$SUDO_USER" -H "$0" "$@"
fi

if [[ $EUID -eq 0 ]]; then SUDO=""
elif command -v sudo &>/dev/null; then SUDO="sudo"
else SUDO="none"
fi

# Runs a system package command; fails when there is no way to become root.
as_root() { [[ "$SUDO" != none ]] && $SUDO "$@"; }

# ── Detect distro ─────────────────────────────────────────────────────────────
if   command -v apt-get &>/dev/null; then DISTRO=debian
elif command -v dnf     &>/dev/null; then DISTRO=fedora
elif command -v pacman  &>/dev/null; then DISTRO=arch
else DISTRO=other
fi

info "Detected distro family: $DISTRO"

# ── WebKitGTK runtime (required by the Dioxus desktop shell) ──────────────────
webkit_missing() {
  warn "WebKitGTK is not installed and could not be installed without admin rights."
  warn "Ask your administrator to install: $1"
}

case "$DISTRO" in
  debian)
    if ! dpkg -l libwebkit2gtk-4.1-0 &>/dev/null && ! dpkg -l libwebkit2gtk-4.0-0 &>/dev/null; then
      info "Installing libwebkit2gtk (needs your password)..."
      if as_root apt-get update -qq && { as_root apt-get install -y libwebkit2gtk-4.1-0 2>/dev/null || as_root apt-get install -y libwebkit2gtk-4.0-0; }; then
        ok "WebKitGTK installed"
      else
        webkit_missing libwebkit2gtk-4.1-0
      fi
    else
      skip "WebKitGTK already installed"
    fi
    ;;
  fedora)
    if ! rpm -q webkit2gtk4.1 &>/dev/null && ! rpm -q webkit2gtk3 &>/dev/null; then
      info "Installing WebKitGTK (needs your password)..."
      if as_root dnf install -y webkit2gtk4.1 2>/dev/null || as_root dnf install -y webkit2gtk3; then
        ok "WebKitGTK installed"
      else
        webkit_missing webkit2gtk4.1
      fi
    else
      skip "WebKitGTK already installed"
    fi
    ;;
  arch)
    if ! pacman -Qi webkit2gtk-4.1 &>/dev/null && ! pacman -Qi webkit2gtk &>/dev/null; then
      info "Installing WebKitGTK (needs your password)..."
      if as_root pacman -S --noconfirm webkit2gtk-4.1 2>/dev/null || as_root pacman -S --noconfirm webkit2gtk; then
        ok "WebKitGTK installed"
      else
        webkit_missing webkit2gtk-4.1
      fi
    else
      skip "WebKitGTK already installed"
    fi
    ;;
  other)
    warn "Unknown distro — make sure libwebkit2gtk-4.1 is installed."
    ;;
esac

# ── Node.js 20 (needed by generate-blog-data.js when publishing posts) ────────
# Adds $1 to PATH in the profile a login shell reads: PublishKit runs node
# through `$SHELL -lc`, so that is where it has to be found.
add_to_login_path() {
  local dir="$1" line profile
  line="export PATH=\"$dir:\$PATH\""
  case "$(basename "${SHELL:-/bin/bash}")" in
    zsh)  profile="$HOME/.zprofile" ;;
    bash) if [[ -f "$HOME/.bash_profile" ]]; then profile="$HOME/.bash_profile"; else profile="$HOME/.profile"; fi ;;
    *)    warn "Unknown shell $SHELL — add $dir to your PATH yourself."; return ;;
  esac
  if ! grep -qsF "$line" "$profile"; then
    printf '\n# Node.js for PublishKit\n%s\n' "$line" >> "$profile"
    ok "Added $dir to PATH in $profile"
  fi
}

# Per-user Node.js: unpack the official tarball, no package manager and no sudo.
install_node() {
  local arch tmp
  case "$(uname -m)" in
    x86_64)  arch=x64 ;;
    aarch64) arch=arm64 ;;
    *)       err "Unsupported CPU $(uname -m) — install Node.js 20 yourself" ;;
  esac
  info "Installing Node.js $NODE_VERSION into $NODE_USER_DIR..."
  tmp="$(mktemp -d)"
  curl -fsSL "https://nodejs.org/dist/v${NODE_VERSION}/node-v${NODE_VERSION}-linux-${arch}.tar.gz" \
    | tar -xz -C "$tmp"
  rm -rf "$NODE_USER_DIR"
  mkdir -p "$(dirname "$NODE_USER_DIR")"
  mv "$tmp/node-v${NODE_VERSION}-linux-${arch}" "$NODE_USER_DIR"
  rm -rf "$tmp"
  export PATH="$NODE_USER_DIR/bin:$PATH"
  add_to_login_path "$NODE_USER_DIR/bin"
}

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

# ── Desktop shortcut ──────────────────────────────────────────────────────────
BINARY_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DATA_HOME="${XDG_DATA_HOME:-$HOME/.local/share}"
DESKTOP_FILE="$DATA_HOME/applications/publishkit.desktop"

# Register the bundled icon with the hicolor theme so launchers pick it up.
# Falls back to the generic editor glyph if the PNG isn't shipped.
ICON_NAME="text-editor"
if [[ -f "$BINARY_DIR/icon.png" ]]; then
  ICON_DIR="$DATA_HOME/icons/hicolor/256x256/apps"
  install -Dm644 "$BINARY_DIR/icon.png" "$ICON_DIR/publishkit.png"
  ICON_NAME="publishkit"
  ok "Icon installed to $ICON_DIR/publishkit.png"
fi

if [[ -f "$BINARY_DIR/publishkit" ]]; then
  info "Creating .desktop launcher..."
  mkdir -p "$(dirname "$DESKTOP_FILE")"
  cat > "$DESKTOP_FILE" <<EOF
[Desktop Entry]
Name=PublishKit
Comment=Write, queue and publish blog posts
Exec=$BINARY_DIR/publishkit
Icon=$ICON_NAME
Terminal=false
Type=Application
Categories=Office;
EOF
  command -v update-desktop-database &>/dev/null && update-desktop-database -q "$(dirname "$DESKTOP_FILE")" || true
  ok "Desktop shortcut created: $DESKTOP_FILE"
fi

echo ""
echo "Setup complete."
echo "Run: $BINARY_DIR/publishkit"
