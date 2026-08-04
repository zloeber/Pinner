#!/usr/bin/env bash

# check for mise
if ! command -v ~/.local/bin/mise &>/dev/null; then
    echo "Please install mise first (run 'curl https://mise.run | sh')"
    echo ""
    echo "Then load mise into your path by adding it to your .bashrc or .zshrc (or .profile) file"
    echo "   echo 'eval \"\$(~/.local/bin/mise activate zsh)\"' >> ~/.zshrc"

    exit 1
else
    eval "$(~/.local/bin/mise activate bash)"
fi

# Run these if you use experimental features like automatic import of .env files
mise settings set experimental true
mise trust

# Install all dependencies
mise install -y
