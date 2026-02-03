#!/bin/bash
set -e

APP_NAME="rust-ime"
RELEASE_DIR="release_pkg"
ARCHIVE_NAME="${APP_NAME}-linux-x64.tar.gz"

echo "📦 开始打包发行版..."

# 1. 编译程序和词库
echo "🔨 正在进行 Release 编译..."
cargo build --release
echo "📚 正在预编译词库..."
cargo run --release --bin compile_dict

# 2. 创建打包目录
echo "📂 正在收集文件..."
rm -rf "$RELEASE_DIR"
mkdir -p "$RELEASE_DIR"

# 复制二进制文件
cp target/release/rust-ime "$RELEASE_DIR/"
# 复制安装脚本和指南
cp install.sh "$RELEASE_DIR/"
cp rust-ime.desktop "$RELEASE_DIR/"
cp INSTALL_GUIDE_ZH.md "$RELEASE_DIR/安装说明.md"
# 复制必要的资源目录
cp -r dicts "$RELEASE_DIR/"
cp -r static "$RELEASE_DIR/"
cp -r data "$RELEASE_DIR/" # 包含编译好的二进制词库
cp -r picture "$RELEASE_DIR/" # 包含图标

# 3. 创建压缩包
echo "🗜️ 正在生成压缩包..."
mkdir -p releases
tar -czf "releases/$ARCHIVE_NAME" -C "$RELEASE_DIR" .

# 4. 清理
rm -rf "$RELEASE_DIR"

echo -e "\n✅ 打包完成！"
echo "📦 发行版文件: $(pwd)/releases/$ARCHIVE_NAME"
echo "💡 用户只需下载并解压该包，运行 './install.sh' 即可完成安装。"
