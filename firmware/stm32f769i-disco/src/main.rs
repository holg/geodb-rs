//! geodb on the STM32F769I-DISCO: the city image lives in flash and is
//! queried in place; the globe, the nearest cities and touch control are on
//! the 800 x 480 LCD. Results and timings also go out over RTT (defmt).

#![no_std]
#![no_main]

mod display;
mod link;
mod net;
mod ota;
mod touch;

use core::fmt::Write;
use cortex_m::peripheral::DWT;
use defmt::{error, info, warn};
use embassy_executor::Spawner;
use embassy_net::Stack;
use embassy_stm32::dsihost::{self, DsiHost};
use embassy_stm32::eth::{self, Ethernet};
use embassy_stm32::fmc::Fmc;
use embassy_stm32::gpio::{Level, Output, Speed};
use embassy_stm32::i2c::{self, I2c};
use embassy_stm32::ltdc::{self, Ltdc, LtdcLayer};
use embassy_stm32::rng::{self, Rng};
use embassy_stm32::time::Hertz;
use embassy_stm32::{bind_interrupts, peripherals, Config};
use embassy_time::{with_timeout, Duration, Instant, Timer};
use geodb_fw_core::render::{GlobeLut, LutCell, Texture, View};
use geodb_fw_core::{geo, ui, FwImage, Hit};
use panic_probe as _;
use {defmt_rtt as _, embassy_stm32 as _};

/// The city image (`make_image`) sits in the last flash sectors, outside both program slots, so a
/// program update leaves it alone: flash it once with
/// `probe-rs download --binary-format bin --base-address 0x080C0000 geodb.fw`.
const IMAGE_ADDR: usize = 0x080C_0000;
const IMAGE_MAX: usize = 0x14_0000;

/// SDRAM beyond the framebuffers and the globe tables (about 2.1 MB): the earth picture and the
/// scratch of its rasterizer.
const EARTH_AT: usize = 0x0040_0000;
const SCRATCH_AT: usize = 0x0080_0000;
const SCRATCH_LEN: usize = 0x0020_0000;

/// A tiny valid image (no coast, one city "No image"): what runs when the flash holds no usable city
/// image (a new image format, an update that was cut off): the network and the updates keep working
/// and the LCD says so. `make_image --placeholder` writes it.
static PLACEHOLDER: &[u8] = include_bytes!("../placeholder.fw");

/// The image in flash, when it parses (its header names the length); else the placeholder.
fn flash_image() -> &'static [u8] {
    if !ota::UPDATING_IMAGE.load(core::sync::atomic::Ordering::Relaxed) {
        let len = unsafe { ((IMAGE_ADDR + 60) as *const u32).read_volatile() } as usize;
        let bytes =
            unsafe { core::slice::from_raw_parts(IMAGE_ADDR as *const u8, len.min(IMAGE_MAX)) };
        if FwImage::parse(bytes).is_ok() {
            return bytes;
        }
    }
    PLACEHOLDER
}

bind_interrupts!(struct Irqs {
    LTDC => ltdc::InterruptHandler<peripherals::LTDC>;
    DSI => dsihost::InterruptHandler<peripherals::DSIHOST>;
    ETH => eth::InterruptHandler<peripherals::ETH>;
    RNG => rng::InterruptHandler<peripherals::RNG>;
});

#[embassy_executor::main]
async fn main(spawner: Spawner) {
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
    // The watchdog: a program that hangs is reset, and after an update the bootloader then counts the
    // failed tries and goes back to the old slot. The loops below pet it.
    let mut watchdog = embassy_stm32::wdg::IndependentWatchdog::new(p.IWDG, 8_000_000);
    watchdog.unleash();

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
        flash_image().len(),
        flash_image().as_ptr() as usize
    );
    let image_ok = flash_image().as_ptr() as usize == IMAGE_ADDR;
    let img = match FwImage::parse(flash_image()) {
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
    // (the MPU now maps the Ethernet window non-cacheable)
    unsafe { net::clear_buffers() };
    link::init();

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
    watchdog.pet();
    #[cfg(feature = "fail-boot")]
    loop {
        cortex_m::asm::nop(); // (never pets: the watchdog resets, see Cargo.toml)
    }

    let mut i2c_cfg = i2c::Config::default();
    i2c_cfg.frequency = Hertz(100_000);
    let mut touch = touch::Touch::new(I2c::new_blocking(p.I2C4, p.PD12, p.PB7, i2c_cfg));

    let mut status = Buf::new();
    let _ = status.write_str("net: no cable");
    let mut net_state = 0u8;

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

    // ---- the earth: the coastline rings of the image rasterized into a 2048 x 1024 RGB565 picture
    // (4 MB) in SDRAM, with a scratch area for the crossings behind it ----
    watchdog.pet();
    let earth_px: &'static mut [u8] = unsafe {
        core::slice::from_raw_parts_mut(
            (display::SDRAM_BASE + EARTH_AT) as *mut u8,
            ui::EARTH_W * ui::EARTH_H * 2,
        )
    };
    let scratch: &'static mut [u8] = unsafe {
        core::slice::from_raw_parts_mut((display::SDRAM_BASE + SCRATCH_AT) as *mut u8, SCRATCH_LEN)
    };
    let t = Instant::now();
    match geodb_fw_core::coast::rasterize(img.coast(), ui::EARTH_W, ui::EARTH_H, earth_px, scratch)
    {
        Ok(()) => info!("earth rasterized in {} ms", t.elapsed().as_millis()),
        Err(_) => error!("the coastline does not rasterize"),
    }
    watchdog.pet();
    let earth = Texture {
        w: ui::EARTH_W,
        h: ui::EARTH_H,
        px: &*earth_px,
    };

    // ---- the first screen ----
    let mut view = View::new(30.0, 10.0);
    let mut spin = ui::Spin::new();
    let mut front = 0usize;
    draw_and_show(
        &mut disp,
        &mut front,
        &img,
        view,
        spin,
        &mut lut,
        &mut red,
        false,
        0,
        status.as_str(),
        &extras,
        &earth,
    )
    .await;
    green.set_high();

    // (after the first screen: the globe is up even when the network does not come)
    // ---- Ethernet (LAN8742 over RMII) with DHCP; the host is found by broadcast ----
    let mut rng = Rng::new(p.RNG, Irqs);
    let mut seed = [0u8; 8];
    rng.blocking_fill_bytes(&mut seed);
    info!("ethernet: starting the MAC (waits for the PHY clock)");
    let device = Ethernet::new(
        unsafe { net::packets() },
        p.ETH,
        p.PA1,
        p.PA7,
        p.PC4,
        p.PC5,
        p.PG13,
        p.PG14,
        p.PG11,
        [0x02, 0x47, 0x45, 0x4f, 0x44, 0x42], // locally administered: "GEODB"
        p.ETH_SMA,
        p.PA2,
        p.PC1,
        Irqs,
    );
    info!("ethernet: MAC up");
    let (stack, runner) = Stack::new(unsafe { net::storage() }, u64::from_le_bytes(seed));
    let iface = stack.add_iface(net::DEVICE.init(device)).ok();
    if let Some(iface) = &iface {
        let _ = iface.set_dhcpv4(Some(Default::default()));
    }
    spawner.spawn(defmt::unwrap!(net::net_task(runner)));
    info!("ethernet: started, waiting for a cable and DHCP");
    let flash = embassy_stm32::flash::Flash::new_blocking(p.FLASH);
    spawner.spawn(defmt::unwrap!(net::command_task(
        stack,
        ota::Ota::new(flash)
    )));
    let mut udp = net::open_socket(stack);

    // ---- touch: the globe is grabbed like a heavy trackball: it follows the finger while it is
    // down, keeps rolling (and slows down) after a flick. A short touch is a tap: a button or a
    // point on the globe. The screen is redrawn back to back while anything moves. ----
    const FRICTION: f32 = 2.5; // 1/s: the roll decays as exp(-FRICTION t)
    const STOP_PX_S: f32 = 12.0;
    let mut grab: Option<Grab> = None;
    let mut vel = (0.0f32, 0.0f32); // px/s
    let mut stamp = Instant::now();
    let mut frames = 0u32;
    let mut fps = 0u32;
    let mut last_frame = Instant::now();
    let mut last_state = Instant::now();
    let mut last_tick = Instant::now();
    let mut inj_seen: Option<Instant> = None; // when a drag from the mirror last moved the globe
                                              // Both buffers need one full draw (panel, dots) before quick globe-only frames may reuse them.
    let mut full = 0u8;
    let mut dirty = false; // quick frames left the panel and the dots stale
    let mut since = Instant::now();
    loop {
        watchdog.pet();
        if ota::UPDATING_IMAGE.load(core::sync::atomic::Ordering::Relaxed) {
            // The city image is being replaced over the network: nothing may read it.
            for fb in 0..2 {
                ui::draw_status(
                    &mut disp.fb[fb].fb(),
                    "UPDATING THE CITY IMAGE - KEEP THE POWER ON",
                );
            }
            Timer::after_millis(100).await;
            continue;
        }
        let moving = spin.on || grab.is_some() || vel.0.abs() + vel.1.abs() > STOP_PX_S;
        if !moving {
            Timer::after_millis(20).await;
        }
        let dt = (stamp.elapsed().as_micros() as f32 / 1.0e6).min(0.25);
        stamp = Instant::now();
        // the network state: no link / waiting for DHCP / up with an address
        let state = match &iface {
            Some(i) if i.is_config_up() => 2,
            Some(i) if i.is_link_up() => 1,
            _ => 0,
        };
        if state != net_state {
            net_state = state;
            net::NET_UP.store(state == 2, core::sync::atomic::Ordering::Relaxed);
            status.clear();
            match state {
                2 => {
                    let ip = iface
                        .as_ref()
                        .and_then(|i| i.ip_addrs().first().map(|a| a.cidr));
                    match ip {
                        Some(cidr) => {
                            let _ = write!(status, "net: {}", cidr);
                        }
                        None => {
                            let _ = status.write_str("net: up");
                        }
                    }
                }
                1 => {
                    let _ = status.write_str("net: link, waiting for DHCP");
                }
                _ => {
                    let _ = status.write_str("net: no cable");
                }
            }
            info!("{}", status.as_str());
            netlog(udp.as_mut(), state == 2, status.as_str()).await;
            full = 2;
        }
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
                if inj_seen.is_some_and(|t| t.elapsed() < Duration::from_millis(80)) {
                    // a drag from the mirror is moving the globe: no coasting on top of it
                } else if vel.0.abs() + vel.1.abs() > STOP_PX_S {
                    ui::pan_f(&mut view, vel.0 * dt, vel.1 * dt);
                    let decay = 1.0 - (FRICTION * dt).min(1.0);
                    vel = (vel.0 * decay, vel.1 * decay);
                    redraw = true;
                } else {
                    vel = (0.0, 0.0);
                }
            }
        }
        // commands through the debug probe: touches as from the mirror, queries answered here
        if let Some(cmd) = link::poll() {
            match cmd {
                geodb_fw_core::link::Command::Query { lat, lon, km } => {
                    let a = img.answer(lat, lon, km, || DWT::cycle_count() / 216);
                    link::done(Some(&a));
                }
                touch_cmd => {
                    net::inject(touch_cmd);
                    link::done(None);
                }
            }
        }
        // input injected from the mirror window (UDP commands): the same as a finger
        let (idx, idy) = (
            net::INJ_DX.swap(0, core::sync::atomic::Ordering::Relaxed),
            net::INJ_DY.swap(0, core::sync::atomic::Ordering::Relaxed),
        );
        if (idx != 0 || idy != 0) && dt > 0.0 {
            ui::pan(&mut view, idx, idy);
            let k = (dt * 20.0).min(1.0);
            vel.0 += (idx as f32 / dt - vel.0) * k;
            vel.1 += (idy as f32 / dt - vel.1) * k;
            inj_seen = Some(Instant::now());
            redraw = true;
        }
        if net::INJ_RELEASE.swap(false, core::sync::atomic::Ordering::Relaxed) {
            inj_seen = None; // let go: the flick coasts
        }
        let tap = net::INJ_TAP.swap(0, core::sync::atomic::Ordering::Relaxed);
        if tap & (1 << 31) != 0 {
            let (x, y) = (((tap >> 16) & 0x3ff) as i32, (tap & 0x3ff) as i32);
            let action = ui::hit(view, x, y);
            info!("tap {},{} from the mirror", x, y);
            vel = (0.0, 0.0);
            if action != ui::Action::None {
                ui::apply(&mut view, &mut spin, action);
                redraw = true;
                full = 2;
            }
        }
        if spin.on {
            ui::advance(&mut view, spin, dt);
        }
        let motion = spin.on
            || grab.is_some_and(|g| g.dragging)
            || vel.0.abs() + vel.1.abs() > STOP_PX_S
            || inj_seen.is_some_and(|t| t.elapsed() < Duration::from_millis(80));
        if !motion && dirty {
            // Came to rest: bring the panel and the dots up to date in both buffers.
            dirty = false;
            full = 2;
        }
        if !motion && full == 0 && grab.is_none() {
            let key = (view.lat.to_bits(), view.lon.to_bits(), view.zoom.to_bits());
            if key != resolved_for && Instant::now() >= host_retry {
                resolved_for = key;
                let net_up = net_state == 2;
                let mut link = match udp.as_mut() {
                    Some(u) if net_up => Some(Link::Udp(u)),
                    _ => None,
                };
                if let Some(link) = link.as_mut() {
                    match resolve(link, &img, view, &mut extras, &mut extra_next).await {
                        Some(true) => full = 2,
                        Some(false) => {}
                        None => host_retry = Instant::now() + Duration::from_secs(30),
                    }
                }
            }
        }
        if full > 0 {
            redraw = true;
        }
        // a heartbeat on the LCD: the network line with the uptime, so a stuck board is told from a
        // quiet one without any debugger (written into both buffers, whichever is on the panel)
        if last_tick.elapsed() >= Duration::from_secs(1) {
            last_tick = Instant::now();
            let mut line = Buf::new();
            let base = if image_ok {
                status.as_str()
            } else {
                "NO IMAGE: geodb-board ota-image"
            };
            let _ = write!(line, "{} up {}s", base, Instant::now().as_secs());
            for fb in 0..2 {
                ui::draw_status(&mut disp.fb[fb].fb(), line.as_str());
            }
        }
        link::publish(&ui::encode_state(view, spin, fps));
        // the viewer on the host mirrors the screen from this: the view, a few bytes
        if net_state == 2
            && last_state.elapsed() >= Duration::from_millis(if motion { 15 } else { 500 })
        {
            last_state = Instant::now();
            if let Some(socket) = udp.as_mut() {
                let packet = ui::encode_state(view, spin, fps);
                let _ = with_timeout(
                    Duration::from_millis(5),
                    socket.send_to(&packet, net::STATE_BROADCAST),
                )
                .await;
            }
        }
        if ota::UPDATING_IMAGE.load(core::sync::atomic::Ordering::Relaxed) {
            continue; // (the update began while this turn waited)
        }
        if redraw {
            // a smoothed frame rate from the time between frames
            let gap = last_frame.elapsed().as_micros() as u32;
            last_frame = Instant::now();
            if motion && gap > 0 {
                let now = 1_000_000 / gap;
                fps = if fps == 0 { now } else { (fps * 3 + now) / 4 };
                net::FPS.store(fps, core::sync::atomic::Ordering::Relaxed);
            }
            let quick = full == 0 && motion && view.zoom < ui::SCOPE_ZOOM;
            let draw_ms = draw_and_show(
                &mut disp,
                &mut front,
                &img,
                view,
                spin,
                &mut lut,
                &mut red,
                quick,
                fps,
                status.as_str(),
                &extras,
                &earth,
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
                let mut line = Buf::new();
                let _ = write!(line, "moving: {} fps, {} ms drawing", fps, draw_ms);
                netlog(udp.as_mut(), net_state == 2, line.as_str()).await;
                frames = 0;
                since = Instant::now();
            }
        } else {
            frames = 0;
            fps = 0;
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
    fps: u32,
    status: &str,
    extras: &[ui::Extra],
    earth: &Texture<'_>,
) -> u32 {
    red.set_high();
    let back = 1 - *front;
    let cycles = DWT::cycle_count();
    if quick {
        let mut fb = disp.fb[back].fb();
        ui::draw_moving(&mut fb, img, view, spin, &mut lut.1, extras, earth);
        ui::draw_fps(&mut fb, fps);
        ui::draw_status(&mut fb, status);
    } else {
        ui::draw(
            &mut disp.fb[back].fb(),
            img,
            view,
            spin,
            Some(&mut lut.0),
            extras,
            earth,
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
    const fn new() -> Buf {
        Buf {
            bytes: [0; 40],
            len: 0,
        }
    }

    fn clear(&mut self) {
        self.len = 0;
    }

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
    link: &mut Link<'_>,
    img: &FwImage<'_>,
    view: View,
    extras: &mut [ui::Extra],
    next: &mut usize,
) -> Option<bool> {
    let mut hits = [Hit { index: 0, km: 0.0 }; ui::LIST];
    let n = img.nearest(view.lat, view.lon, &mut hits);
    let mut changed = false;
    for (k, hit) in hits[..n].iter().enumerate() {
        if ota::UPDATING_IMAGE.load(core::sync::atomic::Ordering::Relaxed) {
            return Some(false); // the image is being replaced
        }
        let idx = hit.index;
        // (every city in the image has a name; the host adds the state and country of the nearest)
        if (img.name(idx as usize).is_some() && k > 0) || extras.iter().any(|e| e.index == idx) {
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
        let _ = writeln!(ask, "?{la},{lo}");
        let mut reply = [0u8; 96];
        let got = link.ask(&ask.bytes[..ask.len], &mut reply).await?;
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

/// The way to the host: a broadcast question and a unicast answer on the network.
enum Link<'a> {
    Udp(&'a mut embassy_net::udp::UdpSocket<'static>),
}

impl Link<'_> {
    /// Sends `ask`, waits for the answer; `None` when the host does not answer.
    async fn ask(&mut self, ask: &[u8], reply: &mut [u8]) -> Option<usize> {
        match self {
            Link::Udp(socket) => {
                socket.send_to(ask, net::BROADCAST).await.ok()?;
                with_timeout(
                    Duration::from_millis(100),
                    socket.recv_from_with(|data, _from| {
                        let n = data.len().min(reply.len());
                        reply[..n].copy_from_slice(&data[..n]);
                        n
                    }),
                )
                .await
                .ok()?
                .ok()
            }
        }
    }
}

/// A log line for the host (the script prints lines that start with `#`), by broadcast.
async fn netlog(udp: Option<&mut embassy_net::udp::UdpSocket<'static>>, up: bool, text: &str) {
    if let (Some(socket), true) = (udp, up) {
        let mut line = [0u8; 64];
        let n = text.len().min(62);
        line[0] = b'#';
        line[1..=n].copy_from_slice(&text.as_bytes()[..n]);
        let _ = with_timeout(
            Duration::from_millis(50),
            socket.send_to(&line[..=n], net::BROADCAST),
        )
        .await;
    }
}
