//! The Ethernet link (LAN8742 PHY over RMII, DHCP) and the question/answer transport to the host.

use core::mem::MaybeUninit;
use embassy_net::udp::UdpSocket;
use embassy_net::wire::Ipv4Addr;
use embassy_net::{Runner, Stack, StackStorage};
use embassy_stm32::eth::{Ethernet, GenericPhy, PacketQueue, Sma};
use embassy_stm32::peripherals::{ETH, ETH_SMA};
use static_cell::StaticCell;

pub type Device = Ethernet<'static, ETH, GenericPhy<Sma<'static, ETH_SMA>>>;

/// The packet queue lives in the non-cacheable SRAM1 window (`ethbuf.x`, `display::ETH_BUF_ADDR`).
#[link_section = ".ethbuf"]
static mut PACKETS: MaybeUninit<PacketQueue<4, 4>> = MaybeUninit::uninit();

/// The stack's storage holds the packet buffers the Ethernet DMA reads and writes: same window.
#[link_section = ".ethbuf"]
static mut STORAGE: MaybeUninit<StackStorage<'static>> = MaybeUninit::uninit();
pub static DEVICE: StaticCell<Device> = StaticCell::new();

/// Zeroes the `.ethbuf` section (`ethbuf.x`). It is NOLOAD, so cortex-m-rt leaves it alone, but it
/// holds xarxa-driver's packet pool, whose bitmap must start at zero: SRAM1 keeps the bits of the
/// previous run across a reset, and the RX ring then finds the pool "exhausted".
///
/// # Safety
/// Call once, first thing in `main`, before anything uses the packet pool or the statics here.
pub unsafe fn zero_ethbuf() {
    extern "C" {
        static mut __sethbuf: u32;
        static mut __eethbuf: u32;
    }
    let mut p = core::ptr::addr_of_mut!(__sethbuf);
    let end = core::ptr::addr_of_mut!(__eethbuf);
    while p < end {
        p.write_volatile(0);
        p = p.add(1);
    }
}

/// The stack storage, initialised once.
///
/// # Safety
/// Call once.
pub unsafe fn storage() -> &'static mut StackStorage<'static> {
    let slot = &mut *core::ptr::addr_of_mut!(STORAGE);
    slot.write(StackStorage::new())
}

/// The queue, initialised once.
///
/// # Safety
/// Call once.
pub unsafe fn packets() -> &'static mut PacketQueue<4, 4> {
    let slot = &mut *core::ptr::addr_of_mut!(PACKETS);
    slot.write(PacketQueue::<4, 4>::new())
}

#[embassy_executor::task]
pub async fn net_task(mut runner: Runner<'static>) -> ! {
    runner.run().await
}

/// UDP port the host script listens on (questions are broadcast, answers come back unicast).
pub const HOST_PORT: u16 = 7878;

pub fn open_socket(stack: Stack<'static>) -> Option<UdpSocket<'static>> {
    let mut socket = UdpSocket::new(stack).ok()?;
    socket.bind(7879u16).ok()?;
    Some(socket)
}

pub const BROADCAST: (Ipv4Addr, u16) = (Ipv4Addr::BROADCAST, HOST_PORT);

/// UDP port of the board's command server (see `scripts/board_ctl.py`).
pub const COMMAND_PORT: u16 = 7880;

/// The frame rate main.rs measures, for `!info`.
pub static FPS: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(0);

/// Reply text under construction.
struct Out {
    bytes: [u8; 96],
    len: usize,
}

impl core::fmt::Write for Out {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let b = s.as_bytes();
        let n = b.len().min(self.bytes.len() - self.len);
        self.bytes[self.len..self.len + n].copy_from_slice(&b[..n]);
        self.len += n;
        Ok(())
    }
}

/// Answers commands from the host, whose address it learns from the datagram:
///
/// * `!ping N` -> `!pong N`: round trip and loss
/// * `!info` -> uptime, frame rate, build
/// * `!blast N` -> N datagrams of 512 bytes (`!blast I` + padding): throughput and loss
/// * `!reset` -> restarts the board (after `!reset`), no ST-LINK needed
#[embassy_executor::task]
pub async fn command_task(stack: Stack<'static>) -> ! {
    use core::fmt::Write;
    use core::sync::atomic::Ordering;
    let mut socket = match UdpSocket::new(stack) {
        Ok(s) => s,
        Err(_) => loop {
            embassy_time::Timer::after_secs(3600).await;
        },
    };
    let _ = socket.bind(COMMAND_PORT);
    loop {
        let mut cmd = [0u8; 48];
        let Ok((n, from)) = socket
            .recv_from_with(|data, meta| {
                let n = data.len().min(cmd.len());
                cmd[..n].copy_from_slice(&data[..n]);
                (n, meta)
            })
            .await
        else {
            continue;
        };
        let text = core::str::from_utf8(&cmd[..n]).unwrap_or("").trim();
        let mut words = text.split_whitespace();
        let mut out = Out {
            bytes: [0; 96],
            len: 0,
        };
        match words.next() {
            Some("!ping") => {
                let _ = write!(out, "!pong {}", words.next().unwrap_or("0"));
            }
            Some("!info") => {
                let _ = write!(
                    out,
                    "!info {} {} uptime {} ms, {} fps",
                    env!("CARGO_PKG_NAME"),
                    env!("CARGO_PKG_VERSION"),
                    embassy_time::Instant::now().as_millis(),
                    FPS.load(Ordering::Relaxed)
                );
            }
            Some("!blast") => {
                let count: u32 = words
                    .next()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0)
                    .min(2000);
                let mut packet = [b'.'; 512];
                for i in 0..count {
                    let mut head = Out {
                        bytes: [0; 96],
                        len: 0,
                    };
                    let _ = write!(head, "!blast {i} ");
                    packet[..head.len].copy_from_slice(&head.bytes[..head.len]);
                    let _ = socket.send_to(&packet, from).await;
                }
                let _ = write!(out, "!blast done {count}");
            }
            Some("!reset") => {
                let _ = socket.send_to(b"!reset now", from).await;
                embassy_time::Timer::after_millis(100).await;
                cortex_m::peripheral::SCB::sys_reset();
            }
            _ => {
                let _ = write!(out, "!? {text}");
            }
        }
        if out.len > 0 {
            let _ = socket.send_to(&out.bytes[..out.len], from).await;
        }
    }
}
