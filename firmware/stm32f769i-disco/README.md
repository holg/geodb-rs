# geodb on the STM32F769I-DISCO

The 153,312 cities of the web demo, in the chip's flash and queried in place.

```text
crates/geodb-fw-core     no_std core: the flash image (zero-copy), radius / nearest
                         queries on Z-order ranges, a software globe, the whole
                         800 x 480 screen as one function
firmware/stm32f769i-disco  this crate: embassy-stm32 on the board
```

## Build and flash

```sh
# 1. the image (1.49 MB: geoids, country ids, ~36k names, a 256 x 128 earth) and
#    previews of the screen (preview-*.png: what the LCD draws)
cargo run --release -p geodb-fw-core --features std --example make_image

# 2. build, flash over the ST-LINK and read the RTT log
cd firmware/stm32f769i-disco
cargo run --release          # probe-rs run --chip STM32F769NI
```

Needs `probe-rs` and the target (`rustup target add thumbv7em-none-eabihf`).
Only the internal flash is written (about 1.5 MB of the 2 MB); the QSPI flash
and the option bytes are not touched.

## Back to the factory demo

`backup/factory-flash-2MB.bin` (git-ignored) is the board's internal flash as
it was before the first flash of this firmware:

```sh
st-flash write backup/factory-flash-2MB.bin 0x08000000
```

## Measured on the chip (216 MHz, caches on)

| | |
|---|---|
| 10 nearest cities | 0.13 - 0.7 ms |
| all cities within 300 km of Munich (18,296 candidates) | 22 ms |
| the same around Tokyo (1,074 candidates) | 1.3 ms |
| globe, 240 x 240 | 199 ms |

The queries answer exactly like `geodb-core` (`make_image` checks 7 radius
queries: identical counts; nearest within 0.021 km).

Two things that made it 20x faster: the Cortex-M7 runs from flash (7 wait
states at 216 MHz), so the I- and D-cache must be on; and `libm`'s sinf / cosf /
atan2f compute in software double precision on this single-precision-FPU
target, so `fmath` has f32-only versions.
