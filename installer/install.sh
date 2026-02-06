#!/bin/sh
# CopperMoon Installer — Linux & macOS
# Usage: curl -fsSL https://raw.githubusercontent.com/coppermoondev/coppermoon/refs/heads/main/installer/install.sh | sh
#        COPPERMOON_ARCHIVE=/path/to/coppermoon-x86_64-unknown-linux-gnu.tar.gz sh install.sh
#
# Environment variables:
#   COPPERMOON_INSTALL_DIR    — Custom install directory (default: ~/.coppermoon/bin)
#   COPPERMOON_VERSION        — Specific version to install (default: latest)
#   COPPERMOON_ARCHIVE        — Path to a local .tar.gz archive (skips download)
#   COPPERMOON_NO_MODIFY_PATH — Set to 1 to skip PATH modification

set -e

# ─── Colors ─────────────────────────────────────────────────────────
BOLD='\033[1m'
DIM='\033[2m'
COPPER='\033[38;5;173m'
GREEN='\033[32m'
RED='\033[31m'
YELLOW='\033[33m'
RESET='\033[0m'

info() { printf "${COPPER}>${RESET} %s\n" "$1"; }
success() { printf "${GREEN}✓${RESET} %s\n" "$1"; }
warn() { printf "${YELLOW}!${RESET} %s\n" "$1"; }
error() { printf "${RED}✗${RESET} %s\n" "$1" >&2; exit 1; }

# ─── Banner ─────────────────────────────────────────────────────────
printf "\n"
printf "${COPPER}${BOLD}  ┌─────────────────────────────────────┐${RESET}\n"
printf "${COPPER}${BOLD}  │      🌙 CopperMoon Installer        │${RESET}\n"
printf "${COPPER}${BOLD}  │      Write Lua. Run at Rust speed.   │${RESET}\n"
printf "${COPPER}${BOLD}  └─────────────────────────────────────┘${RESET}\n"
printf "\n"

# ─── Detect Platform ────────────────────────────────────────────────
OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
    Linux)  PLATFORM="unknown-linux-gnu" ;;
    Darwin) PLATFORM="apple-darwin" ;;
    *)      error "Unsupported operating system: $OS. Use Windows PowerShell installer for Windows." ;;
esac

case "$ARCH" in
    x86_64|amd64)   ARCH="x86_64" ;;
    aarch64|arm64)   ARCH="aarch64" ;;
    *)               error "Unsupported architecture: $ARCH" ;;
esac

TARGET="${ARCH}-${PLATFORM}"
info "Detected platform: ${BOLD}${OS} ${ARCH}${RESET} (${TARGET})"

# ─── Configuration ──────────────────────────────────────────────────
INSTALL_DIR="${COPPERMOON_INSTALL_DIR:-$HOME/.coppermoon/bin}"
VERSION="${COPPERMOON_VERSION:-latest}"
GITHUB_REPO="coppermoondev/coppermoon"
NO_MODIFY_PATH="${COPPERMOON_NO_MODIFY_PATH:-0}"
LOCAL_ARCHIVE="${COPPERMOON_ARCHIVE:-}"

info "Install directory: ${BOLD}${INSTALL_DIR}${RESET}"

# ─── Create install directory ────────────────────────────────────────
mkdir -p "$INSTALL_DIR" || error "Failed to create directory: $INSTALL_DIR"

# ─── Get archive (local or download) ─────────────────────────────────
TMPDIR="$(mktemp -d)"
ARCHIVE="${TMPDIR}/coppermoon.tar.gz"

if [ -n "$LOCAL_ARCHIVE" ]; then
    # ─── Local archive ────────────────────────────────────────────
    if [ ! -f "$LOCAL_ARCHIVE" ]; then
        rm -rf "$TMPDIR"
        error "Local archive not found: $LOCAL_ARCHIVE"
    fi
    info "Using local archive: ${BOLD}${LOCAL_ARCHIVE}${RESET}"
    cp "$LOCAL_ARCHIVE" "$ARCHIVE"
else
    # ─── Download ─────────────────────────────────────────────────
    if [ "$VERSION" = "latest" ]; then
        DOWNLOAD_URL="https://github.com/${GITHUB_REPO}/releases/latest/download/coppermoon-${TARGET}.tar.gz"
    else
        DOWNLOAD_URL="https://github.com/${GITHUB_REPO}/releases/download/v${VERSION}/coppermoon-${TARGET}.tar.gz"
    fi

    info "Version: ${BOLD}${VERSION}${RESET}"

    if command -v curl > /dev/null 2>&1; then
        DOWNLOADER="curl"
    elif command -v wget > /dev/null 2>&1; then
        DOWNLOADER="wget"
    else
        error "Neither curl nor wget found. Please install one and try again."
    fi

    info "Downloading CopperMoon..."

    if [ "$DOWNLOADER" = "curl" ]; then
        HTTP_CODE=$(curl -fSL -w "%{http_code}" -o "$ARCHIVE" "$DOWNLOAD_URL" 2>/dev/null) || true
        if [ "$HTTP_CODE" != "200" ] && [ "$HTTP_CODE" != "302" ]; then
            rm -rf "$TMPDIR"
            error "Download failed (HTTP $HTTP_CODE). Check that version '${VERSION}' exists for ${TARGET}.\n  URL: ${DOWNLOAD_URL}"
        fi
    else
        wget -q -O "$ARCHIVE" "$DOWNLOAD_URL" 2>/dev/null || {
            rm -rf "$TMPDIR"
            error "Download failed. Check that version '${VERSION}' exists for ${TARGET}.\n  URL: ${DOWNLOAD_URL}"
        }
    fi
fi

# ─── Extract ─────────────────────────────────────────────────────────
info "Extracting binaries..."

tar xzf "$ARCHIVE" -C "$TMPDIR" 2>/dev/null || {
    rm -rf "$TMPDIR"
    error "Failed to extract archive. The download may be corrupted."
}

# Move binaries to install dir (check both root and subdirectory)
for bin in coppermoon harbor shipyard quarry; do
    SRC=""
    if [ -f "${TMPDIR}/${bin}" ]; then
        SRC="${TMPDIR}/${bin}"
    elif [ -f "${TMPDIR}/coppermoon-${TARGET}/${bin}" ]; then
        SRC="${TMPDIR}/coppermoon-${TARGET}/${bin}"
    fi
    if [ -n "$SRC" ]; then
        mv "$SRC" "${INSTALL_DIR}/${bin}"
        chmod +x "${INSTALL_DIR}/${bin}"
    fi
done

# Cleanup
rm -rf "$TMPDIR"

success "Binaries installed to ${INSTALL_DIR}"

# ─── Update PATH ─────────────────────────────────────────────────────
add_to_path() {
    local profile="$1"
    local line="export PATH=\"${INSTALL_DIR}:\$PATH\""

    if [ -f "$profile" ] && grep -q "$INSTALL_DIR" "$profile" 2>/dev/null; then
        return 0
    fi

    printf "\n# CopperMoon\n%s\n" "$line" >> "$profile"
    return 1
}

add_to_fish_path() {
    local config="$HOME/.config/fish/config.fish"
    local line="set -gx PATH ${INSTALL_DIR} \$PATH"

    if [ -f "$config" ] && grep -q "$INSTALL_DIR" "$config" 2>/dev/null; then
        return 0
    fi

    mkdir -p "$(dirname "$config")"
    printf "\n# CopperMoon\n%s\n" "$line" >> "$config"
    return 1
}

if [ "$NO_MODIFY_PATH" != "1" ]; then
    # Check if already in PATH
    case ":$PATH:" in
        *":${INSTALL_DIR}:"*)
            info "Already in PATH"
            ;;
        *)
            MODIFIED=0

            # Detect shell and update appropriate profile
            CURRENT_SHELL="$(basename "${SHELL:-/bin/sh}")"

            case "$CURRENT_SHELL" in
                zsh)
                    add_to_path "$HOME/.zshrc" && true || MODIFIED=1
                    ;;
                bash)
                    if [ -f "$HOME/.bashrc" ]; then
                        add_to_path "$HOME/.bashrc" && true || MODIFIED=1
                    elif [ -f "$HOME/.bash_profile" ]; then
                        add_to_path "$HOME/.bash_profile" && true || MODIFIED=1
                    else
                        add_to_path "$HOME/.bashrc" && true || MODIFIED=1
                    fi
                    ;;
                fish)
                    add_to_fish_path && true || MODIFIED=1
                    ;;
                *)
                    add_to_path "$HOME/.profile" && true || MODIFIED=1
                    ;;
            esac

            if [ "$MODIFIED" = "1" ]; then
                success "Added to PATH in shell profile"
            fi
            ;;
    esac
else
    warn "Skipping PATH modification (COPPERMOON_NO_MODIFY_PATH=1)"
fi

# ─── Verify ──────────────────────────────────────────────────────────
printf "\n"
if [ -x "${INSTALL_DIR}/coppermoon" ]; then
    VER=$("${INSTALL_DIR}/coppermoon" --version 2>/dev/null || echo "installed")
    success "coppermoon  ${DIM}${VER}${RESET}"
fi
if [ -x "${INSTALL_DIR}/harbor" ]; then
    VER=$("${INSTALL_DIR}/harbor" --version 2>/dev/null || echo "installed")
    success "harbor      ${DIM}${VER}${RESET}"
fi
if [ -x "${INSTALL_DIR}/shipyard" ]; then
    VER=$("${INSTALL_DIR}/shipyard" --version 2>/dev/null || echo "installed")
    success "shipyard    ${DIM}${VER}${RESET}"
fi
if [ -x "${INSTALL_DIR}/quarry" ]; then
    VER=$("${INSTALL_DIR}/quarry" --version 2>/dev/null || echo "installed")
    success "quarry      ${DIM}${VER}${RESET}"
fi

# ─── Success ─────────────────────────────────────────────────────────
printf "\n"
printf "${GREEN}${BOLD}  Installation complete!${RESET}\n"
printf "\n"
printf "  ${DIM}Restart your terminal or run:${RESET}\n"
printf "    ${BOLD}export PATH=\"${INSTALL_DIR}:\$PATH\"${RESET}\n"
printf "\n"
printf "  ${DIM}Get started:${RESET}\n"
printf "    ${BOLD}shipyard new my-app --template web${RESET}\n"
printf "    ${BOLD}cd my-app && shipyard dev${RESET}\n"
printf "\n"
printf "  ${DIM}Documentation:${RESET}  ${COPPER}https://docs.coppermoon.dev${RESET}\n"
printf "  ${DIM}GitHub:${RESET}         ${COPPER}https://github.com/${GITHUB_REPO}${RESET}\n"
printf "\n"
