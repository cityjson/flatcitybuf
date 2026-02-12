#!/bin/bash
# Claude Code Plugin Installer for Dev Container
# Installs plugins in the devcontainer's independent Claude config

set -e

GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

echo -e "${GREEN}Setting up Claude Code plugins in devcontainer...${NC}"

cd "$(dirname "$0")/.."

# Add marketplace if not present
if ! claude plugin marketplace list 2>/dev/null | grep -q "claude-plugins-official"; then
    echo -e "${YELLOW}Adding official plugins marketplace...${NC}"
    claude plugin marketplace add anthropics/claude-plugins-official
fi

# Update marketplace
echo -e "${YELLOW}Updating marketplace...${NC}"
claude plugin marketplace update 2>/dev/null || true

# Install plugins
for plugin in "$@"; do
    if claude plugin list 2>/dev/null | grep -q "$(echo "$plugin" | cut -d'@' -f1)"; then
        echo -e "${GREEN}Already installed: $plugin${NC}"
    else
        echo -e "${YELLOW}Installing: $plugin${NC}"
        claude plugin install "$plugin" || echo "Failed to install $plugin"
    fi
done

echo -e "${GREEN}Done!${NC}"
