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
