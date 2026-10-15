//! The bootloader of the geodb board: two program slots (A and B, one 256 KB sector each) and a
//! small log of boot records in sector 1. After an update the new slot is tried (at most three
//! boots); the program confirms itself once it runs and has its network up, else the other slot
//! (the last known good one) starts again.
//!
//! Flash layout (STM32F769, 2 MB, single bank):
//!   0x0800_0000  32 KB   sector 0     this bootloader
//!   0x0800_8000  32 KB   sector 1     boot records (16 bytes each, append only)
//!   0x0804_0000  256 KB  sector 5     slot A
//!   0x0808_0000  256 KB  sector 6     slot B
//!   0x080C_0000  1.25 MB sectors 7-11 the city image (geodb.fw)
//!
//! A boot record is four words: `META_MAGIC`, sequence, slot (0 = A, 1 = B), flags (low byte
//! state: 1 = pending, 2 = confirmed; second byte = boots tried; bits 16..31 a check of seq and slot,
//! never 0xFFFF). The magic is written last and the check must match, so a record cut off by a power
//! loss does not count. Erased flash is 0xFFFF_FFFF, the log only ever
//! appends, nothing here erases.

#![no_std]
#![no_main]

use cortex_m_rt::entry;
use panic_halt as _;

const SLOT: [u32; 2] = [0x0804_0000, 0x0808_0000];
const SLOT_LEN: u32 = 0x4_0000;
const META: u32 = 0x0800_8000;
const META_LEN: u32 = 0x8000;
const META_MAGIC: u32 = 0x4D45_5441; // "META"
const PENDING: u32 = 1;
const CONFIRMED: u32 = 2;
const MAX_TRIES: u32 = 3;

const FLASH_REGS: u32 = 0x4002_3C00;
const KEYR: *mut u32 = (FLASH_REGS + 0x04) as *mut u32;
const SR: *mut u32 = (FLASH_REGS + 0x0C) as *mut u32;
const CR: *mut u32 = (FLASH_REGS + 0x10) as *mut u32;

#[derive(Clone, Copy)]
struct Record {
    seq: u32,
    slot: u32,
    state: u32,
    tries: u32,
}

fn word(addr: u32) -> u32 {
    unsafe { (addr as *const u32).read_volatile() }
}

/// The latest record and the address where the next one goes (`None` when the log is full).
fn read_log() -> (Option<Record>, Option<u32>) {
    let mut last = None;
    let mut at = META;
    while at < META + META_LEN {
        let w = [word(at), word(at + 4), word(at + 8), word(at + 12)];
        if w == [0xFFFF_FFFF; 4] {
            return (last, Some(at));
        }
        if w[0] == META_MAGIC && (w[3] >> 16) == check(w[1], w[2]) {
            last = Some(Record {
                seq: w[1],
                slot: w[2] & 1,
                state: w[3] & 0xFF,
                tries: (w[3] >> 8) & 0xFF,
            });
        }
        at += 16;
    }
    (last, None)
}

/// 15 bits, so it can never equal erased flash (0xFFFF).
fn check(seq: u32, slot: u32) -> u32 {
    (seq ^ slot ^ 0x5A5A) & 0x7FFF
}

fn flash_wait() {
    while unsafe { SR.read_volatile() } & (1 << 16) != 0 {}
}

/// Programs one word (the flash must have been erased there).
fn program(addr: u32, value: u32) {
    unsafe {
        flash_wait();
        // PSIZE = 32 bit (0b10), PG
        CR.write_volatile((0b10 << 8) | 1);
        (addr as *mut u32).write_volatile(value);
        cortex_m::asm::dsb();
        flash_wait();
        CR.write_volatile(0);
    }
}

fn unlock() {
    unsafe {
        if CR.read_volatile() & (1 << 31) != 0 {
            KEYR.write_volatile(0x4567_0123);
            KEYR.write_volatile(0xCDEF_89AB);
        }
    }
}

fn append(at: u32, r: Record) {
    unlock();
    program(at + 4, r.seq);
    program(at + 8, r.slot);
    program(
        at + 12,
        r.state | (r.tries << 8) | (check(r.seq, r.slot) << 16),
    );
    program(at, META_MAGIC); // last: only a whole record counts
    unsafe { CR.write_volatile(1 << 31) }; // lock
}

/// Plausible program: a stack pointer in RAM and a reset vector inside the slot.
fn slot_ok(slot: u32) -> bool {
    let base = SLOT[slot as usize];
    let (sp, reset) = (word(base), word(base + 4));
    (0x2000_0000..=0x2008_0000).contains(&sp) && (base..base + SLOT_LEN).contains(&reset)
}

fn jump(slot: u32) -> ! {
    let base = SLOT[slot as usize];
    unsafe {
        let p = cortex_m::Peripherals::steal();
        p.SCB.vtor.write(base);
        cortex_m::asm::bootstrap(word(base) as *const u32, word(base + 4) as *const u32)
    }
}

#[entry]
fn main() -> ! {
    let (last, next) = read_log();
    let mut boot = 0u32; // no record: slot A
    match (last, next) {
        (Some(r), Some(at)) if r.state == PENDING => {
            if r.tries < MAX_TRIES && slot_ok(r.slot) {
                // a trial boot: count it, the program confirms itself when it is up
                append(
                    at,
                    Record {
                        seq: r.seq + 1,
                        tries: r.tries + 1,
                        ..r
                    },
                );
                boot = r.slot;
            } else {
                // the new program did not come up: back to the last known good slot
                boot = 1 - r.slot;
                append(
                    at,
                    Record {
                        seq: r.seq + 1,
                        slot: boot,
                        state: CONFIRMED,
                        tries: 0,
                    },
                );
            }
        }
        (Some(r), _) => boot = r.slot,
        (None, _) => {}
    }
    if !slot_ok(boot) && slot_ok(1 - boot) {
        boot = 1 - boot;
    }
    if slot_ok(boot) {
        jump(boot);
    }
    // nothing to start: wait for the ST-LINK
    loop {
        cortex_m::asm::wfi();
    }
}
