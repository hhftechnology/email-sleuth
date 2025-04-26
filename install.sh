#!/bin/bash
# Install script for email-sleuth
# Usage: curl -fsSL https://raw.githubusercontent.com/tokenizer-decode/email-sleuth/main/install.sh | bash

set -e

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

DEFAULT_INSTALL_DIR="$HOME/.local/bin"
REPO_OWNER="tokenizer-decode"
REPO_NAME="email-sleuth"
BINARY_NAME="email-sleuth"
GITHUB_RELEASE_URL="https://github.com/$REPO_OWNER/$REPO_NAME/releases"
LATEST_RELEASE_URL="$GITHUB_RELEASE_URL/latest"

echo -e "${BLUE}============================================${NC}"
echo -e "${BLUE}          Email Sleuth Installer           ${NC}"
echo -e "${BLUE}============================================${NC}"
echo ""

check_command() {
    if ! command -v "$1" &> /dev/null; then
        echo -e "${RED}Error: $1 is required but not installed.${NC}"
        exit 1
    fi
}

check_command curl
check_command grep
check_command tar

detect_platform() {
    OS=$(uname -s | tr '[:upper:]' '[:lower:]')
    ARCH=$(uname -m)
    
    if [[ "$OS" == "darwin" ]]; then
        OS="apple-darwin"
    elif [[ "$OS" == "linux" ]]; then
        OS="unknown-linux-gnu"
    else
        echo -e "${RED}Error: Unsupported operating system: $OS${NC}"
        echo "This installer supports macOS and Linux only."
        echo "For Windows, please download the binary manually from:"
        echo "$GITHUB_RELEASE_URL"
        exit 1
    fi
    
    if [[ "$ARCH" == "x86_64" ]]; then
        ARCH="x86_64"
    elif [[ "$ARCH" == "aarch64" || "$ARCH" == "arm64" ]]; then
        ARCH="aarch64"
    else
        echo -e "${RED}Error: Unsupported architecture: $ARCH${NC}"
        echo "This installer supports x86_64 and ARM64 architectures only."
        echo "Please download the binary manually from:"
        echo "$GITHUB_RELEASE_URL"
        exit 1
    fi
    
    PLATFORM="${ARCH}-${OS}"
    echo -e "${GREEN}Detected platform: $PLATFORM${NC}"
}

get_latest_version() {
    echo -e "${BLUE}Fetching latest release information...${NC}"
    
    LATEST_VERSION=$(curl -s -L \
        -H "Accept: application/vnd.github+json" \
        "$LATEST_RELEASE_URL" | 
        grep -o 'tag/v[0-9]*\.[0-9]*\.[0-9]*' | 
        head -n 1 | 
        cut -d '/' -f2)
    
    if [[ -z "$LATEST_VERSION" ]]; then
        echo -e "${YELLOW}API extraction failed, trying alternate method...${NC}"
        LATEST_VERSION=$(curl -s -L "$GITHUB_RELEASE_URL" | 
            grep -o '/releases/tag/v[0-9]*\.[0-9]*\.[0-9]*' | 
            head -n 1 | 
            cut -d '/' -f4)
    fi
    
    if [[ -z "$LATEST_VERSION" ]]; then
        echo -e "${YELLOW}Still unable to find version. Listing available releases...${NC}"
        curl -s -L "$GITHUB_RELEASE_URL" > /tmp/releases.html
        
        echo "Debug: Available releases:"
        grep -o '/releases/tag/[^"]*' /tmp/releases.html | sort -u
        
        LATEST_VERSION=$(grep -o '/releases/tag/[^"]*' /tmp/releases.html | 
            sort -u | head -n 1 | cut -d '/' -f4)
        
        if [[ -z "$LATEST_VERSION" ]]; then
            echo -e "${RED}Error: Could not determine any release version.${NC}"
            echo "Please check $GITHUB_RELEASE_URL manually and specify a version."
            exit 1
        fi
    fi
    
    echo -e "${GREEN}Latest version: $LATEST_VERSION${NC}"
}

download_binary() {
    local DOWNLOAD_URL
    local ARCHIVE_NAME="${BINARY_NAME}-${LATEST_VERSION}-${PLATFORM}"
    
    if [[ "$OS" == "apple-darwin" ]]; then
        ARCHIVE_NAME="${ARCHIVE_NAME}.zip"
    else
        ARCHIVE_NAME="${ARCHIVE_NAME}.tar.gz"
    fi
    
    DOWNLOAD_URL="$GITHUB_RELEASE_URL/download/$LATEST_VERSION/$ARCHIVE_NAME"
    
    echo -e "${BLUE}Downloading $ARCHIVE_NAME...${NC}"
    echo -e "From: $DOWNLOAD_URL"
    
    TMP_DIR=$(mktemp -d)
    pushd "$TMP_DIR" > /dev/null
    
    if ! curl -L --progress-bar -o "$ARCHIVE_NAME" "$DOWNLOAD_URL"; then
        echo -e "${RED}Error: Failed to download $ARCHIVE_NAME${NC}"
        echo "Available files on $GITHUB_RELEASE_URL/download/$LATEST_VERSION/ :"
        
        ASSETS_URL="$GITHUB_RELEASE_URL/expanded_assets/$LATEST_VERSION"
        curl -s -L "$ASSETS_URL" | grep -o "$LATEST_VERSION/[^\"]*" | sort -u
        
        popd > /dev/null
        rm -rf "$TMP_DIR"
        exit 1
    fi
    
    echo -e "${BLUE}Extracting...${NC}"
    if [[ "$ARCHIVE_NAME" == *.zip ]]; then
        if ! command -v unzip &> /dev/null; then
            echo -e "${RED}Error: 'unzip' command not found. Please install it and try again.${NC}"
            popd > /dev/null
            rm -rf "$TMP_DIR"
            exit 1
        fi
        unzip -q "$ARCHIVE_NAME"
    else
        tar xzf "$ARCHIVE_NAME"
    fi
    
    if [[ ! -f "$BINARY_NAME" ]]; then
        echo -e "${YELLOW}Binary not found with expected name, searching...${NC}"
        find . -type f -executable -print
        
        FOUND_BINARY=$(find . -type f -executable | head -n 1)
        
        if [[ -z "$FOUND_BINARY" ]]; then
            echo -e "${RED}Error: No executable found in the archive.${NC}"
            echo "Archive contents:"
            ls -la
            popd > /dev/null
            rm -rf "$TMP_DIR"
            exit 1
        else
            echo -e "${GREEN}Found executable: $FOUND_BINARY${NC}"
            cp "$FOUND_BINARY" "$BINARY_NAME"
        fi
    fi
    
    chmod +x "$BINARY_NAME"
    
    popd > /dev/null
}

create_wrapper_script() {
    echo -e "${BLUE}Creating wrapper script...${NC}"
    local WRAPPER="$TMP_DIR/email-sleuth-cli.sh"
    
    cat > "$WRAPPER" << 'EOL'
#!/bin/bash
# email-sleuth-cli.sh - Simple wrapper for email-sleuth

# Find the email-sleuth binary
find_binary() {
    if [ -f "$(dirname "$0")/email-sleuth" ]; then
        echo "$(dirname "$0")/email-sleuth"
    elif command -v email-sleuth &> /dev/null; then
        command -v email-sleuth
    else
        echo "Error: email-sleuth not found in PATH or script directory" >&2
        exit 1
    fi
}

EMAIL_SLEUTH=$(find_binary)

# Show help if requested
if [ "$1" = "-h" ] || [ "$1" = "--help" ]; then
    echo "Usage: $(basename "$0") [OPTIONS] NAME DOMAIN"
    echo ""
    echo "Find email addresses using name and domain."
    echo ""
    echo "Arguments:"
    echo "  NAME                 Person's name (e.g., \"John Doe\")"
    echo "  DOMAIN               Company domain (e.g., example.com)"
    echo ""
    echo "Options:"
    echo "  -h, --help           Show this help message and exit"
    echo "  -o, --output FILE    Save results to file instead of stdout"
    echo "  -v, --version        Show version information and exit"
    exit 0
fi

# Show version if requested
if [ "$1" = "-v" ] || [ "$1" = "--version" ]; then
    "$EMAIL_SLEUTH" --version
    exit 0
fi

# Parse output option
OUTPUT_FILE=""
if [ "$1" = "-o" ] || [ "$1" = "--output" ]; then
    if [ -z "$2" ]; then
        echo "Error: Missing filename for output option" >&2
        exit 1
    fi
    OUTPUT_FILE="$2"
    shift 2
fi

# Check for NAME and DOMAIN
if [ $# -lt 2 ]; then
    echo "Error: NAME and DOMAIN are required" >&2
    echo "Try '$(basename "$0") --help' for more information." >&2
    exit 1
fi

NAME="$1"
DOMAIN="$2"

# Build and execute command
CMD="\"$EMAIL_SLEUTH\" --name \"$NAME\" --domain \"$DOMAIN\""

if [ -n "$OUTPUT_FILE" ]; then
    CMD="$CMD --output \"$OUTPUT_FILE\""
else 
    CMD="$CMD --stdout true"
fi

echo "Searching for email: $NAME at $DOMAIN..."
eval $CMD
EOL
    
    chmod +x "$WRAPPER"
}

install_files() {
    INSTALL_DIR="$DEFAULT_INSTALL_DIR"
    
    if [[ "$EUID" -eq 0 ]]; then
        # If running as root, install to /usr/local/bin
        INSTALL_DIR="/usr/local/bin"
    else
        # Create user bin directory if it doesn't exist
        mkdir -p "$INSTALL_DIR"
    fi
    
    echo -e "${BLUE}Installing to $INSTALL_DIR...${NC}"
    
    cp "$TMP_DIR/$BINARY_NAME" "$INSTALL_DIR/"
    cp "$TMP_DIR/email-sleuth-cli.sh" "$INSTALL_DIR/"
    
    chmod +x "$INSTALL_DIR/$BINARY_NAME"
    chmod +x "$INSTALL_DIR/email-sleuth-cli.sh"
    
    # Create symlink for convenience
    if [[ ! -f "$INSTALL_DIR/es" ]]; then
        ln -s "$INSTALL_DIR/email-sleuth-cli.sh" "$INSTALL_DIR/es"
        echo -e "${GREEN}Created shortcut: es${NC}"
    fi
}

check_path() {
    if [[ ":$PATH:" != *":$INSTALL_DIR:"* ]]; then
        echo -e "${YELLOW}Warning: $INSTALL_DIR is not in your PATH.${NC}"
        
        SHELL_NAME=$(basename "$SHELL")
        case "$SHELL_NAME" in
            bash)
                echo -e "Add this line to your ~/.bashrc or ~/.bash_profile:"
                echo -e "  ${BLUE}export PATH=\"\$PATH:$INSTALL_DIR\"${NC}"
                ;;
            zsh)
                echo -e "Add this line to your ~/.zshrc:"
                echo -e "  ${BLUE}export PATH=\"\$PATH:$INSTALL_DIR\"${NC}"
                ;;
            *)
                echo -e "Add $INSTALL_DIR to your PATH to use email-sleuth and es commands from anywhere."
                ;;
        esac
        
        echo -e "\nOr you can run the tools directly using their full path:"
        echo -e "  ${BLUE}$INSTALL_DIR/email-sleuth${NC}"
        echo -e "  ${BLUE}$INSTALL_DIR/email-sleuth-cli.sh${NC}"
        echo -e "  ${BLUE}$INSTALL_DIR/es${NC}"
    fi
}

cleanup() {
    if [[ -d "$TMP_DIR" ]]; then
        rm -rf "$TMP_DIR"
    fi
}

show_success() {
    echo -e "\n${GREEN} Email Sleuth installed successfully!${NC}"
    echo -e "\n${BLUE}Quick Start:${NC}"
    echo -e "  ${YELLOW}es --name \"John Doe\" --domain example.com${NC}"
    echo -e "  ${YELLOW}es -o results.json \"Jane Smith\" company.com${NC}"
    echo -e "\n${BLUE}For more help:${NC}"
    echo -e "  ${YELLOW}es --help${NC}"
    echo -e "  ${YELLOW}email-sleuth --help${NC}"
    echo -e "\n${BLUE}Latest Version:${NC} ${YELLOW}$LATEST_VERSION${NC}"
}

main() {
    detect_platform
    get_latest_version
    download_binary
    create_wrapper_script
    install_files
    check_path
    show_success
    cleanup
}

trap cleanup EXIT

main