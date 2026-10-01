//! The debug-probe link: a block of words at a fixed RAM address on the board, read and written
//! by the host through the ST-LINK while the program runs (`geodb-board bridge --probe`).
//!
//! It sits at the start of the board's non-cacheable SRAM1 window (`ethbuf.x`), so what the CPU
//! writes is in memory (not only in its data cache) when the probe reads it. No atomics there
//! (LDREX/STREX fault on that memory): the board writes the state and the answers, the host the
//! command words, each side only its own.
//!
//! | word | written by | content |
//! |---|---|---|
//! | 0 | board | [`MAGIC`] once the program runs |
//! | 1 | board | sequence, +1 per new state |
//! | 2..7 | board | the state packet ([`crate::ui::encode_state`], 19 bytes) |
//! | 7 | host | command sequence, +1 per command |
//! | 8 | board | the command sequence done (= word 7: the host may send the next) |
//! | 9..13 | host | the command: kind, a, b, c ([`Command`]) |
//! | 13..38 | board | the answer to the last [`Command::Query`] |

use crate::query::{Answer, Hit, ANSWER_NEAREST};
use crate::ui::STATE_LEN;

/// The block's address on the board.
pub const ADDR: u32 = 0x2006_0000;
pub const MAGIC: u32 = u32::from_le_bytes(*b"GDLK");

pub const W_MAGIC: usize = 0;
pub const W_SEQ: usize = 1;
pub const W_STATE: usize = 2;
const STATE_WORDS: usize = STATE_LEN.div_ceil(4);
pub const W_HOST_SEQ: usize = W_STATE + STATE_WORDS;
pub const W_ACK: usize = W_HOST_SEQ + 1;
pub const W_COMMAND: usize = W_ACK + 1;
pub const W_ANSWER: usize = W_COMMAND + 4;
const ANSWER_WORDS: usize = 4 + 2 * ANSWER_NEAREST + 1;
/// Words in the block.
pub const WORDS: usize = W_ANSWER + ANSWER_WORDS;

/// What the host asks of the board: the touches of the mirror, or a timed query.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Command {
    Tap { x: i32, y: i32 },
    Drag { dx: i32, dy: i32 },
    Release,
    Query { lat: f32, lon: f32, km: f32 },
}

impl Command {
    pub fn encode(self) -> [u32; 4] {
        match self {
            Command::Tap { x, y } => [1, x as u32, y as u32, 0],
            Command::Drag { dx, dy } => [2, dx as u32, dy as u32, 0],
            Command::Release => [3, 0, 0, 0],
            Command::Query { lat, lon, km } => [4, lat.to_bits(), lon.to_bits(), km.to_bits()],
        }
    }

    pub fn decode(w: [u32; 4]) -> Option<Command> {
        Some(match w[0] {
            1 => Command::Tap {
                x: w[1] as i32,
                y: w[2] as i32,
            },
            2 => Command::Drag {
                dx: w[1] as i32,
                dy: w[2] as i32,
            },
            3 => Command::Release,
            4 => Command::Query {
                lat: f32::from_bits(w[1]),
                lon: f32::from_bits(w[2]),
                km: f32::from_bits(w[3]),
            },
            _ => return None,
        })
    }

    /// The text form of the network commands: `!tap X Y`, `!drag DX DY`, `!release`,
    /// `!query LAT LON KM`.
    pub fn parse(text: &str) -> Option<Command> {
        let mut words = text.split_whitespace();
        let kind = words.next()?;
        let mut num = || words.next().and_then(|w| w.parse::<f32>().ok());
        Some(match kind {
            "!tap" => Command::Tap {
                x: num()? as i32,
                y: num()? as i32,
            },
            "!drag" => Command::Drag {
                dx: num()? as i32,
                dy: num()? as i32,
            },
            "!release" => Command::Release,
            "!query" => Command::Query {
                lat: num()?,
                lon: num()?,
                km: num()?,
            },
            _ => return None,
        })
    }
}

/// The state packet as words (little-endian, zero padded).
pub fn state_words(packet: &[u8; STATE_LEN]) -> [u32; STATE_WORDS] {
    let mut w = [0u32; STATE_WORDS];
    for (i, b) in packet.iter().enumerate() {
        w[i / 4] |= u32::from(*b) << (8 * (i % 4));
    }
    w
}

pub fn state_of(w: &[u32]) -> [u8; STATE_LEN] {
    let mut p = [0u8; STATE_LEN];
    for (i, b) in p.iter_mut().enumerate() {
        *b = (w[i / 4] >> (8 * (i % 4))) as u8;
    }
    p
}

pub fn answer_words(a: &Answer) -> [u32; ANSWER_WORDS] {
    let mut w = [0u32; ANSWER_WORDS];
    w[..5].copy_from_slice(&[a.count, a.tested, a.radius_us, a.nearest_us, a.found as u32]);
    for (i, h) in a.nearest[..a.found].iter().enumerate() {
        w[5 + 2 * i] = h.index;
        w[6 + 2 * i] = h.km.to_bits();
    }
    w
}

pub fn answer_of(w: &[u32]) -> Answer {
    let found = (w[4] as usize).min(ANSWER_NEAREST);
    let mut nearest = [Hit { index: 0, km: 0.0 }; ANSWER_NEAREST];
    for (i, h) in nearest[..found].iter_mut().enumerate() {
        *h = Hit {
            index: w[5 + 2 * i],
            km: f32::from_bits(w[6 + 2 * i]),
        };
    }
    Answer {
        count: w[0],
        tested: w[1],
        radius_us: w[2],
        nearest_us: w[3],
        nearest,
        found,
    }
}
const _: () = assert!(
    WORDS * 4 <= 256,
    "the block fits the first 256 bytes of the window"
);
