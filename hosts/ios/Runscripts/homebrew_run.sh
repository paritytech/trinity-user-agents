#!/bin/bash

# Common setup
setup_environment() {
    if [ "$ACTION" == "indexbuild" ]; then
        echo "Skip for indexbuild"
        exit 0
    fi
    export PATH="/opt/homebrew/bin:/usr/local/bin:$PATH"
}

# Check if Homebrew is available
check_homebrew() {
    if [ ! -x "/opt/homebrew/bin/brew" ] && [ ! -x "/usr/local/bin/brew" ]; then
        echo "No Homebrew found"
        echo "Run: /bin/bash -c \"\$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)\""
        exit 1
    fi
}

# Generic tool handler
handle_tool() {
    local tool_name="$1"
    local tool_arguments="$2"
    local github_url="$3"

    if command -v "$tool_name" >/dev/null 2>&1; then
        echo "$tool_name found. Upgrading and running..."
        eval "$tool_name $tool_arguments"
    else
        echo "$tool_name not installed. Attempting to install via Homebrew..."
        check_homebrew
        
        echo "Installing $tool_name..."
        brew install "$tool_name"
        
        if command -v "$tool_name" >/dev/null 2>&1; then
            echo "$tool_name installed successfully."
            eval "$tool_name $tool_arguments"
        else
            echo "error: Failed to install $tool_name. Please install manually from $github_url"
            exit 1
        fi
    fi
}
