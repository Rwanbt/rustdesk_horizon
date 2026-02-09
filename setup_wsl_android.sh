#!/bin/bash
set -e

export HOME="/home/aes"
source "$HOME/.cargo/env"

echo "=== Installing build dependencies ==="
sudo apt-get update -qq
sudo apt-get install -y -qq build-essential pkg-config libclang-dev unzip curl cmake nasm perl > /dev/null 2>&1
echo "=== Deps installed ==="

# Download Linux NDK (Windows NDK won't work for Linux cross-compilation)
echo "=== Setting up Android NDK ==="
ANDROID_NDK_HOME="$HOME/android-ndk"
if [ -d "$ANDROID_NDK_HOME/toolchains" ]; then
    echo "NDK already installed at $ANDROID_NDK_HOME"
else
    echo "Downloading Android NDK r27c..."
    cd /tmp
    curl -L -o ndk.zip "https://dl.google.com/android/repository/android-ndk-r27c-linux.zip"
    echo "Extracting NDK..."
    unzip -q ndk.zip
    rm -rf "$ANDROID_NDK_HOME"
    mv android-ndk-r27c "$ANDROID_NDK_HOME"
    rm ndk.zip
    echo "NDK installed at $ANDROID_NDK_HOME"
fi
export ANDROID_NDK_HOME

# Setup vcpkg
echo "=== Setting up vcpkg ==="
VCPKG_ROOT="$HOME/vcpkg"
if [ ! -f "$VCPKG_ROOT/vcpkg" ]; then
    git clone --depth 1 https://github.com/microsoft/vcpkg.git "$VCPKG_ROOT"
    "$VCPKG_ROOT/bootstrap-vcpkg.sh" -disableMetrics
fi
export VCPKG_ROOT

# Install Android vcpkg deps
echo "=== Installing vcpkg Android packages ==="
cd /mnt/d/App/Fulldesk
"$VCPKG_ROOT/vcpkg" install --triplet arm64-android --x-install-root="$VCPKG_ROOT/installed"

echo "=== All setup complete ==="
echo "ANDROID_NDK_HOME=$ANDROID_NDK_HOME"
echo "VCPKG_ROOT=$VCPKG_ROOT"
ls "$VCPKG_ROOT/installed/arm64-android/lib/" 2>/dev/null
