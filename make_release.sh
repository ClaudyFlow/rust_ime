#!/bin/bash
set -e

APP_NAME="rust-ime"
RELEASE_DIR="release_pkg"
ARCHIVE_NAME="${APP_NAME}-linux-x64.tar.gz"

echo "📦 Starting release packaging..."

# 1. Compile program and dictionaries
echo "🔨 Building Release..."
cargo build --release
echo "📚 Pre-compiling dictionaries..."
cargo run --release -- --compile-only

# 2. Create packaging directory
echo "📂 Collecting files..."
rm -rf "$RELEASE_DIR"
mkdir -p "$RELEASE_DIR"

# Copy binary
cp target/release/rust-ime "$RELEASE_DIR/"
# Copy install script and desktop file
cp install.sh "$RELEASE_DIR/"
cp rust-ime.desktop "$RELEASE_DIR/"
cp INSTALL_GUIDE.md "$RELEASE_DIR/"
cp INSTALL_GUIDE_ZH.md "$RELEASE_DIR/"
# Copy necessary resource directories
cp -r dicts "$RELEASE_DIR/"
cp -r static "$RELEASE_DIR/"
cp -r data "$RELEASE_DIR/" # Contains compiled binary dictionaries
cp -r picture "$RELEASE_DIR/" # Contains icons

# 3. Create archive
echo "🗜️ Generating archive..."
mkdir -p releases
tar -czf "releases/$ARCHIVE_NAME" -C "$RELEASE_DIR" .

# 4. Cleanup
rm -rf "$RELEASE_DIR"

echo -e "\n✅ Packaging complete!"
echo "📦 Release file: $(pwd)/releases/$ARCHIVE_NAME"
echo "💡 Users can download, extract, and run './install.sh' to install."
