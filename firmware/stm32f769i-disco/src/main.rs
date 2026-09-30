//! geodb on the STM32F769I-DISCO, stage 2: the city image lives in flash
//! and is queried in place; results and timings go out over RTT (defmt).

#![no_std]
#![no_main]

use cortex_m::peripheral::DWT;
use defmt::{error, info};
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_stm32::gpio::{Level, Output, Speed};
use embassy_stm32::rcc::{
    AHBPrescaler, APBPrescaler, Pll, PllMul, PllPDiv, PllPreDiv, PllSource, Sysclk,
};
use embassy_stm32::Config;
use embassy_time::Timer;
use geodb_fw_core::render::{self, Fb, Texture, View};
use geodb_fw_core::{FwImage, Hit};
use panic_probe as _;

/// The image (`cargo run --release -p geodb-fw-core --features std --example
/// make_image`): 1.5 MB in flash, read in place.
static IMAGE: &[u8] = include_bytes!("../geodb.fw");

/// The system clock (MHz) set below.
const MHZ: u32 = 216;

fn micros(cycles: u32) -> u32 {
    cycles / MHZ
}

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let mut config = Config::default();
    // HSI 16 MHz / 16 * 432 / 2 = 216 MHz (overdrive is set by the driver).
    config.rcc.pll_src = PllSource::HSI;
    config.rcc.pll = Some(Pll {
        prediv: PllPreDiv::DIV16,
        mul: PllMul::MUL432,
        divp: Some(PllPDiv::DIV2),
        divq: None,
        divr: None,
    });
    config.rcc.sys = Sysclk::PLL1_P;
    config.rcc.ahb_pre = AHBPrescaler::DIV1;
    config.rcc.apb1_pre = APBPrescaler::DIV4;
    config.rcc.apb2_pre = APBPrescaler::DIV2;
    let p = embassy_stm32::init(config);

    let mut core = cortex_m::Peripherals::take().unwrap();
    // The image and the code run from flash (7 wait states at 216 MHz):
    // the caches make that fast. The Cortex-M7's DWT is locked at reset.
    core.SCB.enable_icache();
    core.SCB.enable_dcache(&mut core.CPUID);
    DWT::unlock();
    core.DCB.enable_trace();
    core.DWT.enable_cycle_counter();

    info!("geodb firmware: image {} bytes at {:x}", IMAGE.len(), IMAGE.as_ptr() as usize);
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
        "{} cities, {} named, {} countries, texture {}x{}",
        img.len(),
        img.named(),
        img.countries(),
        img.texture().0,
        img.texture().1
    );

    for (name, lat, lon) in [
        ("Munich", 48.137f32, 11.575f32),
        ("Tokyo", 35.68, 139.69),
        ("Sydney", -33.87, 151.21),
        ("Reykjavik", 64.14, -21.9),
        ("open ocean", -40.0, -140.0),
    ] {
        let mut hits = [Hit { index: 0, km: 0.0 }; 10];
        let t = DWT::cycle_count();
        let n = img.nearest(lat, lon, &mut hits);
        let nearest_us = micros(DWT::cycle_count().wrapping_sub(t));
        let mut inside = 0u32;
        let t = DWT::cycle_count();
        let tested = img.radius(lat, lon, 300.0, |_| inside += 1);
        let radius_us = micros(DWT::cycle_count().wrapping_sub(t));
        info!(
            "{}: 10 nearest in {} us, 300 km radius: {} cities ({} tested) in {} us",
            name, nearest_us, inside, tested, radius_us
        );
        for (i, h) in hits[..n].iter().take(3).enumerate() {
            let idx = h.index as usize;
            info!(
                "  {}. {} ({}) {} km",
                i + 1,
                img.name(idx).unwrap_or("(unnamed)"),
                img.country_iso(img.country(idx)),
                h.km as u32
            );
        }
    }

    // The globe renderer, timed into a small buffer in SRAM (the full
    // 800 x 480 frame needs the external SDRAM).
    static mut FRAME: [u16; 240 * 240] = [0; 240 * 240];
    let (w, h, px) = img.texture();
    let tex = Texture { w, h, px };
    #[allow(static_mut_refs)]
    let mut fb = Fb {
        px: unsafe { &mut FRAME },
        w: 240,
        h: 240,
    };
    let t = DWT::cycle_count();
    render::draw_globe(&mut fb, 120, 120, 116, View::new(20.0, 80.0), &tex);
    let globe_ms = micros(DWT::cycle_count().wrapping_sub(t)) / 1000;
    let lit = fb.px.iter().filter(|&&c| c != 0).count();
    info!("globe 240x240 (r 116) drawn in {} ms, {} pixels lit", globe_ms, lit);

    // LD1 (red, PJ13) and LD2 (green, PJ5) of the STM32F769I-DISCO.
    let mut red = Output::new(p.PJ13, Level::Low, Speed::Low);
    let mut green = Output::new(p.PJ5, Level::Low, Speed::Low);
    loop {
        green.set_high();
        Timer::after_millis(400).await;
        green.set_low();
        red.set_high();
        Timer::after_millis(400).await;
        red.set_low();
    }
}
