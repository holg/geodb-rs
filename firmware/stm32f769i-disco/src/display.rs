//! STM32F769I-DISCO display path: 16 MB SDRAM on the FMC, MIPI-DSI host + LTDC
//! in video burst mode, the 4" 800x480 panel behind it, and two RGB565
//! framebuffers in SDRAM.
//!
//! The bring-up (SDRAM pins, MPU attribute, DSI PHY and video timing, the
//! controller detection and both panel init sequences) is the one proven on
//! this board in the plantworks gateway (`plantworks-gw-stm32`), from ST's
//! board support package: OTM8009A on the older revisions, NT35510 on rev
//! B03 and later, read over DSI after the host is up.

use core::sync::atomic::{AtomicU8, Ordering};

use defmt::info;
use embassy_stm32::dsihost::panel::DsiPanel;
use embassy_stm32::dsihost::{
    self, DsiColor, DsiHost, DsiHostMode, DsiHostPhyConfig, DsiHostPhyLanes, DsiVideoConfig,
    DsiVideoMode, PacketType,
};
use embassy_stm32::fmc::Fmc;
use embassy_stm32::gpio::Output;
use embassy_stm32::ltdc::{
    Ltdc, LtdcConfiguration, LtdcLayer, LtdcLayerConfig, PixelFormat, PolarityActive, PolarityEdge,
    DSI,
};
use embassy_stm32::peripherals::{DSIHOST, FMC, LTDC};
use embassy_time::{block_for, Duration, Timer};
use embedded_display_controller::dsi::{DsiHostCtrlIo, DsiReadCommand, DsiWriteCommand};
use geodb_fw_core::render::{rgb565, Fb};
use stm32_fmc::devices::mt48lc4m32b2_6::Mt48lc4m32b2;

pub const WIDTH: usize = 800;
pub const HEIGHT: usize = 480;

/// SDRAM map: two RGB565 framebuffers.
pub const SDRAM_BASE: usize = 0xC000_0000;
pub const SDRAM_SIZE: usize = 16 * 1024 * 1024;
pub const FB_PIXELS: usize = WIDTH * HEIGHT;
pub const FB_BYTES: usize = FB_PIXELS * 2;
pub const FB0_ADDR: usize = SDRAM_BASE;
pub const FB1_ADDR: usize = SDRAM_BASE + FB_BYTES;

// ---------------------------------------------------------------------------
// SDRAM
// ---------------------------------------------------------------------------

/// Where ethbuf.x puts the Ethernet packet buffers.
pub const ETH_BUF_ADDR: usize = 0x2006_0000;

pub type Sdram = stm32_fmc::Sdram<Fmc<'static, FMC>, Mt48lc4m32b2>;

/// The Cortex-M default memory map treats 0xC000_0000 as *Device* memory: no
/// unaligned accesses, no multi-word transfers — Rust structs with f64 fields on
/// a heap there hard-fault (UsageFault "unaligned access" while a `Vec` pushes).
/// One MPU region makes the 16 MB SDRAM *Normal*, shareable, non-cacheable
/// memory (the data cache is off on this firmware).
fn sdram_mpu() {
    let mpu = unsafe { &*cortex_m::peripheral::MPU::PTR };
    unsafe {
        cortex_m::asm::dmb();
        mpu.ctrl.write(0);
        mpu.rnr.write(0);
        mpu.rbar.write(SDRAM_BASE as u32);
        // XN | AP=full access | TEX=001 S=1 C=0 B=0 (normal, non-cacheable) | SIZE=2^24 | ENABLE
        mpu.rasr
            .write((1 << 28) | (0b011 << 24) | (0b001 << 19) | (1 << 18) | (23 << 1) | 1);
        // Region 1: the Ethernet DMA buffers (64 KB of SRAM1, see ethbuf.x), non-cacheable too.
        mpu.rnr.write(1);
        mpu.rbar.write(ETH_BUF_ADDR as u32);
        mpu.rasr
            .write((1 << 28) | (0b011 << 24) | (0b001 << 19) | (1 << 18) | (15 << 1) | 1);
        mpu.ctrl.write((1 << 2) | 1); // PRIVDEFENA | ENABLE
        cortex_m::asm::dsb();
        cortex_m::asm::isb();
    }
}

/// Bring up the SDRAM, map it as normal memory and verify a few words. Returns the base pointer.
pub fn init_sdram(mut sdram: Sdram) -> *mut u32 {
    let ptr = sdram.init(&mut embassy_time::Delay);
    assert_eq!(ptr as usize, SDRAM_BASE, "unexpected SDRAM base");
    sdram_mpu();
    // walking pattern over a few KB at both ends
    let words = unsafe { core::slice::from_raw_parts_mut(ptr, SDRAM_SIZE / 4) };
    let n = words.len();
    for i in (0..1024).chain(n - 1024..n) {
        words[i] = (i as u32).wrapping_mul(0x9E37_79B9);
    }
    for i in (0..1024).chain(n - 1024..n) {
        assert_eq!(
            words[i],
            (i as u32).wrapping_mul(0x9E37_79B9),
            "SDRAM readback failed"
        );
    }
    info!(
        "SDRAM ok: {} MB at {:#x}",
        SDRAM_SIZE / 1024 / 1024,
        ptr as usize
    );
    ptr
}

// ---------------------------------------------------------------------------
// Framebuffer (RGB565 in SDRAM)
// ---------------------------------------------------------------------------

pub struct Framebuffer {
    ptr: *mut u16,
}

unsafe impl Send for Framebuffer {}

impl Framebuffer {
    /// # Safety
    /// `addr` must point at `FB_BYTES` of writable memory that nothing else uses.
    pub const unsafe fn at(addr: usize) -> Self {
        Self {
            ptr: addr as *mut u16,
        }
    }

    pub fn as_ptr(&self) -> *const u16 {
        self.ptr
    }

    /// The pixels as the renderer's framebuffer.
    pub fn fb(&mut self) -> Fb<'_> {
        Fb {
            px: unsafe { core::slice::from_raw_parts_mut(self.ptr, FB_PIXELS) },
            w: WIDTH,
            h: HEIGHT,
        }
    }
}

// ---------------------------------------------------------------------------
// Panels
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug, defmt::Format)]
pub enum PanelKind {
    Otm8009a,
    Nt35510,
}

impl PanelKind {
    pub fn name(self) -> &'static str {
        match self {
            PanelKind::Otm8009a => "OTM8009A",
            PanelKind::Nt35510 => "NT35510",
        }
    }
}

/// LTDC ↔ DSI-wrapper signalling the way ST's `HAL_LTDC_StructInitFromVideoConfig`
/// derives it for DSI polarities "active high": HSYNC/VSYNC active high, DE active
/// LOW (inverted by design), pixel clock not inverted. embassy's trait default is
/// active-low for all three, which gives a black screen on this board.
fn dsi_ltdc_config<P: DsiPanel>() -> LtdcConfiguration {
    LtdcConfiguration {
        active_width: P::ACTIVE_WIDTH,
        active_height: P::ACTIVE_HEIGHT,
        h_back_porch: P::HBP,
        h_front_porch: P::HFP,
        v_back_porch: P::VBP,
        v_front_porch: P::VFP,
        h_sync: P::HSYNC,
        v_sync: P::VSYNC,
        h_sync_polarity: PolarityActive::ActiveHigh,
        v_sync_polarity: PolarityActive::ActiveHigh,
        data_enable_polarity: PolarityActive::ActiveLow,
        pixel_clock_polarity: PolarityEdge::RisingEdge,
    }
}

/// Controller IDs read over DSI before the panel init (DA / DB / DC).
static ID: [AtomicU8; 3] = [AtomicU8::new(0), AtomicU8::new(0), AtomicU8::new(0)];

/// First pass: the DSI host is up, nothing is initialised yet — read the IDs.
/// Timing constants are the NT35510 ones (the current board revision); an
/// OTM8009A board gets re-timed after detection.
pub struct Detect;

impl DsiPanel for Detect {
    const ACTIVE_WIDTH: u16 = 800;
    const ACTIVE_HEIGHT: u16 = 480;
    // ST's lcd.c and Zephyr both feed the NT35510 the 480×800 timing set in landscape as well
    const HSYNC: u16 = 2;
    const HBP: u16 = 34;
    const HFP: u16 = 34;
    const VSYNC: u16 = 120;
    const VBP: u16 = 150;
    const VFP: u16 = 150;
    const HSYNC_POLARITY: PolarityActive = PolarityActive::ActiveHigh;
    const VSYNC_POLARITY: PolarityActive = PolarityActive::ActiveHigh;
    const DATA_ENABLE_POLARITY: PolarityActive = PolarityActive::ActiveHigh;
    const PIXEL_CLOCK_POLARITY: PolarityEdge = PolarityEdge::FallingEdge;
    const NULL_PACKET_SIZE: u16 = 0xFFF;
    const LOOSELY_PACKED: bool = false;

    fn ltdc_config() -> LtdcConfiguration {
        dsi_ltdc_config::<Self>()
    }

    async fn init<DSI: dsihost::Instance>(
        dsi: &mut DsiHost<'_, DSI>,
        _color: DsiColor,
    ) -> Result<(), dsihost::Error> {
        block_for(Duration::from_millis(20));
        for (i, reg) in [0xDAu8, 0xDB, 0xDC].iter().enumerate() {
            let mut b = [0u8; 1];
            match dsi.read(0, PacketType::DcsShortPktRead(*reg), 1, &mut b) {
                Ok(()) => ID[i].store(b[0], Ordering::Relaxed),
                Err(_) => ID[i].store(0, Ordering::Relaxed),
            }
        }
        Ok(())
    }
}

pub fn detected() -> ([u8; 3], PanelKind) {
    let ids = [
        ID[0].load(Ordering::Relaxed),
        ID[1].load(Ordering::Relaxed),
        ID[2].load(Ordering::Relaxed),
    ];
    // ST BSP: NT35510 answers 0x80 on RDID2 (DB); OTM8009A answers 0x40 on ID1 (DA).
    let kind = if ids[1] == 0x80 {
        PanelKind::Nt35510
    } else {
        PanelKind::Otm8009a
    };
    (ids, kind)
}

/// OTM8009A (older board revisions), init sequence from the `otm8009a` crate.
pub struct Otm8009a;

impl DsiPanel for Otm8009a {
    const ACTIVE_WIDTH: u16 = 800;
    const ACTIVE_HEIGHT: u16 = 480;
    const HSYNC: u16 = 2;
    const HBP: u16 = 34;
    const HFP: u16 = 34;
    const VSYNC: u16 = 1;
    const VBP: u16 = 15;
    const VFP: u16 = 16;
    const HSYNC_POLARITY: PolarityActive = PolarityActive::ActiveHigh;
    const VSYNC_POLARITY: PolarityActive = PolarityActive::ActiveHigh;
    const DATA_ENABLE_POLARITY: PolarityActive = PolarityActive::ActiveHigh;
    const PIXEL_CLOCK_POLARITY: PolarityEdge = PolarityEdge::FallingEdge;
    const NULL_PACKET_SIZE: u16 = 0xFFF;
    const LOOSELY_PACKED: bool = false;

    fn ltdc_config() -> LtdcConfiguration {
        dsi_ltdc_config::<Self>()
    }

    async fn init<DSI: dsihost::Instance>(
        dsi: &mut DsiHost<'_, DSI>,
        _color: DsiColor,
    ) -> Result<(), dsihost::Error> {
        let mut io = DsiIo(dsi);
        let cfg = otm8009a::Otm8009AConfig {
            frame_rate: otm8009a::FrameRate::_60Hz,
            mode: otm8009a::Mode::Landscape,
            color_map: otm8009a::ColorMap::Rgb,
            cols: 800,
            rows: 480,
        };
        otm8009a::Otm8009A::new().init(&mut io, cfg, &mut BlockDelay)
    }
}

/// NT35510 (board rev B03+), sequence from ST's nt35510.c, landscape, RGB888.
pub struct Nt35510;

impl DsiPanel for Nt35510 {
    const ACTIVE_WIDTH: u16 = 800;
    const ACTIVE_HEIGHT: u16 = 480;
    // nt35510.h defines swapped "800×480" macros, but ST's lcd.c and Zephyr use the
    // 480×800 set for landscape too — that is what the panel locks to
    const HSYNC: u16 = 2;
    const HBP: u16 = 34;
    const HFP: u16 = 34;
    const VSYNC: u16 = 120;
    const VBP: u16 = 150;
    const VFP: u16 = 150;
    const HSYNC_POLARITY: PolarityActive = PolarityActive::ActiveHigh;
    const VSYNC_POLARITY: PolarityActive = PolarityActive::ActiveHigh;
    const DATA_ENABLE_POLARITY: PolarityActive = PolarityActive::ActiveHigh;
    const PIXEL_CLOCK_POLARITY: PolarityEdge = PolarityEdge::FallingEdge;
    const NULL_PACKET_SIZE: u16 = 0xFFF;
    const LOOSELY_PACKED: bool = false;

    fn ltdc_config() -> LtdcConfiguration {
        dsi_ltdc_config::<Self>()
    }

    async fn init<DSI: dsihost::Instance>(
        dsi: &mut DsiHost<'_, DSI>,
        _color: DsiColor,
    ) -> Result<(), dsihost::Error> {
        // Byte-for-byte the legacy ST driver (stm32-nt35510 v1.0.0, the one the F769 board
        // package calls): `DSI_IO_WriteCmd(n, p)` sends a DCS short write P1 for n ≤ 1 — so the
        // "no parameter" commands go out WITH a 0x00 parameter — and a long write otherwise.
        let w = |dsi: &mut DsiHost<'_, DSI>, reg: u8, data: &[u8]| dsi.write_cmd(0, reg, data);
        Timer::after_millis(120).await;
        w(dsi, 0xF0, &[0x55, 0xAA, 0x52, 0x08, 0x01])?; // LV2: page 1
        w(dsi, 0xB0, &[0x03, 0x03, 0x03])?; // AVDD 5.2 V
        w(dsi, 0xB6, &[0x46, 0x46, 0x46])?;
        w(dsi, 0xB1, &[0x03, 0x03, 0x03])?; // AVEE -5.2 V
        w(dsi, 0xB7, &[0x36, 0x36, 0x36])?;
        w(dsi, 0xB2, &[0x00, 0x00, 0x02])?; // VCL -2.5 V
        w(dsi, 0xB8, &[0x26, 0x26, 0x26])?;
        w(dsi, 0xBF, &[0x01])?; // VGH 15 V
        w(dsi, 0xB3, &[0x09, 0x09, 0x09])?;
        w(dsi, 0xB9, &[0x36, 0x36, 0x36])?;
        w(dsi, 0xB5, &[0x08, 0x08, 0x08])?; // VGL_REG -10 V
        w(dsi, 0xBA, &[0x26, 0x26, 0x26])?;
        w(dsi, 0xBC, &[0x00, 0x80, 0x00])?; // VGMP/VGSP
        w(dsi, 0xBD, &[0x00, 0x80, 0x00])?; // VGMN/VGSN
        w(dsi, 0xBE, &[0x00, 0x50])?; // VCOM
        w(dsi, 0xF0, &[0x55, 0xAA, 0x52, 0x08, 0x00])?; // LV2: page 0
        w(dsi, 0xB1, &[0xFC, 0x00])?; // display control
        w(dsi, 0xB6, &[0x03])?; // source hold time
        w(dsi, 0xB5, &[0x51])?; // resolution control (legacy value)
        w(dsi, 0xB7, &[0x00, 0x00])?; // gate EQ
        w(dsi, 0xB8, &[0x01, 0x02, 0x02, 0x02])?; // source EQ
        w(dsi, 0xBC, &[0x00, 0x00, 0x00])?; // inversion
        w(dsi, 0xCC, &[0x03, 0x00, 0x00])?;
        w(dsi, 0xBA, &[0x01])?;
        w(dsi, 0x35, &[0x00])?; // tearing effect on
        w(dsi, 0x3A, &[0x77])?; // COLMOD RGB888
        Timer::after_millis(200).await;
        w(dsi, 0x36, &[0xA0])?; // MADCTL landscape, rotated 180° (MY|MV; ST's 0x60 = MX|MV)
        w(dsi, 0x2A, &[0x00, 0x00, 0x03, 0x1F])?; // CASET 0..799
        w(dsi, 0x2B, &[0x00, 0x00, 0x01, 0xDF])?; // RASET 0..479
        w(dsi, 0x11, &[0x00])?; // sleep out
        Timer::after_millis(120).await;
        w(dsi, 0x3A, &[0x77])?; // COLMOD again
        w(dsi, 0x51, &[0x7F])?; // brightness
        w(dsi, 0x53, &[0x2C])?; // CTRL display: BL on, dimming, BCTRL
        w(dsi, 0x55, &[0x02])?; // CABC still picture
        w(dsi, 0x5E, &[0xFF])?; // CABC min brightness
        w(dsi, 0x29, &[0x00])?; // display on
        w(dsi, 0x2C, &[0x00])?; // RAMWR: frames from the LTDC stream follow
        Ok(())
    }
}

/// Adapter: the `otm8009a` crate talks to a `DsiHostCtrlIo`; map it onto embassy's host.
struct DsiIo<'a, 'd, T: dsihost::Instance>(&'a mut DsiHost<'d, T>);

impl<T: dsihost::Instance> DsiHostCtrlIo for DsiIo<'_, '_, T> {
    type Error = dsihost::Error;

    fn write(&mut self, command: DsiWriteCommand) -> Result<(), Self::Error> {
        match command {
            DsiWriteCommand::DcsShortP0 { arg } => self.0.write_cmd(0, arg, &[]),
            DsiWriteCommand::DcsShortP1 { arg, data } => self.0.write_cmd(0, arg, &[data]),
            DsiWriteCommand::DcsLongWrite { arg, data } => self.0.write_cmd(0, arg, data),
            DsiWriteCommand::GenericLongWrite { arg, data } => self.0.write_cmd(0, arg, data),
            // not used by the OTM8009A init sequence
            DsiWriteCommand::GenericShortP0
            | DsiWriteCommand::GenericShortP1
            | DsiWriteCommand::GenericShortP2 => Ok(()),
            DsiWriteCommand::SetMaximumReturnPacketSize(_) => Ok(()),
        }
    }

    fn read(&mut self, command: DsiReadCommand, buf: &mut [u8]) -> Result<(), Self::Error> {
        match command {
            DsiReadCommand::DcsShort { arg } => {
                self.0
                    .read(0, PacketType::DcsShortPktRead(arg), buf.len() as u16, buf)
            }
            _ => Ok(()),
        }
    }
}

/// Blocking millisecond delay for the `otm8009a` crate (embedded-hal 0.2).
struct BlockDelay;

impl embedded_hal_02::blocking::delay::DelayMs<u32> for BlockDelay {
    fn delay_ms(&mut self, ms: u32) {
        block_for(Duration::from_millis(ms as u64));
    }
}

// ---------------------------------------------------------------------------
// Bring-up
// ---------------------------------------------------------------------------

pub struct Display {
    pub ltdc: Ltdc<'static, LTDC, DSI>,
    #[allow(dead_code)] // kept alive: dropping the host would stop the link
    pub dsi: DsiHost<'static, DSIHOST>,
    pub fb: [Framebuffer; 2],
    pub panel: PanelKind,
    #[allow(dead_code)]
    pub ids: [u8; 3],
    /// BL_CTRL (PI14): held high for as long as the display lives.
    pub backlight: Option<Output<'static>>,
}

fn layer() -> LtdcLayerConfig {
    LtdcLayerConfig {
        layer: LtdcLayer::Layer1,
        pixel_format: PixelFormat::Rgb565,
        window_x0: 0,
        window_x1: WIDTH as u16,
        window_y0: 0,
        window_y1: HEIGHT as u16,
    }
}

fn video_mode() -> DsiHostMode {
    // ST BSP: burst mode, RGB888, LP transitions allowed everywhere, commands in LP
    DsiHostMode::Video(DsiVideoConfig {
        mode: DsiVideoMode::Burst,
        color: DsiColor::Rgb888,
        channel: 0,
        bta: false,
        lpvsa: true,
        lpvbp: true,
        lpvfp: true,
        lpva: true,
        lphbp: true,
        lphfp: true,
        lpcmd: true,
    })
}

/// DSI video timing in lane-byte-clock units, the way ST's BSP computes it
/// (`HSA * laneByteClk / LcdClock`). embassy derives these from
/// `LTDC::frequency()`, which on the F7 is the APB2 bus clock (108 MHz), not
/// the PLLSAI pixel clock — the line comes out far too short and the link
/// never gets its low-power blanking, so every command write times out.
const LANE_BYTE_CLK_KHZ: u64 = 62_500;
const LCD_CLK_KHZ: u64 = 27_429;

fn fix_video_timing<P: DsiPanel>() {
    let r = embassy_stm32::pac::DSIHOST;
    let t = |px: u16| (px as u64 * LANE_BYTE_CLK_KHZ / LCD_CLK_KHZ) as u16;
    r.vhsacr().modify(|w| w.set_hsa(t(P::HSYNC)));
    r.vhbpcr().modify(|w| w.set_hbp(t(P::HBP)));
    r.vlcr().modify(|w| w.set_hline(t(P::HLINE_TOTAL)));
    r.vvsacr().modify(|w| w.set_vsa(P::VSYNC));
    r.vvbpcr().modify(|w| w.set_vbp(P::VBP));
    r.vvfpcr().modify(|w| w.set_vfp(P::VFP));
    r.vvacr().modify(|w| w.set_va(P::ACTIVE_HEIGHT));
    // ST: largest LP packet 16 in blanking, 0 during active video
    r.lpmcr().modify(|w| {
        w.set_lpsize(16);
        w.set_vlpsize(0);
    });
    let rcc = embassy_stm32::pac::RCC;
    let cfg = rcc.pllsaicfgr().read();
    info!(
        "DSI video timing: hsa {} hbp {} hline {} lane clk; PLLSAI n={} r={} divr={} (pixel clock {} kHz assumed)",
        t(P::HSYNC), t(P::HBP), t(P::HLINE_TOTAL), cfg.plln(), cfg.pllr(), rcc.dckcfgr1().read().pllsaidivr().to_bits(), LCD_CLK_KHZ
    );
}

/// Reset the panel, start LTDC + DSI in video mode, detect and init the
/// controller, and hand back both framebuffers (fb0 is on screen, cleared).
pub async fn init_display(
    mut ltdc: Ltdc<'static, LTDC, DSI>,
    mut dsi: DsiHost<'static, DSIHOST>,
    reset: &mut Output<'static>,
) -> Result<Display, dsihost::Error> {
    // XRES pulse (ST: 20 ms low, 10 ms after release)
    reset.set_low();
    Timer::after_millis(20).await;
    reset.set_high();
    Timer::after_millis(200).await; // Zephyr waits 200 ms here (ST: 10 ms)

    let mut fb0 = unsafe { Framebuffer::at(FB0_ADDR) };
    let mut fb1 = unsafe { Framebuffer::at(FB1_ADDR) };
    fb0.fb().fill(0);
    fb1.fb().fill(0);

    ltdc.init(&Detect::ltdc_config());
    ltdc.init_layer(&layer(), None);
    ltdc.init_buffer(LtdcLayer::Layer1, fb0.as_ptr().cast());
    ltdc.enable();
    reload_now();

    // ST BSP PHY timings for this board: HS2LP/LP2HS 35, stop wait 10; BTA on for reads
    let phy = DsiHostPhyConfig {
        lanes: DsiHostPhyLanes::Two,
        stop_wait_time: 10,
        acr: false,
        crc_rx: false,
        ecc_rx: false,
        eotp_rx: false,
        eotp_tx: false,
        bta: true,
        clock_hs2lp: 35,
        clock_lp2hs: 35,
        data_hs2lp: 35,
        data_lp2hs: 35,
        data_mrd: 0,
    };
    let mode = video_mode();
    dsi.start_panel::<Detect>(&phy, &mode).await?;
    fix_video_timing::<Detect>();
    let (ids, panel) = detected();
    info!(
        "DSI up (v{:#x}); panel IDs {:#04x} {:#04x} {:#04x} → {}",
        dsi.get_version(),
        ids[0],
        ids[1],
        ids[2],
        panel
    );

    match panel {
        PanelKind::Nt35510 => dsi.init_panel::<Nt35510>(DsiColor::Rgb888).await?,
        PanelKind::Otm8009a => {
            // different porches: put the LTDC back into its reset state (embassy's
            // `init` asserts it), re-time it and the DSI video mode, then init
            ltdc.disable();
            embassy_stm32::pac::LTDC
                .gcr()
                .write_value(embassy_stm32::pac::ltdc::regs::Gcr(0x2220));
            ltdc.init(&Otm8009a::ltdc_config());
            ltdc.init_layer(&layer(), None);
            ltdc.init_buffer(LtdcLayer::Layer1, fb0.as_ptr().cast());
            ltdc.enable();
            reload_now();
            dsi.disable_wrapper_dsi();
            dsi.disable();
            dsi.set_mode::<Otm8009a>(&mode)?;
            fix_video_timing::<Otm8009a>();
            dsi.enable();
            dsi.enable_wrapper_dsi();
            dsi.init_panel::<Otm8009a>(DsiColor::Rgb888).await?;
        }
    }
    // test pattern: grey field with white / red / green / blue bars, then a status dump
    {
        let mut fb = fb0.fb();
        fb.fill(rgb565(0x60, 0x60, 0x60));
        for (i, c) in [
            rgb565(255, 255, 255),
            rgb565(255, 0, 0),
            rgb565(0, 255, 0),
            rgb565(0, 0, 255),
        ]
        .iter()
        .enumerate()
        {
            fb.rect(100 + i as i32 * 160, 100, 120, 280, *c);
        }
    }
    Timer::after_millis(500).await;
    dump_status("after panel init");
    // read back what the init should have set: power mode (0x0A: bit4 sleep-out, bit2 display on),
    // MADCTL (0x0B), COLMOD (0x0C), brightness (0x52), CTRL display (0x54)
    let mut rb = [0u8; 5];
    for (i, reg) in [0x0Au8, 0x0B, 0x0C, 0x52, 0x54].iter().enumerate() {
        let mut b = [0u8; 1];
        rb[i] = match dsi.read(0, PacketType::DcsShortPktRead(*reg), 1, &mut b) {
            Ok(()) => b[0],
            Err(_) => 0xEE,
        };
    }
    info!("panel readback: RDDPM={:#04x} MADCTL={:#04x} COLMOD={:#04x} RDDISBV={:#04x} RDCTRLD={:#04x} (0xEE = read failed)", rb[0], rb[1], rb[2], rb[3], rb[4]);
    Timer::after_millis(2500).await;
    dump_status("2.5 s later");
    Ok(Display {
        ltdc,
        dsi,
        fb: [fb0, fb1],
        panel,
        ids,
        backlight: None,
    })
}

/// Layer registers are shadowed: make the layer config + framebuffer address
/// live now (immediate reload) and make sure the layer is enabled.
fn reload_now() {
    let l = embassy_stm32::pac::LTDC;
    l.layer(0).cr().modify(|w| w.set_len(true));
    l.srcr()
        .write(|w| w.set_imr(embassy_stm32::pac::ltdc::vals::Imr::Reload));
}

/// LTDC / DSI register snapshot for bring-up: is the controller scanning, does the
/// pixel FIFO underrun, does the PHY report errors, is the wrapper streaming.
pub fn dump_status(when: &str) {
    let l = embassy_stm32::pac::LTDC;
    let d = embassy_stm32::pac::DSIHOST;
    let isr = l.isr().read();
    let cpsr = l.cpsr().read();
    let cdsr = l.cdsr().read();
    info!(
        "LTDC {}: en={} layer1 en={} pos x={} y={} isr fuif={} terrif={} rrif={} lif={} cdsr vsync={} hsync={} vdes={} hdes={} cfbar={:#x}",
        when,
        l.gcr().read().ltdcen(),
        l.layer(0).cr().read().len(),
        cpsr.cxpos(),
        cpsr.cypos(),
        isr.fuif(),
        isr.terrif(),
        isr.rrif(),
        isr.lif(),
        cdsr.vsyncs(),
        cdsr.hsyncs(),
        cdsr.vdes(),
        cdsr.hdes(),
        l.layer(0).cfbar().read().cfbadd()
    );
    info!(
        "DSI {}: cr en={} wcr dsien={} isr0={:#x} isr1={:#x} wisr={:#x} vmcr={:#x} wcfgr={:#x} lcolcr={:#x}",
        when,
        d.cr().read().en(),
        d.wcr().read().dsien(),
        d.isr0().read().0,
        d.isr1().read().0,
        d.wisr().read().0,
        d.vmcr().read().0,
        d.wcfgr().read().0,
        d.lcolcr().read().0
    );
}
