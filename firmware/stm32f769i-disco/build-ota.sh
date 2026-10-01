#!/usr/bin/env bash
# Builds the program for both flash slots: app-a.bin (0x0804_0000) and app-b.bin (0x0808_0000).
# An update is the binary of the slot that is NOT running: `geodb-board ota app-b.bin --host IP`.
set -euo pipefail
cd "$(dirname "$0")"
for slot in a b; do
    GEODB_SLOT=$slot CARGO_TARGET_DIR=target-$slot cargo build --release
    GEODB_SLOT=$slot CARGO_TARGET_DIR=target-$slot cargo objcopy --release -- -O binary "app-$slot.bin"
    ls -l "app-$slot.bin"
done
