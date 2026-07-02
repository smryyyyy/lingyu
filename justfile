# 灵语（LingYu）— 构建 & 运行辅助

# Install system dependencies (GTK4)
deps:
    #!/usr/bin/env bash
    set -euo pipefail
    if [[ "$(uname)" == "Darwin" ]]; then
        if ! brew list gtk4 &>/dev/null; then
            echo "Installing gtk4 via Homebrew..."
            brew install gtk4 pkgconf cmake
        else
            echo "✓ gtk4 already installed."
        fi
    elif [[ "$(uname -o)" == "Msys" || "$(uname -o)" == "Cygwin" ]]; then
        echo "Running inside MSYS2 — checking GTK4..."
        if ! pacman -Q mingw-w64-ucrt-x86_64-gtk4 &>/dev/null; then
            echo "Installing GTK4 via pacman..."
            pacman -S --noconfirm --needed mingw-w64-ucrt-x86_64-gtk4 mingw-w64-ucrt-x86_64-pkgconf
        else
            echo "✓ gtk4 already installed."
        fi
    elif [[ "$(uname -s)" == MINGW* || "$(uname -s)" == MSYS* ]]; then
        echo "Running inside MSYS2 — checking GTK4..."
        if ! pacman -Q mingw-w64-ucrt-x86_64-gtk4 &>/dev/null; then
            echo "Installing GTK4 via pacman..."
            pacman -S --noconfirm --needed mingw-w64-ucrt-x86_64-gtk4 mingw-w64-ucrt-x86_64-pkgconf
        else
            echo "✓ gtk4 already installed."
        fi
    else
        echo "Unsupported platform. Please install GTK4 manually."
        echo "macOS: brew install gtk4 pkgconf cmake"
        echo "Windows: install MSYS2 (https://www.msys2.org/) then run this script inside UCRT64 terminal"
        exit 1
    fi

# Build release binary
build: deps
    cargo build --release

# Run
run: deps
    cargo run
