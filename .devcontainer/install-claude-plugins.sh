#!/bin/bash
# Claude Code Plugin Installer for Dev Container
# Installs plugins in the devcontainer's independent Claude config
#
# Usage: install-claude-plugins.sh [--marketplace <url>]... [plugin1 plugin2 ...]
#   --marketplace <url>  Add a custom plugin marketplace (can be specified multiple times)
#   plugin               Plugin name to install from any registered marketplace

set -e

GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

echo -e "${GREEN}Setting up Claude Code plugins in devcontainer...${NC}"

cd "$(dirname "$0")/.."

# Parse arguments: separate --marketplace flags from plugin names
MARKETPLACES=()
PLUGINS=()
while [[ $# -gt 0 ]]; do
    case "$1" in
        --marketplace)
            shift
            if [[ $# -gt 0 ]]; then
                MARKETPLACES+=("$1")
            else
                echo "Error: --marketplace requires a URL argument"
                exit 1
            fi
            ;;
        *)
            PLUGINS+=("$1")
            ;;
    esac
    shift
done

# Add official marketplace if not present
if ! claude plugin marketplace list 2>/dev/null | grep -q "claude-plugins-official"; then
    echo -e "${YELLOW}Adding official plugins marketplace...${NC}"
    claude plugin marketplace add anthropics/claude-plugins-official
fi

# Add custom marketplaces
for marketplace in "${MARKETPLACES[@]}"; do
    echo -e "${YELLOW}Adding custom marketplace: $marketplace${NC}"
    claude plugin marketplace add "$marketplace" || echo "Failed to add marketplace $marketplace"
done

# Update all marketplaces
echo -e "${YELLOW}Updating marketplaces...${NC}"
claude plugin marketplace update 2>/dev/null || true

# Install plugins
for plugin in "${PLUGINS[@]}"; do
    if claude plugin list 2>/dev/null | grep -q "$(echo "$plugin" | cut -d'@' -f1)"; then
        echo -e "${GREEN}Already installed: $plugin${NC}"
    else
        echo -e "${YELLOW}Installing: $plugin${NC}"
        claude plugin install "$plugin" || echo "Failed to install $plugin"
    fi
done

echo -e "${GREEN}Done!${NC}"
