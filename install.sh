#!/bin/bash

# Exit immediately if a command exits with a non-zero status
set -e

echo "=== Rust-IME Auto Installer ==="

# 0. Check Rust environment
if ! command -v cargo &> /dev/null;
then
    echo "❌ Error: Rust/Cargo environment not found"
    echo "Please install Rust first: curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
    exit 1
fi

# 1. Install Dependencies
echo -e "\n[1/4] Installing system dependencies..."
if [ -f /etc/debian_version ]; then
    # Detect Debian/Ubuntu/Pop!_OS
    echo "Debian-based system detected, installing via apt..."
    sudo apt-get update
    sudo apt-get install -y libxcb-composite0-dev libx11-dev libdbus-1-dev build-essential pkg-config clang gtk4-layer-shell
elif [ -f /etc/arch-release ]; then
    # Detect Arch Linux/Manjaro
    echo "Arch-based system detected, installing via pacman..."
    sudo pacman -S --noconfirm --needed base-devel pkgconf clang gtk4-layer-shell libxcb libx11 dbus
else
    echo "⚠️  Unknown package manager (not apt or pacman)"
    echo "Please ensure the following development libraries are installed:"
    echo "  - xcb, x11, dbus development files"
    echo "  - pkg-config, clang"
    echo "  - gtk4-layer-shell"
    read -p "Press Enter to continue..."
fi

# 2. Configure Permissions
echo -e "\n[2/4] Configuring user permissions..."
CURRENT_USER=$(whoami)

# Add to input group
if groups | grep -q "\binput\b"; then
    echo "✅ User '$CURRENT_USER' is already in the 'input' group"
else
    echo "Adding user '$CURRENT_USER' to 'input' group..."
    sudo usermod -aG input "$CURRENT_USER"
    echo "✅ Added (Logout/login required to take effect)"
fi

# Udev rules for uinput
echo "Configuring uinput device rules..."
if [ ! -f /etc/udev/rules.d/99-rust-ime-uinput.rules ]; then
    echo 'KERNEL=="uinput", GROUP="input", MODE="0660", OPTIONS+="static_node=uinput"' | sudo tee /etc/udev/rules.d/99-rust-ime-uinput.rules > /dev/null
    echo "✅ Rule file created"
    sudo udevadm control --reload-rules
    sudo udevadm trigger
else
    echo "✅ Rule file already exists"
fi

# 3. Build Project / Prepare Binary
echo -e "\n[3/4] Preparing program files..."
if [ -f "./rust-ime" ]; then
    echo "✅ Precompiled binary detected, skipping build step."
    chmod +x ./rust-ime
else
    echo "Precompiled binary not found, attempting to build from source..."
    if ! command -v cargo &> /dev/null; then
        echo "❌ Error: Rust environment not found and no precompiled binary available."
        echo "Please install Rust or use a release package."
        exit 1
    fi
    cargo build --release
    cp target/release/rust-ime .
fi

# 3.5 Compile Dictionaries
echo -e "\n[3.5/4] Preparing dictionaries..."
if [ -d "./data" ] && [ "$(ls -A ./data 2>/dev/null)" ]; then
    echo "✅ Dictionaries already exist."
else
    echo "Compiling raw dictionaries (this may take a few seconds)..."
    if command -v cargo &> /dev/null; then
        cargo run --release --bin compile_dict
    else
        # If it's a binary package, we should ensure it has compiled data or run compilation via binary
        ./rust-ime --compile-dict || echo "⚠️ Warning: Failed to automatically compile dictionaries. Ensure 'data' directory is complete."
    fi
fi

# 4. Install
echo -e "\n[4/4] Executing installation..."
# Get absolute path
INSTALL_PATH=$(pwd)

# 4.1 Install binary
sudo cp -f "$INSTALL_PATH/rust-ime" /usr/local/bin/rust-ime
sudo chmod +x /usr/local/bin/rust-ime
echo "✅ Installed binary to: /usr/local/bin/rust-ime"

# 4.2 Install icon
ICON_DIR="/usr/share/icons/hicolor/256x256/apps"
sudo mkdir -p "$ICON_DIR"
if [ -f "$INSTALL_PATH/picture/rust-ime_v2.png" ]; then
    sudo cp -f "$INSTALL_PATH/picture/rust-ime_v2.png" "$ICON_DIR/rust-ime.png"
    echo "✅ Installed icon to: $ICON_DIR/rust-ime.png"
fi

# 4.3 Install desktop entry
APP_DIR="/usr/share/applications"
if [ -f "$INSTALL_PATH/rust-ime.desktop" ]; then
    sudo cp -f "$INSTALL_PATH/rust-ime.desktop" "$APP_DIR/rust-ime.desktop"
    sudo update-desktop-database "$APP_DIR" || true
    echo "✅ Installed desktop shortcut. You can now find Rust-IME in your application menu."
fi

# 4.4 Trigger first-time installation tasks
# Use the installed path
/usr/local/bin/rust-ime --install || true

echo -e "\n=========================================="
echo "🎉 Installation Complete!"
echo "You can start it by typing 'rust-ime' in the terminal."
echo "⚠️  Note: If this is your first time running this script and you were added to the 'input' group,"
echo "    you MUST 【logout and log back in】 (or restart) for it to work properly!"
echo "=========================================="
