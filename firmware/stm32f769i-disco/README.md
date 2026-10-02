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
`<>` (turn the other way round), `Z-` / `Z+` (zoom), `WORLD`. Spinning uses a per-tilt table in SDRAM
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
versions; the globe is computed per 2 x 2 pixel block (the earth is 2048 x 1024);
and a wide view neither counts nor draws its 80,000 dots.

## Names, tests and control from the host (Ethernet)

`geodb-board` (`crates/geodb-board`, Rust) is the host side. Every city in flash has a
name; the host adds the state and country of the nearest city (the footer line).

    cargo run --release -p geodb-board -- serve            # answers the board (UDP 7878), prints its log lines
    cargo run --release -p geodb-board -- test --host IP   # info, 200 pings, a 1000 x 512 byte blast
    cargo run --release -p geodb-board -- reset --host IP  # restarts the board over the network

The board gets an address by DHCP (the LCD's bottom line shows it) and broadcasts
`?LAT,LON` (degrees x 1e5); the server answers `=LAT,LON|Name|State, Country`.
Commands go to the board's UDP port 7880 (`!info`, `!ping N`, `!blast N`,
`!reset`, and `!tap X Y`, `!drag DX DY`, `!release` for the mirror window).
Without `--host` the client finds the board by its state packets (UDP 7881; not
while the mirror window is open: it holds that port) and then by broadcast, which
needs a single network interface, so pass `--host` when in doubt.

The Ethernet DMA cannot reach the DTCM and ignores the data cache, so only its
buffers (the stack's packet pool) are linked into SRAM1 at 0x20060000 (`ethbuf.x`),
cleared at start, and an MPU region makes that window non-cacheable and
not shareable (LDREX/STREX fault on anything else but the DTCM or cacheable memory).

## Updates over Ethernet (A/B slots)

The 2 MB flash is split so that a program update can be tried and undone:

| Address | Size | Content |
|---|---|---|
| `0x0800_0000` | 32 KB (sector 0) | the bootloader (`../bootloader`, 2 KB) |
| `0x0800_8000` | 32 KB (sector 1) | boot records: which slot, pending or confirmed, tries |
| `0x0804_0000` | 256 KB (sector 5) | program slot A |
| `0x0808_0000` | 256 KB (sector 6) | program slot B |
| `0x080C_0000` | 1.25 MB (sectors 7-11) | the city image (`geodb.fw`, 52,088 cities) |

The program is linked for one slot (`GEODB_SLOT=a|b`, `build-ota.sh` builds both: `app-a.bin`,
`app-b.bin`). An update is the build for the slot that is **not** running. The program
runs from the slot it is in, receives the other build over UDP into the other slot (CRC-32 checked),
marks it *pending* and restarts. The bootloader starts a pending slot at most three times; the new
program confirms itself once its network has been up for 20 s, else the old slot (the last known
good one) starts again. A power loss anywhere leaves a bootable slot.

First time, with the ST-LINK: `./flash-first.sh` does all of the following (the whole internal flash is rewritten; QSPI and option bytes are not touched):

    cd firmware/bootloader && cargo build --release
    cd ../stm32f769i-disco && ./build-ota.sh
    cargo run --release -p geodb-fw-core --features std --example make_image
    st-flash --connect-under-reset erase
    probe-rs download --chip STM32F769NIHx --speed 500 ../bootloader/target/thumbv7em-none-eabihf/release/geodb-bootloader
    probe-rs download --chip STM32F769NIHx --speed 500 target-a/thumbv7em-none-eabihf/release/geodb-firmware
    probe-rs download --chip STM32F769NIHx --speed 500 --binary-format bin --base-address 0x080C0000 geodb.fw

Then, with the board on the network (it runs slot A, so send the slot B build):

    cargo run --release -p geodb-board -- ota app-b.bin --host 192.168.x.y

The next update is `app-a.bin`, and so on. `!info` (and `geodb-board info`) tell which slot runs.
The city image is separate: it is not part of an update (yet).

Tested on the board: A to B and B to A over Ethernet (about 1-3 s for the 170 KB, the board is back
after about 7 s), and the fall-back: `GEODB_SLOT=b cargo build --release --features fail-boot` is a program
that hangs after the display is up; sent as an update, the independent watchdog (8 s) resets it three
times and the bootloader goes back to the old slot (about 40 s).

Notes:
* The watchdog is started first thing and petted by the main loop; a hung program is reset (and, after an
  update, counted by the bootloader).
* After a debug session (probe-rs, "connect under reset") the reset vector catch stays set in DEMCR and
  a software reset would halt the core at its first instruction. `ota::reboot()` clears it before the
  reset, so `!reset` and an update work after a debug session; a power cycle clears it as well.
* The boot log has room for about 2000 records (16 bytes each; an update writes 3-4); erasing it when full
  is not implemented.

## The earth (hybrid)

The picture on the globe is not stored: the image carries the coastline as vector rings (Natural Earth
1:50m, the web demo's packed rings, 73 KB, points about 1 km apart). At start the board rasterizes them
into a 2048 x 1024 RGB565 picture in SDRAM (4 MB, `coast::rasterize`: even-odd fill, lakes are holes, four
scanlines per pixel row with exact coverage along each, so the coast is anti-aliased; sea, land by
latitude, ice at the poles). One pixel is 0.18 degrees (20 km at the equator), 14 times finer than the
256 x 128 picture it replaced, for the same flash. The image format is version 2 (coast instead of texture),
so the image has to be replaced: over the network (below) or with `flash-first.sh`.

### The city image over Ethernet

    cargo run --release -p geodb-board -- ota-image firmware/stm32f769i-disco/geodb.fw --host 192.168.x.y

replaces the image in sectors 7-11 (about 6 s to send 1.3 MB, 24 s in all with the erase and the restart). While it
runs the program stops reading the image (the LCD says so; the watchdog is petted between the sector erases). A
cut-off update leaves an image that does not parse: the program then runs a built-in one-city placeholder
(`placeholder.fw`, written by `make_image --placeholder`), the LCD's last line says `NO IMAGE`, and the network
and the updates keep working, so the image can simply be sent again. This is also how a program with a new image
format is rolled out without the ST-LINK: update the program (it runs on the placeholder), then send the image.

### Hybrid colours (image version 3)

The earth is now the hybrid: the vector coast decides land and sea, the colours come from the Blue Marble picture
the image carries (256 x 128 RGB565, 64 KB, the same one the browser simulation offers as "before"). The picture
has sea colour in its coastal land texels, so at start the board splits it by the coast into a land-only and a
sea-only colour field (texels the coast covers fully, or not at all; the rest are filled from their neighbours,
`coast::rasterize_hybrid`) and mixes the two per pixel by the pixel's own coverage. An image without the
picture (the placeholder) gets the procedural colours. The coastlines are stroked over it as before.
The rasterizer's working memory sits in the cacheable SDRAM window (uncached it would take seconds). Rolling
out a new image version: update the program over Ethernet first (it runs on the placeholder), then the image.
The image budget (1.25 MB of flash) now holds 49,264 cities.

### Relief (image version 4)

The image also carries the elevation picture (1024 x 512, one byte a cell, ETOPO 2022 reduced by
`crates/geodb-fw-core/scripts/make_elev.py`; stored packed, 200 KB: median predictor and an adaptive Rice code,
`relief::pack`). At start the board unpacks it into the cacheable SDRAM window, turns it into a hill-shade field
(slopes made `ui::SHADE_EXAGGERATION` times steeper, light from the north-west) and mixes that into the earth
colours (`ui::SHADE_STRENGTH`), then halves the shaded earth for the globe while it moves. Zoomed in (from zoom
2.5) the full redraws also stroke contour lines (isohypses) found on the elevation grid by marching squares: land in
tan, sea floor in blue (every 1000 m, every 500 m from zoom 6). The 200 KB come out of the city budget: 40,928
cities.

## Layers and touch (the same on the board, in the mirror window and in the browser)

* **LAYERS** (bottom button) lights the layers one by one in the order coastlines, relief shading, isohypses,
  city dots, query ring until all are lit, then switches them off one by one back to none, and so on. It starts with
  all lit (the next press switches the last one off). The button shows how many are lit ("LAYERS 3/5") and is
  highlighted while not all are. The board keeps the shaded and the plain earth (the plain copy sits in its own
  cacheable SDRAM window), so switching relief off is free. The layer bits (and the direction) are part of the state
  packet (24 bytes: the options byte and the selected city), so the mirror shows the same screen; clicks in the
  mirror are sent to the board as taps and step its layers.
* **Double tap on a city** (two taps within 350 ms on the same city; a single tap on a city still recentres and
  zooms in, after the double-tap window): the view flies to the city (0.9 s, ease in and out, to about an 80 km
  view) and a card opens over the globe: name, country, position, the ground height from the elevation picture and
  what the names server knows (state). A ring marks the city; a tap on the card closes it.
* **Mouse over a city** (mirror window and browser): a tooltip with its name and country. The board has no pointer.
* **Frame rate:** a moving view at the closest zooms (the scope, from zoom 12) is drawn as a full frame; it skips the
  coastline and contour strokes while it moves (`ui::STROKES`), as quick frames do: 23 ms instead of 61 ms per frame
  on the board (32 fps; the strokes come back when it rests). `ui::PROFILE` times the parts of a quick frame (the
  log line `32fps 19ms: g15 n0 p2` is the frame, the globe, the nearest cities, the panel text, in ms).
* **Layers on a moving globe.** Stroking the vector coastline and the isohypses costs 35 to 70 ms a frame at the
  middle zooms, so a quick frame cannot do it. The board *bakes* the two strokes into the coarse 1024 x 512 earth the
  moving globe is drawn from (`relief::bake_coast`, `bake_contours`, `ui::bake_moving_layers`: equirectangular lines,
  soft like the picture, free per frame); it makes that picture again, in a few hundred ms and only at rest, whenever
  the relief, isohypses or coastlines are switched (`ui::bake_signature`). The sharp vector strokes are drawn over
  the full frame at rest as before. Measured on the board while spinning: 32 fps at zoom 1 to 8 (16 ms at zoom 8),
  17 fps in the scope view (zoom 16: a full frame with the strokes, 53 ms; switch coastlines and isohypses off for
  32 fps there).
