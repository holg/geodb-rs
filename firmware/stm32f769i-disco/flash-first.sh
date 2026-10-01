#!/usr/bin/env bash
# The first flash with the ST-LINK: bootloader, program slot A and the city image. Afterwards updates
# go over Ethernet (`geodb-board ota`), and `cargo run --release` flashes slot A only (the board then
# starts through the bootloader as before).
#
# The whole INTERNAL flash is erased (st-flash mass erase); the QSPI flash and the option bytes are
# not touched.
set -euo pipefail
cd "$(dirname "$0")"
CHIP=STM32F769NIHx
SPEED=500

(cd ../bootloader && cargo build --release)
./build-ota.sh
(cd ../.. && cargo run --release -q -p geodb-fw-core --features std --example make_image >/dev/null)

st-flash --connect-under-reset erase
probe-rs download --chip $CHIP --speed $SPEED ../bootloader/target/thumbv7em-none-eabihf/release/geodb-bootloader
probe-rs download --chip $CHIP --speed $SPEED target-a/thumbv7em-none-eabihf/release/geodb-firmware
probe-rs download --chip $CHIP --speed $SPEED --binary-format bin --base-address 0x080C0000 geodb.fw
probe-rs reset --chip $CHIP --speed $SPEED
echo "flashed: bootloader, slot A, image. The board starts slot A; its LCD shows the globe."
