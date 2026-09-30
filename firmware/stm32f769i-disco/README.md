# geodb on the STM32F769I-DISCO

The 153,312 cities of the web demo, in the chip's flash and queried in place,
with the globe, the nearest cities and touch control on the 4" 800 x 480 LCD.

```text
crates/geodb-fw-core     no_std core: the flash image (zero-copy), radius / nearest
                         queries on Z-order ranges, a software globe, the whole
                         800 x 480 screen as one function, taps and drags
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

## The screen

Touch: a **drag** pans the globe, a **tap on the globe** looks at that point
(and zooms in). Buttons: `SPIN` (turns the globe; shows `STOP` while on), `-` / `+`
(spin slower / faster, x1.5 per tap, 1..180 deg/s; `+` also starts the spin),
`Z-` / `Z+` (zoom), `WORLD`. Spinning uses a per-tilt table in SDRAM
(`render::GlobeLut`): the spin only shifts the longitude, so a frame is integer
texture sampling and the panel refresh paces it. Zoomed in (view under about
1000 km) the coarse earth picture says nothing and the screen switches to a
*scope*: distance rings, every city as a dot, the ten nearest labelled. The
whole screen is `geodb_fw_core::ui::draw`, so `make_image` renders exactly what
the LCD shows to `preview-globe.png`, `preview-scope.png`, `preview-world.png`.

The display bring-up (SDRAM on the FMC, MPU attribute, DSI PHY and video
timing, panel detection over DSI, NT35510 / OTM8009A init, backlight PI14,
FT6206 touch on I2C4) is the one proven on this board in the plantworks
gateway (`../plantworks/firmware/plantworks-gw-stm32`, `display.rs`, `touch.rs`)
with its gotchas: embassy from the same pinned git revision (`dsihost` is not on
crates.io), the SDRAM mapped Normal / non-cacheable, `LTDC` pixel clock 27.43 MHz
from PLLSAI. Here the LTDC layer is RGB565 (768 KB per frame, two in the 16 MB
SDRAM) instead of ARGB8888.

## Measured on the chip (216 MHz, caches on)

| | |
|---|---|
| 10 nearest cities | 0.13 - 0.7 ms |
| all cities within 300 km of Munich (18,296 candidates) | 22 ms |
| the same around Tokyo (1,074 candidates) | 1.3 ms |
| a whole 800 x 480 screen, world view | 272 ms |
| a whole screen, scope view | see the RTT log (`screen ... drawn in`) |

The queries answer exactly like `geodb-core` (`make_image` checks 7 radius
queries: identical counts; nearest within 0.021 km).

What made it fast: the Cortex-M7 runs from flash (7 wait states at 216 MHz), so
the I- and D-cache must be on; `libm`'s sinf / cosf / atan2f compute in software
double precision on this single-precision-FPU target, so `fmath` has f32-only
versions; the globe is computed per 2 x 2 pixel block (the texture is 256 x 128);
and a wide view neither counts nor draws its 80,000 dots.

## Names from the host

Flash holds names for about a third of the cities (by population). For the
others the board asks the host over the ST-LINK's virtual COM port (USART1,
115200 8N1): `?LAT,LON` (degrees x 1e5) and the script answers
`=LAT,LON|Name|State, Country` from the full dataset. Run it while the board
runs (it needs `pyserial`):

    python3 firmware/stm32f769i-disco/scripts/serve_names.py

The board asks when the globe comes to rest, for the listed cities without a
name, and shows the answers in the list (the nearest city's state and country in
the footer). Without the script the list shows `(unnamed)` and the board asks
again only every 5 s.
