//! The board through its ST-LINK (feature `probe`): probe-rs attaches to the running program
//! without halting or resetting it, and reads and writes the link block
//! (`geodb_fw_core::link`, at 0x20060000) while it runs.

use geodb_fw_core::link::{self, Command, WORDS};
use geodb_fw_core::Answer;
use probe_rs::probe::list::Lister;
use probe_rs::{MemoryInterface, Permissions, Session};
use std::time::{Duration, Instant};

/// The chip of the STM32F769I-DISCO, as the firmware's runner names it.
const CHIP: &str = "STM32F769NIHx";
/// The DISCO's ST-LINK V2-1 is happier at 1 MHz SWD (see the firmware's .cargo/config.toml).
const SPEED_KHZ: u32 = 1000;

pub struct Probe {
    session: Session,
    pub name: String,
    host_seq: u32,
}

impl Probe {
    pub fn attach() -> Result<Probe, String> {
        let lister = Lister::new();
        let info = lister
            .list_all()
            .into_iter()
            .next()
            .ok_or("no debug probe found (is the ST-LINK plugged in?)")?;
        let name = info.identifier.clone();
        let mut probe = info.open().map_err(|e| format!("{name}: {e}"))?;
        probe
            .set_speed(SPEED_KHZ)
            .map_err(|e| format!("{name}: speed: {e}"))?;
        let session = probe
            .attach(CHIP, Permissions::default())
            .map_err(|e| format!("{name}: attach to {CHIP}: {e}"))?;
        let mut p = Probe {
            session,
            name,
            host_seq: 0,
        };
        let block = p.read()?;
        if block[link::W_MAGIC] != link::MAGIC {
            return Err(format!(
                "no link block at {:#x} (the firmware is older than the probe link, or not running)",
                link::ADDR
            ));
        }
        // carry on from the board's count (a bridge may have run before)
        p.host_seq = block[link::W_HOST_SEQ];
        Ok(p)
    }

    /// The whole block, read while the program runs.
    pub fn read(&mut self) -> Result<[u32; WORDS], String> {
        let mut w = [0u32; WORDS];
        let mut core = self.session.core(0).map_err(|e| e.to_string())?;
        core.read_32(u64::from(link::ADDR), &mut w)
            .map_err(|e| format!("read: {e}"))?;
        Ok(w)
    }

    /// Whether the board is done with the last command.
    pub fn idle(block: &[u32; WORDS]) -> bool {
        block[link::W_ACK] == block[link::W_HOST_SEQ]
    }

    /// Hands the board a command (call when [`idle`](Self::idle)).
    pub fn send(&mut self, cmd: Command) -> Result<(), String> {
        let mut core = self.session.core(0).map_err(|e| e.to_string())?;
        let at = |i: usize| u64::from(link::ADDR) + 4 * i as u64;
        core.write_32(at(link::W_COMMAND), &cmd.encode())
            .map_err(|e| format!("write: {e}"))?;
        self.host_seq = self.host_seq.wrapping_add(1);
        // the sequence last: the board takes the command once it moves
        core.write_word_32(at(link::W_HOST_SEQ), self.host_seq)
            .map_err(|e| format!("write: {e}"))?;
        Ok(())
    }

    /// A query, waited for: the board's answer.
    pub fn query(&mut self, lat: f32, lon: f32, km: f32) -> Result<Answer, String> {
        let deadline = Instant::now() + Duration::from_secs(3);
        while !Self::idle(&self.read()?) {
            if Instant::now() > deadline {
                return Err("the board does not take commands".into());
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        self.send(Command::Query { lat, lon, km })?;
        loop {
            let block = self.read()?;
            if Self::idle(&block) {
                return Ok(link::answer_of(&block[link::W_ANSWER..]));
            }
            if Instant::now() > deadline {
                return Err("no answer from the board".into());
            }
            std::thread::sleep(Duration::from_millis(2));
        }
    }
}
