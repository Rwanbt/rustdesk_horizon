#!/bin/bash
# Install the newly compiled binary to replace the installed one
set -e

BUNDLE="/mnt/Stockage/App/Fulldesk/flutter/build/linux/x64/release/bundle"
INSTALL_DIR="/usr/share/rustdesk"

echo "=== Stopping rustdesk service ==="
sudo systemctl stop rustdesk

echo "=== Copying new binary and libraries ==="
sudo cp "$BUNDLE/rustdesk" "$INSTALL_DIR/rustdesk"
sudo cp "$BUNDLE/lib/librustdesk.so" "$INSTALL_DIR/lib/librustdesk.so"
sudo cp "$BUNDLE/lib/libapp.so" "$INSTALL_DIR/lib/libapp.so"

echo "=== Restarting rustdesk service ==="
sudo systemctl start rustdesk

echo "=== Verifying ==="
sleep 2
systemctl is-active rustdesk
echo ""
echo "Done! Check logs with: journalctl -u rustdesk -f"
