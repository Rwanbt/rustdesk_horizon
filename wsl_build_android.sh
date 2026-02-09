#!/bin/bash
set -e

export HOME="/home/aes"
source "$HOME/.cargo/env"

echo "=== [1/5] Setting up Android NDK ==="
ANDROID_NDK_HOME="$HOME/android-ndk"
if [ -d "$ANDROID_NDK_HOME/toolchains" ]; then
    echo "NDK already installed"
else
    cd /tmp
    echo "Downloading NDK r27c (~1.7GB)..."
    curl -L --progress-bar -o ndk.zip "https://dl.google.com/android/repository/android-ndk-r27c-linux.zip"
    echo "Extracting..."
    unzip -q ndk.zip
    rm -rf "$ANDROID_NDK_HOME"
    mv android-ndk-r27c "$ANDROID_NDK_HOME"
    rm -f ndk.zip
    echo "NDK installed"
fi
export ANDROID_NDK_HOME
echo "NDK: $ANDROID_NDK_HOME"

echo "=== [2/5] Setting up vcpkg ==="
VCPKG_ROOT="$HOME/vcpkg"
if [ ! -f "$VCPKG_ROOT/vcpkg" ]; then
    git clone --depth 1 https://github.com/microsoft/vcpkg.git "$VCPKG_ROOT"
    "$VCPKG_ROOT/bootstrap-vcpkg.sh" -disableMetrics
fi
export VCPKG_ROOT
echo "vcpkg: $VCPKG_ROOT"

echo "=== [3/5] Installing vcpkg Android deps ==="
cd /mnt/d/App/Fulldesk
"$VCPKG_ROOT/vcpkg" install --triplet arm64-android --x-install-root="$VCPKG_ROOT/installed"
echo "Libs installed:"
ls "$VCPKG_ROOT/installed/arm64-android/lib/"

echo "=== [4/5] Building Rust library for Android ==="
cd /mnt/d/App/Fulldesk
cargo ndk --platform 21 --target aarch64-linux-android build --release --features flutter
echo "Build complete"

echo "=== [5/5] Copying .so to jniLibs ==="
JNILIBS="/mnt/d/App/Fulldesk/flutter/android/app/src/main/jniLibs/arm64-v8a"
mkdir -p "$JNILIBS"
cp /mnt/d/App/Fulldesk/target/aarch64-linux-android/release/liblibrustdesk.so "$JNILIBS/librustdesk.so"
cp "$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/sysroot/usr/lib/aarch64-linux-android/libc++_shared.so" "$JNILIBS/"

echo "=== ALL DONE ==="
ls -la "$JNILIBS/"
