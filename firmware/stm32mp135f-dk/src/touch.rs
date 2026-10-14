//! The touch panel (Goodix GT911) through evdev, read without waiting. The driver reports in screen pixels,
//! one finger is enough: BTN_TOUCH says down or up, ABS_X / ABS_Y where, SYN_REPORT closes a sample.

use std::fs::{File, OpenOptions};
use std::io::{self, Read};
use std::os::unix::fs::OpenOptionsExt;

const O_NONBLOCK: i32 = 0o4000;
const EV_SYN: u16 = 0;
const EV_KEY: u16 = 1;
const EV_ABS: u16 = 3;
const SYN_REPORT: u16 = 0;
const BTN_TOUCH: u16 = 0x14a;
const ABS_X: u16 = 0;
const ABS_Y: u16 = 1;

/// `struct input_event`: the time (two longs), type, code, value.
const EVENT: usize = 2 * core::mem::size_of::<usize>() + 8;

pub struct Touch {
    dev: File,
    down: bool,
    x: i32,
    y: i32,
}

impl Touch {
    pub fn open(path: &str) -> io::Result<Touch> {
        let dev = OpenOptions::new()
            .read(true)
            .custom_flags(O_NONBLOCK)
            .open(path)?;
        Ok(Touch {
            dev,
            down: false,
            x: 0,
            y: 0,
        })
    }

    /// The samples that came since the last call, in order (`None`: no finger). A tap shorter than a frame
    /// still shows as a touch and a release.
    pub fn poll(&mut self) -> Vec<Option<(i32, i32)>> {
        let mut out = Vec::new();
        let mut buf = [0u8; EVENT * 64];
        loop {
            let n = match self.dev.read(&mut buf) {
                Ok(n) if n >= EVENT => n,
                _ => break, // (would block, or an error: no more now)
            };
            for e in buf[..n - n % EVENT].chunks_exact(EVENT) {
                let at = EVENT - 8;
                let kind = u16::from_ne_bytes([e[at], e[at + 1]]);
                let code = u16::from_ne_bytes([e[at + 2], e[at + 3]]);
                let value = i32::from_ne_bytes([e[at + 4], e[at + 5], e[at + 6], e[at + 7]]);
                match (kind, code) {
                    (EV_KEY, BTN_TOUCH) => self.down = value != 0,
                    (EV_ABS, ABS_X) => self.x = value,
                    (EV_ABS, ABS_Y) => self.y = value,
                    (EV_SYN, SYN_REPORT) => out.push(self.now()),
                    _ => {}
                }
            }
        }
        out
    }

    /// Where the finger is, when it is down.
    pub fn now(&self) -> Option<(i32, i32)> {
        self.down.then_some((self.x, self.y))
    }
}
