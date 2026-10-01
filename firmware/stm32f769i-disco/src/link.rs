//! The debug-probe link (`geodb_fw_core::link`): the block at the start of the non-cacheable
//! SRAM1 window, so the host reads the screen's state and sends touches and queries through the
//! ST-LINK while the program runs (`geodb-board bridge --probe`). Volatile word accesses only:
//! no atomics on that memory.

use core::ptr::addr_of_mut;
use geodb_fw_core::link::{self, Command, WORDS};
use geodb_fw_core::ui::STATE_LEN;
use geodb_fw_core::Answer;

/// Linked first into the window (`ethbuf.x`), so at `link::ADDR`; zeroed by `net::clear_buffers`.
#[link_section = ".geodb_link"]
#[used]
static mut BLOCK: [u32; WORDS] = [0; WORDS];

fn word(i: usize) -> *mut u32 {
    unsafe { addr_of_mut!(BLOCK).cast::<u32>().add(i) }
}

fn get(i: usize) -> u32 {
    unsafe { word(i).read_volatile() }
}

fn set(i: usize, v: u32) {
    unsafe { word(i).write_volatile(v) }
}

/// After `net::clear_buffers`: the magic tells the host the block is live.
pub fn init() {
    defmt::assert_eq!(
        word(0) as u32,
        link::ADDR,
        "the link block is not at its address"
    );
    set(link::W_MAGIC, link::MAGIC);
}

/// The screen's state for the host; the sequence moves only when it changed.
pub fn publish(packet: &[u8; STATE_LEN]) {
    let w = link::state_words(packet);
    if (0..w.len()).all(|i| get(link::W_STATE + i) == w[i]) {
        return;
    }
    for (i, v) in w.iter().enumerate() {
        set(link::W_STATE + i, *v);
    }
    cortex_m::asm::dmb();
    set(link::W_SEQ, get(link::W_SEQ).wrapping_add(1));
}

/// A new command of the host, if there is one; [`done`] when it is handled.
pub fn poll() -> Option<Command> {
    let seq = get(link::W_HOST_SEQ);
    if seq == get(link::W_ACK) {
        return None;
    }
    cortex_m::asm::dmb();
    let w = [0, 1, 2, 3].map(|i| get(link::W_COMMAND + i));
    let cmd = Command::decode(w);
    if cmd.is_none() {
        done(None);
    }
    cmd
}

/// The command is handled (with the answer of a query).
pub fn done(answer: Option<&Answer>) {
    if let Some(a) = answer {
        for (i, v) in link::answer_words(a).iter().enumerate() {
            set(link::W_ANSWER + i, *v);
        }
    }
    cortex_m::asm::dmb();
    set(link::W_ACK, get(link::W_HOST_SEQ));
}
