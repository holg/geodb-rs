//! geodb on the STM32F769I-DISCO: the city image lives in flash and is
//! queried in place; the globe, the nearest cities and touch control are on
//! the 800 x 480 LCD. Results and timings also go out over RTT (defmt).

#![no_std]
#![no_main]

mod display;
mod touch;

use cortex_m::peripheral::DWT;
use defmt::{error, info, warn};
use embassy_executor::Spawner;
use embassy_stm32::dsihost::{self, DsiHost};
use embassy_stm32::fmc::Fmc;
use embassy_stm32::gpio::{Level, Output, Speed};
use embassy_stm32::i2c::{self, I2c};
use embassy_stm32::ltdc::{self, Ltdc, LtdcLayer};
use embassy_stm32::time::Hertz;
use embassy_stm32::usart::{self, Uart};
use embassy_stm32::{bind_interrupts, dma, peripherals, Config};
use embassy_time::{with_timeout, Duration, Instant, Timer};
use geodb_fw_core::render::{GlobeLut, LutCell, View};
use geodb_fw_core::{geo, ui, FwImage, Hit};
use panic_probe as _;
use {defmt_rtt as _, embassy_stm32 as _};

/// The image (`cargo run --release -p geodb-fw-core --features std --example
/// make_image`): 1.5 MB in flash, read in place.
static IMAGE: &[u8] = include_bytes!("../geodb.fw");

bind_interrupts!(struct Irqs {
    LTDC => ltdc::InterruptHandler<peripherals::LTDC>;
    DSI => dsihost::InterruptHandler<peripherals::DSIHOST>;
    USART1 => usart::InterruptHandler<peripherals::USART1>;
    DMA2_STREAM7 => dma::InterruptHandler<peripherals::DMA2_CH7>;
    DMA2_STREAM5 => dma::InterruptHandler<peripherals::DMA2_CH5>;
});

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    // STM32F769I-DISCO: 25 MHz HSE crystal -> 216 MHz sysclk, PLLSAI 384/7/2 = 27.43 MHz LTDC pixel
    // clock, DSI PLL 25/5 x 100 = 500 MHz (62.5 MHz lane byte clock).
    let mut config = Config::default();
    {
        use embassy_stm32::rcc::*;
        config.rcc.hse = Some(Hse {
            freq: Hertz(25_000_000),
            mode: HseMode::Oscillator,
        });
        config.rcc.pll_src = PllSource::Hse;
        config.rcc.pll = Some(Pll {
            prediv: PllPreDiv::Div25,
            mul: PllMul::Mul432,
            divp: Some(PllPDiv::Div2), // 216 MHz
            divq: Some(PllQDiv::Div9),
            divr: None,
        });
        config.rcc.pllsai = Some(Pll {
            prediv: PllPreDiv::Div25, // shared PLLM
            mul: PllMul::Mul384,
            divp: None,
            divq: None,
            divr: Some(PllRDiv::Div7), // 54.86 MHz
        });
        config.rcc.lcd_div = Some(embassy_stm32::ltdc::LcdClockDiv::Div2); // 27.43 MHz pixel clock
        config.rcc.dsi = Some(DsiHostPllConfig::new(
            100,
            DsiPllInput::Div5,
            DsiPllOutput::Div1,
        ));
        config.rcc.ahb_pre = AHBPrescaler::Div1;
        config.rcc.apb1_pre = APBPrescaler::Div4;
        config.rcc.apb2_pre = APBPrescaler::Div2;
        config.rcc.sys = Sysclk::Pll1P;
    }
    let p = embassy_stm32::init(config);

    let mut core = cortex_m::Peripherals::take().unwrap();
    // The image and the code run from flash (7 wait states at 216 MHz): the caches make that
    // fast. The SDRAM is mapped non-cacheable by display::init_sdram (MPU).
    core.SCB.enable_icache();
    core.SCB.enable_dcache(&mut core.CPUID);
    DWT::unlock();
    core.DCB.enable_trace();
    core.DWT.enable_cycle_counter();

    info!(
        "geodb firmware: image {} bytes in flash at {:x}",
        IMAGE.len(),
        IMAGE.as_ptr() as usize
    );
    let img = match FwImage::parse(IMAGE) {
        Ok(img) => img,
        Err(_) => {
            error!("the image does not parse");
            loop {
                cortex_m::asm::wfi();
            }
        }
    };
    info!(
        "{} cities, {} named, {} countries",
        img.len(),
        img.named(),
        img.countries()
    );

    // LD1 (red, PJ13) while drawing, LD2 (green, PJ5) once the screen is up.
    let mut red = Output::new(p.PJ13, Level::High, Speed::Low);
    let mut green = Output::new(p.PJ5, Level::Low, Speed::Low);

    // ---- SDRAM (MT48LC4M32B2, 16 MB, 32-bit, FMC bank 1): the framebuffers live here ----
    let sdram = Fmc::sdram_a12bits_d32bits_4banks_bank1(
        p.FMC,
        // A0-A11
        p.PF0,
        p.PF1,
        p.PF2,
        p.PF3,
        p.PF4,
        p.PF5,
        p.PF12,
        p.PF13,
        p.PF14,
        p.PF15,
        p.PG0,
        p.PG1,
        // BA0-BA1
        p.PG4,
        p.PG5,
        // D0-D31
        p.PD14,
        p.PD15,
        p.PD0,
        p.PD1,
        p.PE7,
        p.PE8,
        p.PE9,
        p.PE10,
        p.PE11,
        p.PE12,
        p.PE13,
        p.PE14,
        p.PE15,
        p.PD8,
        p.PD9,
        p.PD10,
        p.PH8,
        p.PH9,
        p.PH10,
        p.PH11,
        p.PH12,
        p.PH13,
        p.PH14,
        p.PH15,
        p.PI0,
        p.PI1,
        p.PI2,
        p.PI3,
        p.PI6,
        p.PI7,
        p.PI9,
        p.PI10,
        // NBL0-NBL3
        p.PE0,
        p.PE1,
        p.PI4,
        p.PI5,
        // SDCKE0, SDCLK, SDNCAS, SDNE0, SDNRAS, SDNWE
        p.PH2,
        p.PG8,
        p.PG15,
        p.PH3,
        p.PF11,
        p.PH5,
        stm32_fmc::devices::mt48lc4m32b2_6::Mt48lc4m32b2 {},
    );
    display::init_sdram(sdram);

    // ---- the panel: reset PJ15, TE PJ2, backlight enable PI14 (BL_CTRL: no DSI command lights the
    // panel while it is low) ----
    let backlight = Output::new(p.PI14, Level::High, Speed::Low);
    let mut lcd_reset = Output::new(p.PJ15, Level::High, Speed::High);
    let ltdc = Ltdc::new(p.LTDC);
    let dsi = DsiHost::new(p.DSIHOST, p.PJ2);
    let mut disp = match display::init_display(ltdc, dsi, &mut lcd_reset).await {
        Ok(mut d) => {
            d.backlight = Some(backlight);
            d
        }
        Err(e) => {
            error!("display init failed: {:?}", e);
            loop {
                Timer::after_millis(500).await;
                red.toggle();
            }
        }
    };
    info!("display up: {} panel", disp.panel.name());

    let mut i2c_cfg = i2c::Config::default();
    i2c_cfg.frequency = Hertz(100_000);
    let mut touch = touch::Touch::new(I2c::new_blocking(p.I2C4, p.PD12, p.PB7, i2c_cfg));

    // ---- the host link: the ST-LINK's virtual COM port (USART1, PA9/PA10) asks a script on the
    // host (scripts/serve_names.py) for the names the flash does not hold ----
    let mut uart = Uart::new(
        p.USART1,
        p.PA9,
        p.PA10,
        p.DMA2_CH7,
        p.DMA2_CH5,
        Irqs,
        usart::Config::default(), // 115200 8N1
    )
    .ok();
    let mut extras = [ui::Extra::empty(); 40];
    let mut extra_next = 0usize;
    let mut resolved_for = (0u32, 0u32, 0u32);
    let mut host_retry = Instant::now();

    // ---- the globe table (per tilt/zoom, so a spin frame is only texture sampling) lives in SDRAM
    // behind the two framebuffers ----
    let lut_table = |offset: usize, n: usize| -> GlobeLut<'static> {
        let cells: &'static mut [LutCell] = unsafe {
            let ptr = (display::SDRAM_BASE + offset) as *mut LutCell;
            for i in 0..n {
                ptr.add(i).write(LutCell::EMPTY);
            }
            core::slice::from_raw_parts_mut(ptr, n)
        };
        GlobeLut::new(cells)
    };
    let fine_at = 2 * display::FB_BYTES;
    // (the fine table for rest and spin, the coarse one for a globe that is being dragged)
    let mut lut = (
        lut_table(fine_at, ui::LUT_CELLS),
        lut_table(
            fine_at + ui::LUT_CELLS * core::mem::size_of::<LutCell>(),
            ui::MOVE_LUT_CELLS,
        ),
    );

    // ---- the first screen ----
    let mut view = View::new(30.0, 10.0);
    let mut spin = ui::Spin::new();
    let mut front = 0usize;
    draw_and_show(
        &mut disp, &mut front, &img, view, spin, &mut lut, &mut red, false, &extras,
    )
    .await;
    green.set_high();

    // ---- touch: the globe is grabbed like a heavy trackball: it follows the finger while it is
    // down, keeps rolling (and slows down) after a flick. A short touch is a tap: a button or a
    // point on the globe. The screen is redrawn back to back while anything moves. ----
    const FRICTION: f32 = 2.5; // 1/s: the roll decays as exp(-FRICTION t)
    const STOP_PX_S: f32 = 12.0;
    let mut grab: Option<Grab> = None;
    let mut vel = (0.0f32, 0.0f32); // px/s
    let mut stamp = Instant::now();
    let mut frames = 0u32;
    let mut draw_ms = 0u32;
    // Both buffers need one full draw (panel, dots) before quick globe-only frames may reuse them.
    let mut full = 0u8;
    let mut dirty = false; // quick frames left the panel and the dots stale
    let mut since = Instant::now();
    loop {
        let moving = spin.on || grab.is_some() || vel.0.abs() + vel.1.abs() > STOP_PX_S;
        if !moving {
            Timer::after_millis(20).await;
        }
        let dt = (stamp.elapsed().as_micros() as f32 / 1.0e6).min(0.25);
        stamp = Instant::now();
        let now = touch.as_mut().and_then(|t| t.read());
        let mut redraw = spin.on;
        match (grab, now) {
            (None, Some(pos)) => {
                vel = (0.0, 0.0);
                grab = Some(Grab {
                    start: pos,
                    last: pos,
                    dragging: false,
                });
            }
            (Some(mut g), Some(pos)) => {
                let (dx, dy) = (
                    i32::from(pos.0) - i32::from(g.last.0),
                    i32::from(pos.1) - i32::from(g.last.1),
                );
                let far = (i32::from(pos.0) - i32::from(g.start.0)).abs()
                    + (i32::from(pos.1) - i32::from(g.start.1)).abs();
                if far >= 12 {
                    g.dragging = true;
                }
                if g.dragging && dt > 0.0 {
                    ui::pan(&mut view, dx, dy);
                    // a smoothed finger speed, for the flick
                    let k = (dt * 20.0).min(1.0);
                    vel.0 += (dx as f32 / dt - vel.0) * k;
                    vel.1 += (dy as f32 / dt - vel.1) * k;
                    redraw = true;
                }
                g.last = pos;
                grab = Some(g);
            }
            (Some(g), None) => {
                grab = None;
                if g.dragging {
                    info!("flick {},{} px/s", vel.0 as i32, vel.1 as i32);
                } else {
                    vel = (0.0, 0.0);
                    let action = ui::hit(view, i32::from(g.start.0), i32::from(g.start.1));
                    info!("tap {},{}", g.start.0, g.start.1);
                    if action != ui::Action::None {
                        ui::apply(&mut view, &mut spin, action);
                        redraw = true;
                        full = 2;
                    }
                }
            }
            (None, None) => {
                if vel.0.abs() + vel.1.abs() > STOP_PX_S {
                    ui::pan_f(&mut view, vel.0 * dt, vel.1 * dt);
                    let decay = 1.0 - (FRICTION * dt).min(1.0);
                    vel = (vel.0 * decay, vel.1 * decay);
                    redraw = true;
                } else {
                    vel = (0.0, 0.0);
                }
            }
        }
        if spin.on {
            ui::advance(&mut view, spin, dt);
        }
        let motion =
            spin.on || grab.is_some_and(|g| g.dragging) || vel.0.abs() + vel.1.abs() > STOP_PX_S;
        if !motion && dirty {
            // Came to rest: bring the panel and the dots up to date in both buffers.
            dirty = false;
            full = 2;
        }
        if !motion && full == 0 && grab.is_none() {
            let key = (view.lat.to_bits(), view.lon.to_bits(), view.zoom.to_bits());
            if key != resolved_for && Instant::now() >= host_retry {
                resolved_for = key;
                if let Some(uart) = uart.as_mut() {
                    match resolve(uart, &img, view, &mut extras, &mut extra_next).await {
                        Some(true) => full = 2,
                        Some(false) => {}
                        None => host_retry = Instant::now() + Duration::from_secs(5),
                    }
                }
            }
        }
        if full > 0 {
            redraw = true;
        }
        if redraw {
            let quick = full == 0 && motion && view.zoom < ui::SCOPE_ZOOM;
            draw_ms = draw_and_show(
                &mut disp, &mut front, &img, view, spin, &mut lut, &mut red, quick, &extras,
            )
            .await;
            if quick {
                dirty = true;
            } else {
                full = full.saturating_sub(1);
            }
            frames += 1;
            if frames == 30 {
                let ms = since.elapsed().as_millis() as u32;
                info!("moving: {} ms per frame ({} ms drawing)", ms / 30, draw_ms);
                frames = 0;
                since = Instant::now();
            }
        } else {
            frames = 0;
            since = Instant::now();
        }
    }
}

#[derive(Clone, Copy)]
struct Grab {
    start: (u16, u16),
    last: (u16, u16),
    dragging: bool,
}

/// Draws the screen into the back buffer, then makes it the one on the panel.
#[allow(clippy::too_many_arguments)]
async fn draw_and_show(
    disp: &mut display::Display,
    front: &mut usize,
    img: &FwImage<'_>,
    view: View,
    spin: ui::Spin,
    lut: &mut (GlobeLut<'_>, GlobeLut<'_>),
    red: &mut Output<'static>,
    quick: bool,
    extras: &[ui::Extra],
) -> u32 {
    red.set_high();
    let back = 1 - *front;
    let cycles = DWT::cycle_count();
    if quick {
        ui::draw_moving(&mut disp.fb[back].fb(), img, view, &mut lut.1);
    } else {
        ui::draw(
            &mut disp.fb[back].fb(),
            img,
            view,
            spin,
            Some(&mut lut.0),
            extras,
        );
    }
    let ms = DWT::cycle_count().wrapping_sub(cycles) / 216_000;
    let ok = disp
        .ltdc
        .set_buffer(LtdcLayer::Layer1, disp.fb[back].as_ptr().cast())
        .await
        .is_ok();
    if ok {
        *front = back;
    } else {
        warn!("display: buffer swap failed");
    }
    red.set_low();
    ms
}

/// Small fixed text buffer for `write!`.
struct Buf {
    bytes: [u8; 40],
    len: usize,
}

impl Buf {
    fn as_str(&self) -> &str {
        core::str::from_utf8(&self.bytes[..self.len]).unwrap_or("")
    }
}

impl core::fmt::Write for Buf {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let b = s.as_bytes();
        let end = self.len + b.len();
        if end > self.bytes.len() {
            return Err(core::fmt::Error);
        }
        self.bytes[self.len..end].copy_from_slice(b);
        self.len = end;
        Ok(())
    }
}

/// Asks the host for the names of the nearest cities the flash has no name for. Returns whether
/// anything new arrived, or `None` when the host did not answer (not running).
async fn resolve(
    uart: &mut Uart<'static, embassy_stm32::mode::Async>,
    img: &FwImage<'_>,
    view: View,
    extras: &mut [ui::Extra],
    next: &mut usize,
) -> Option<bool> {
    use core::fmt::Write;
    let mut hits = [Hit { index: 0, km: 0.0 }; ui::LIST];
    let n = img.nearest(view.lat, view.lon, &mut hits);
    let mut changed = false;
    for hit in &hits[..n] {
        let idx = hit.index;
        if img.name(idx as usize).is_some() || extras.iter().any(|e| e.index == idx) {
            continue;
        }
        let (la, lo) = geo::to_deg(img.geoid(idx as usize));
        let (la, lo) = ((la * 1e5) as i32, (lo * 1e5) as i32);
        let mut key = Buf {
            bytes: [0; 40],
            len: 0,
        };
        let _ = write!(key, "{la},{lo}");
        let mut ask = Buf {
            bytes: [0; 40],
            len: 0,
        };
        let _ = write!(ask, "?{la},{lo}\n");
        uart.write(&ask.bytes[..ask.len]).await.ok()?;
        let mut reply = [0u8; 96];
        let got = with_timeout(Duration::from_millis(250), uart.read_until_idle(&mut reply))
            .await
            .ok()?
            .ok()?;
        let Ok(text) = core::str::from_utf8(&reply[..got]) else {
            continue;
        };
        let text = text.trim();
        let Some(body) = text.strip_prefix('=') else {
            continue;
        };
        let mut parts = body.splitn(3, '|');
        if parts.next() != Some(key.as_str()) {
            continue; // a late answer to an older question
        }
        let (name, detail) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));
        extras[*next % extras.len()].set(idx, name, detail);
        *next += 1;
        changed = true;
        info!("host: city {} is {}", idx, name);
    }
    Some(changed)
}
