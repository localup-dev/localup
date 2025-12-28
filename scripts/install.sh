#!/usr/bin/env bash
set -euo pipefail

# Wrap in block to ensure bash reads entire script before executing (needed for curl | bash)
{

platform=$(uname -ms)

if [[ ${OS:-} = Windows_NT ]]; then
    echo "error: Windows is not supported by this script. Please download manually from GitHub releases." >&2
    exit 1
fi

# Reset
Color_Off=''

# Regular Colors
Red=''
Green=''
Dim=''

# Bold
Bold_White=''
Bold_Green=''

if [[ -t 1 ]]; then
    # Reset
    Color_Off='\033[0m'

    # Regular Colors
    Red='\033[0;31m'
    Green='\033[0;32m'
    Dim='\033[0;2m'

    # Bold
    Bold_Green='\033[1;32m'
    Bold_White='\033[1m'
fi

error() {
    echo -e "${Red}error${Color_Off}:" "$@" >&2
    exit 1
}

info() {
    echo -e "${Dim}$@${Color_Off}"
}

info_bold() {
    echo -e "${Bold_White}$@${Color_Off}"
}

success() {
    echo -e "${Green}$@${Color_Off}"
}

command -v tar >/dev/null ||
    error 'tar is required to install localup'

command -v curl >/dev/null ||
    error 'curl is required to install localup'

if [[ $# -gt 1 ]]; then
    error 'Too many arguments. Only one optional argument is allowed: a specific version tag (e.g., "v0.0.1-beta14")'
fi

case $platform in
'Darwin x86_64')
    target=macos-amd64
    ;;
'Darwin arm64')
    target=macos-arm64
    ;;
'Linux aarch64' | 'Linux arm64')
    target=linux-arm64
    ;;
'Linux x86_64' | *)
    target=linux-amd64
    ;;
esac

if [[ $target = macos-amd64 ]]; then
    # Is this process running in Rosetta?
    if [[ $(sysctl -n sysctl.proc_translated 2>/dev/null) = 1 ]]; then
        target=macos-arm64
        info "Your shell is running in Rosetta 2. Downloading localup for $target instead"
    fi
fi

GITHUB=${GITHUB-"https://github.com"}
github_repo="$GITHUB/localup-dev/localup"

# Get version from argument or fetch latest (including pre-releases) from API
if [[ $# -gt 0 ]]; then
    LOCALUP_VERSION=$1
else
    info "Fetching latest release..."

    # Try gh CLI first (most reliable)
    if command -v gh &> /dev/null; then
        LOCALUP_VERSION=$(gh release list --repo localup-dev/localup --limit 1 2>/dev/null | awk -F'\t' '{print $3}' | head -1) || true
    fi

    # Fallback to curl + jq
    if [[ -z "${LOCALUP_VERSION:-}" ]]; then
        if command -v jq &> /dev/null; then
            LOCALUP_VERSION=$(curl -fsSL "https://api.github.com/repos/localup-dev/localup/releases?per_page=1" 2>/dev/null | jq -r '.[0].tag_name') || true
        fi
    fi

    # Fallback to curl + grep
    if [[ -z "${LOCALUP_VERSION:-}" ]] || [[ "$LOCALUP_VERSION" = "null" ]]; then
        LOCALUP_VERSION=$(curl -fsSL "https://api.github.com/repos/localup-dev/localup/releases?per_page=1" 2>/dev/null | tr ',' '\n' | grep '"tag_name"' | cut -d'"' -f4 | head -1) || true
    fi

    if [[ -z "${LOCALUP_VERSION:-}" ]] || [[ "$LOCALUP_VERSION" = "null" ]]; then
        error "Failed to fetch latest release version from GitHub API. Try specifying a version manually."
    fi
fi

# Validate version format
if ! [[ "$LOCALUP_VERSION" =~ ^v[0-9] ]]; then
    error "Invalid version format: $LOCALUP_VERSION. Expected format like: v0.0.1, v1.0.0-beta1, etc."
fi

info "Installing localup $LOCALUP_VERSION for $target"

# Construct download URLs
localup_uri="$github_repo/releases/download/$LOCALUP_VERSION/localup-$target.tar.gz"
checksums_uri="$github_repo/releases/download/$LOCALUP_VERSION/checksums-$target.txt"

install_env=LOCALUP_INSTALL
install_dir=${!install_env:-$HOME/.localup}
bin_dir=$install_dir/bin

if [[ ! -d $bin_dir ]]; then
    mkdir -p "$bin_dir" ||
        error "Failed to create install directory \"$bin_dir\""
fi

# Download localup
info "Downloading localup..."
curl --fail --location --progress-bar --output "$bin_dir/localup.tar.gz" "$localup_uri" ||
    error "Failed to download localup from \"$localup_uri\""

# Download and verify checksum (optional but recommended)
expected_checksum=""
if curl --fail --location --silent --output "$bin_dir/checksums.txt" "$checksums_uri" 2>/dev/null; then
    # Extract expected checksum for the tar.gz file
    expected_checksum=$(grep "localup-$target.tar.gz" "$bin_dir/checksums.txt" 2>/dev/null | awk '{print $1}') || true
    rm -f "$bin_dir/checksums.txt"
fi

if [[ -n "$expected_checksum" ]]; then
    info "Verifying checksum..."
    actual_checksum=""
    if command -v sha256sum &> /dev/null; then
        actual_checksum=$(sha256sum "$bin_dir/localup.tar.gz" | awk '{print $1}')
    elif command -v shasum &> /dev/null; then
        actual_checksum=$(shasum -a 256 "$bin_dir/localup.tar.gz" | awk '{print $1}')
    fi

    if [[ -n "$actual_checksum" ]]; then
        if [[ "$actual_checksum" = "$expected_checksum" ]]; then
            success "Checksum verified"
        else
            error "Checksum mismatch! Expected: $expected_checksum, Got: $actual_checksum"
        fi
    else
        info "sha256sum not found, skipping verification"
    fi
else
    info "Checksums not available for verification"
fi

# Extract
info "Extracting..."
tar -xzf "$bin_dir/localup.tar.gz" -C "$bin_dir" ||
    error 'Failed to extract localup'

chmod +x "$bin_dir/localup" ||
    error 'Failed to set permissions on localup executable'

rm -f "$bin_dir/localup.tar.gz"

tildify() {
    if [[ $1 = $HOME/* ]]; then
        local replacement=\~/
        echo "${1/$HOME\//$replacement}"
    else
        echo "$1"
    fi
}

echo
success "localup $LOCALUP_VERSION was installed successfully to $Bold_Green$(tildify "$bin_dir")"

# Detect shell and config file
refresh_command=''
shell_config=''

case "$(basename "${SHELL:-}")" in
    zsh)
        shell_config="$HOME/.zshrc"
        ;;
    bash)
        if [[ -f "$HOME/.bashrc" ]]; then
            shell_config="$HOME/.bashrc"
        elif [[ -f "$HOME/.bash_profile" ]]; then
            shell_config="$HOME/.bash_profile"
        else
            shell_config="$HOME/.bashrc"
        fi
        ;;
    fish)
        shell_config="$HOME/.config/fish/config.fish"
        ;;
    *)
        shell_config=""
        ;;
esac

path_export="export PATH=\"$bin_dir:\$PATH\""

# Add to shell config if not already present
if [[ -n "$shell_config" ]]; then
    # Create config file if it doesn't exist
    if [[ ! -f "$shell_config" ]]; then
        mkdir -p "$(dirname "$shell_config")"
        touch "$shell_config"
    fi

    # Check if PATH is already configured
    if ! grep -q "$bin_dir" "$shell_config" 2>/dev/null; then
        echo "" >> "$shell_config"
        echo "# Localup" >> "$shell_config"
        echo "$path_export" >> "$shell_config"
        info "Added localup to PATH in $(tildify "$shell_config")"
    else
        info "localup is already in your PATH configuration"
    fi

    refresh_command="exec \$SHELL"
fi

# Check if localup is already in PATH
localup_already_in_path=false
if command -v localup &> /dev/null; then
    localup_already_in_path=true
fi

# Export PATH for current session
export PATH="$bin_dir:$PATH"

echo
if [[ "$localup_already_in_path" = true ]]; then
    echo -e "${Bold_Green}localup is ready to use!${Color_Off}"
elif [[ -t 0 ]]; then
    # stdin is a terminal (not piped), safe to exec
    echo -e "${Bold_Green}localup is ready to use!${Color_Off}"
    exec $SHELL
elif [[ -n "$refresh_command" ]]; then
    echo -e "${Bold_White}Run this command to start using localup:${Color_Off}"
    echo
    echo -e "  ${Bold_Green}$refresh_command${Color_Off}"
    echo
    info "(or open a new terminal window)"
else
    info "Add this to your shell config:"
    info_bold "  $path_export"
    echo
    info "Then restart your terminal."
fi

echo
echo -e "${Bold_White}Quick start:${Color_Off}"
echo
echo -e "  ${Dim}# Start relay server${Color_Off}"
echo -e "  ${Bold_Green}localup relay${Color_Off}"
echo
echo -e "  ${Dim}# Create HTTP tunnel (in another terminal)${Color_Off}"
echo -e "  ${Bold_Green}localup http --port 3000 --relay localhost:4443${Color_Off}"
echo
echo -e "  ${Dim}# View all commands${Color_Off}"
echo -e "  ${Bold_Green}localup --help${Color_Off}"
echo

}
