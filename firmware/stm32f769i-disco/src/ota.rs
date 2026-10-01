//! Updates over the network. The program for the *other* flash slot arrives in UDP datagrams (see
//! `net::command_task` and `geodb-board ota`), is written there, checked against a CRC-32 and
//! marked "pending" in the boot log; the bootloader (../bootloader) then starts it. It has to
//! confirm itself (`Ota::confirm`, once it runs and the network is up), else the bootloader goes
//! back to this slot after three tries.
//!
//! The record format is the bootloader's: four words, magic last, check in bits 16..31 of the flags.

use embassy_stm32::flash::{Blocking, Flash, WRITE_SIZE};

/// The two program slots (one 256 KB sector each) and where the boot log is.
pub const SLOT: [u32; 2] = [0x0804_0000, 0x0808_0000];
pub const SLOT_LEN: u32 = 0x4_0000;
const FLASH_BASE: u32 = 0x0800_0000;
const META: u32 = 0x0800_8000;
const META_LEN: u32 = 0x8000;
const META_MAGIC: u32 = 0x4D45_5441;
const PENDING: u32 = 1;
const CONFIRMED: u32 = 2;

/// The slot this program runs from (it is linked for one of them: `GEODB_SLOT`).
pub fn my_slot() -> u32 {
    u32::from(my_slot as *const () as usize as u32 >= SLOT[1])
}

fn word(addr: u32) -> u32 {
    unsafe { (addr as *const u32).read_volatile() }
}

fn check(seq: u32, slot: u32) -> u32 {
    (seq ^ slot ^ 0x5A5A) & 0x7FFF
}

#[derive(Clone, Copy)]
pub struct Record {
    pub seq: u32,
    pub slot: u32,
    pub state: u32,
    pub tries: u32,
}

/// The latest valid record and where the next one goes (`None`: the log is full).
pub fn read_log() -> (Option<Record>, Option<u32>) {
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

/// CRC-32 (IEEE, as zlib and the host tool compute it).
pub fn crc32(data: &[u8]) -> u32 {
    const TABLE: [u32; 256] = {
        let mut t = [0u32; 256];
        let mut i = 0;
        while i < 256 {
            let mut c = i as u32;
            let mut k = 0;
            while k < 8 {
                c = if c & 1 != 0 {
                    0xEDB8_8320 ^ (c >> 1)
                } else {
                    c >> 1
                };
                k += 1;
            }
            t[i] = c;
            i += 1;
        }
        t
    };
    let mut crc = 0xFFFF_FFFFu32;
    for &b in data {
        crc = TABLE[((crc ^ u32::from(b)) & 0xFF) as usize] ^ (crc >> 8);
    }
    !crc
}

pub struct Ota {
    flash: Flash<'static, Blocking>,
    /// (target slot, length, crc, next expected offset) of an update in progress.
    update: Option<(u32, u32, u32, u32)>,
    confirmed: bool,
}

impl Ota {
    pub fn new(flash: Flash<'static, Blocking>) -> Self {
        // A program that was started without a pending record has nothing to confirm.
        let confirmed = match read_log() {
            (Some(r), _) => !(r.state == PENDING && r.slot == my_slot()),
            _ => true,
        };
        Self {
            flash,
            update: None,
            confirmed,
        }
    }

    fn append(&mut self, r: Record) -> Result<(), &'static str> {
        let (_, at) = read_log();
        let at = at.ok_or("boot log full")?;
        let mut rec = [0u8; 16];
        rec[0..4].copy_from_slice(&META_MAGIC.to_le_bytes());
        rec[4..8].copy_from_slice(&r.seq.to_le_bytes());
        rec[8..12].copy_from_slice(&r.slot.to_le_bytes());
        let flags = r.state | (r.tries << 8) | (check(r.seq, r.slot) << 16);
        rec[12..16].copy_from_slice(&flags.to_le_bytes());
        self.flash
            .blocking_write(at - FLASH_BASE, &rec)
            .map_err(|_| "flash write failed")
    }

    /// Once the program runs and the network is up: it is the good one.
    pub fn confirm(&mut self) -> bool {
        if self.confirmed {
            return false;
        }
        self.confirmed = true;
        let seq = read_log().0.map_or(0, |r| r.seq);
        self.append(Record {
            seq: seq + 1,
            slot: my_slot(),
            state: CONFIRMED,
            tries: 0,
        })
        .is_ok()
    }

    /// Starts an update: erases the other slot. Returns its number.
    pub fn begin(&mut self, len: u32, crc: u32) -> Result<u32, &'static str> {
        if !(64..=SLOT_LEN).contains(&len) {
            return Err("length");
        }
        let target = 1 - my_slot();
        let from = SLOT[target as usize] - FLASH_BASE;
        self.update = None;
        self.flash
            .blocking_erase(from, from + SLOT_LEN)
            .map_err(|_| "erase failed")?;
        self.update = Some((target, len, crc, 0));
        Ok(target)
    }

    /// A chunk at `offset`; returns the next offset expected (a repeated or a skipped chunk
    /// changes nothing, the host resends from there).
    pub fn data(&mut self, offset: u32, bytes: &[u8]) -> Result<u32, &'static str> {
        let (target, len, _, next) = self.update.as_mut().ok_or("no update")?;
        if offset != *next || offset + bytes.len() as u32 > *len {
            return Ok(*next);
        }
        let at = SLOT[*target as usize] - FLASH_BASE + offset;
        let whole = bytes.len() / WRITE_SIZE * WRITE_SIZE;
        if whole > 0 {
            self.flash
                .blocking_write(at, &bytes[..whole])
                .map_err(|_| "flash write failed")?;
        }
        if whole < bytes.len() {
            let mut tail = [0xFFu8; WRITE_SIZE];
            tail[..bytes.len() - whole].copy_from_slice(&bytes[whole..]);
            self.flash
                .blocking_write(at + whole as u32, &tail)
                .map_err(|_| "flash write failed")?;
        }
        *next += bytes.len() as u32;
        Ok(*next)
    }

    /// Checks the program (length, CRC-32, a stack pointer in RAM and a reset vector inside the
    /// slot) and marks it pending: the next reset starts it.
    pub fn end(&mut self) -> Result<(), &'static str> {
        let (target, len, crc, next) = self.update.ok_or("no update")?;
        if next != len {
            return Err("incomplete");
        }
        let base = SLOT[target as usize];
        let image = unsafe { core::slice::from_raw_parts(base as *const u8, len as usize) };
        if crc32(image) != crc {
            return Err("crc mismatch");
        }
        let (sp, reset) = (word(base), word(base + 4));
        if !(0x2000_0000..=0x2008_0000).contains(&sp) || !(base..base + SLOT_LEN).contains(&reset) {
            return Err("not a program for that slot");
        }
        let seq = read_log().0.map_or(0, |r| r.seq);
        self.append(Record {
            seq: seq + 1,
            slot: target,
            state: PENDING,
            tries: 0,
        })?;
        self.update = None;
        Ok(())
    }
}
