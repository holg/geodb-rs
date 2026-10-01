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

extern "C" {
    static mut __sethbuf: u32;
    static mut __eethbuf: u32;
}

/// Zeroes the `.ethbuf` window. It is NOLOAD (cortex-m-rt only clears `.bss`), and the packet pool
/// inside it is a zero-initialised static: left as it powers up it reads as "exhausted".
///
/// # Safety
/// Call once, first, after the MPU region makes the window non-cacheable.
pub unsafe fn clear_buffers() {
    let mut at = core::ptr::addr_of_mut!(__sethbuf);
    let end = core::ptr::addr_of_mut!(__eethbuf);
    while at < end {
        at.write_volatile(0);
        at = at.add(1);
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

/// UDP port of the state packets for the viewer (`cargo run --example mirror`).
pub const STATE_PORT: u16 = 7881;
pub const STATE_BROADCAST: (Ipv4Addr, u16) = (Ipv4Addr::BROADCAST, STATE_PORT);

pub const BROADCAST: (Ipv4Addr, u16) = (Ipv4Addr::BROADCAST, HOST_PORT);

/// UDP port of the board's command server (see `geodb-board`).
pub const COMMAND_PORT: u16 = 7880;

/// Touch input injected from the host (the mirror window): drag deltas accumulate, a tap is the last
/// tap (bit 31 set, x in bits 16..26, y in bits 0..10), release ends a drag. main.rs takes them.
pub static INJ_DX: core::sync::atomic::AtomicI32 = core::sync::atomic::AtomicI32::new(0);
pub static INJ_DY: core::sync::atomic::AtomicI32 = core::sync::atomic::AtomicI32::new(0);
pub static INJ_TAP: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(0);
pub static INJ_RELEASE: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);

/// The frame rate main.rs measures, for `!info`.
/// Set by main.rs when the network has an address.
pub static NET_UP: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);

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
/// * `!tap X Y`, `!drag DX DY`, `!release` -> touch input from the mirror window (no reply)
/// * `!ota begin LEN CRC32HEX` -> erases the other slot; data chunks (`0x01`, offset u32 LE, bytes)
///   -> `!ota ack NEXT`; `!ota end` -> checks the CRC, marks the slot pending, restarts
/// * `!reset` -> restarts the board (after `!reset`), no ST-LINK needed
#[embassy_executor::task]
pub async fn command_task(stack: Stack<'static>, mut ota: crate::ota::Ota) -> ! {
    use core::fmt::Write;
    use core::sync::atomic::Ordering;
    let mut socket = match UdpSocket::new(stack) {
        Ok(s) => s,
        Err(_) => loop {
            embassy_time::Timer::after_secs(3600).await;
        },
    };
    let _ = socket.bind(COMMAND_PORT);
    let mut up_since: Option<embassy_time::Instant> = None;
    loop {
        let mut cmd = [0u8; 1100];
        // (a second at most, so that the confirmation below runs without any traffic)
        let received = embassy_time::with_timeout(
            embassy_time::Duration::from_secs(1),
            socket.recv_from_with(|data, meta| {
                let n = data.len().min(cmd.len());
                cmd[..n].copy_from_slice(&data[..n]);
                (n, meta)
            }),
        )
        .await;
        // A program that came up from an update confirms itself once the network has been up for a
        // while, else the bootloader goes back to the old slot after three tries.
        if NET_UP.load(Ordering::Relaxed) {
            let since = *up_since.get_or_insert_with(embassy_time::Instant::now);
            if since.elapsed() >= embassy_time::Duration::from_secs(20) && ota.confirm() {
                defmt::info!("update confirmed (slot {})", crate::ota::my_slot());
            }
        }
        let Ok(Ok((n, from))) = received else {
            continue;
        };
        // a data chunk of an update: 0x01, offset (u32 LE), bytes
        if n >= 5 && cmd[0] == 1 {
            let offset = u32::from_le_bytes([cmd[1], cmd[2], cmd[3], cmd[4]]);
            let mut out = Out {
                bytes: [0; 96],
                len: 0,
            };
            match ota.data(offset, &cmd[5..n]) {
                Ok(next) => {
                    let _ = write!(out, "!ota ack {next}");
                }
                Err(e) => {
                    let _ = write!(out, "!ota error {e}");
                }
            }
            let _ = socket.send_to(&out.bytes[..out.len], from).await;
            continue;
        }
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
                    "!info {} {} slot {} uptime {} ms, {} fps",
                    env!("CARGO_PKG_NAME"),
                    env!("CARGO_PKG_VERSION"),
                    if crate::ota::my_slot() == 0 { 'A' } else { 'B' },
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
            Some("!tap") => {
                let x: u32 = words
                    .next()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0)
                    .min(799);
                let y: u32 = words
                    .next()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0)
                    .min(479);
                INJ_TAP.store(1 << 31 | x << 16 | y, Ordering::Relaxed);
            }
            Some("!drag") => {
                let dx: i32 = words.next().and_then(|v| v.parse().ok()).unwrap_or(0);
                let dy: i32 = words.next().and_then(|v| v.parse().ok()).unwrap_or(0);
                INJ_DX.fetch_add(dx.clamp(-800, 800), Ordering::Relaxed);
                INJ_DY.fetch_add(dy.clamp(-480, 480), Ordering::Relaxed);
            }
            Some("!release") => INJ_RELEASE.store(true, Ordering::Relaxed),
            Some("!ota") => match words.next() {
                Some("begin") => {
                    let len: u32 = words.next().and_then(|v| v.parse().ok()).unwrap_or(0);
                    let crc = words
                        .next()
                        .and_then(|v| u32::from_str_radix(v, 16).ok())
                        .unwrap_or(0);
                    // (erasing the slot stalls the chip for a second or two)
                    match ota.begin(len, crc) {
                        Ok(slot) => {
                            let _ = write!(out, "!ota ready {slot}");
                        }
                        Err(e) => {
                            let _ = write!(out, "!ota error {e}");
                        }
                    }
                }
                Some("end") => match ota.end() {
                    Ok(()) => {
                        let _ = socket.send_to(b"!ota done: restarting", from).await;
                        embassy_time::Timer::after_millis(300).await;
                        crate::ota::reboot();
                    }
                    Err(e) => {
                        let _ = write!(out, "!ota error {e}");
                    }
                },
                _ => {
                    let _ = write!(out, "!ota ?");
                }
            },
            Some("!reset") => {
                let _ = socket.send_to(b"!reset now", from).await;
                embassy_time::Timer::after_millis(100).await;
                crate::ota::reboot();
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
