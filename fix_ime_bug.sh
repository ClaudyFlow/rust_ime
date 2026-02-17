#!/bin/bash

# Rust IME Bug Fix Script
# Main issue: uinput kernel module not loaded

echo "=== Rust IME Bug Fix ==="
echo "Checking and fixing known issues for Rust IME..."

# Check current user permissions
echo "1. Checking user permissions..."
if groups $USER | grep -q "input"; then
    echo "✓ User is already in 'input' group"
else
    echo "❌ User is not in 'input' group, please run:"
    echo "   sudo usermod -aG input,uinput $USER"
    echo "   Then logout and log back in."
fi

# Check uinput device
echo "2. Checking uinput device..."
if [ -e /dev/uinput ]; then
    echo "✓ uinput device exists"
else
    echo "❌ uinput device does not exist"
fi

# Check uinput module
echo "3. Checking uinput kernel module..."
if lsmod | grep -q "uinput"; then
    echo "✓ uinput module is loaded"
else
    echo "❌ uinput module is not loaded - this is a major issue!"
    echo ""
    echo "Please run the following command to fix it:"
    echo "   sudo modprobe uinput"
    echo ""
    echo "To make this change permanent, run:"
    echo "   echo 'uinput' | sudo tee /etc/modules-load.d/uinput.conf"
fi

# Check Wayland environment
echo "4. Checking display environment..."
if [ "$XDG_SESSION_TYPE" = "wayland" ]; then
    echo "✓ Wayland environment detected"
    echo "  Alternative: ydotool check"
    if command -v ydotool &> /dev/null; then
        echo "  ✓ ydotool is available"
        # Check ydotoold service
        if systemctl --user is-active --quiet ydotoold 2>/dev/null; then
            echo "  ✓ ydotoold service is running"
        else
            echo "  ⚠ ydotoold service is not running, you may need to start it manually:"
            echo "    systemctl --user enable --now ydotoold"
        fi
    else
        echo "  ❌ ydotool is not installed, please run:"
        echo "    sudo apt install ydotool"
    fi
else
    echo "✓ X11 environment, no additional configuration needed"
fi

# Check configuration file
echo "5. Checking configuration file..."
CONFIG_DIR="$HOME/.local/share/rust-ime"
PROJECT_CONFIG="./config.json"

if [ -f "$PROJECT_CONFIG" ]; then
    echo "✓ Project configuration file exists"
elif [ -f "$CONFIG_DIR/config.json" ]; then
    echo "✓ User configuration file exists"
else
    echo "⚠ Configuration file not found, will use default configuration"
fi

echo ""
echo "=== Recommended Fixes ==="
echo "1. Primary Fix (Required):"
echo "   sudo modprobe uinput"
echo "   echo 'uinput' | sudo tee /etc/modules-load.d/uinput.conf"
echo ""
echo "2. Restart Rust IME:"
echo "   rust-ime --stop"
echo "   rust-ime --foreground  # Run in foreground for testing"
echo ""
echo "3. If issues persist, check the logs:"
echo "   tail -f /tmp/rust-ime.log"
echo ""
echo "=== Common Issues & Solutions ==="
echo "• If clipboard is not working (Wayland):"
echo "  systemctl --user enable --now ydotoold"
echo ""
echo "• If permission is denied:"
echo "  sudo usermod -aG input,uinput \$USER"
echo "  # Logout and log back in"
echo ""
echo "• If keyboard device is not found:"
echo "  # Check keyboard device list:"
echo "  ls -la /dev/input/by-id/"
echo "  # Then set 'device_path' in config.json"
