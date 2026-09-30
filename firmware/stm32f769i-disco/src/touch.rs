//! FT6206 capacitive touch controller on the DISCO's panel: I2C4 (SCL PD12,
//! SDA PB7), 7-bit address 0x2A (newer panels) or 0x38. Polled, one finger.
//! ST's driver maps the controller's portrait coordinates to the 800×480
//! landscape screen with "swap XY, then mirror Y"; the same is done here.

use defmt::info;
use embassy_stm32::i2c::{I2c, Master};
use embassy_stm32::mode::Blocking;

const REG_TD_STATUS: u8 = 0x02;
const REG_CHIP_ID: u8 = 0xA8;
const ID_FT6206: u8 = 0x11;
const ID_FT6X36: u8 = 0xCD;

pub struct Touch {
    i2c: I2c<'static, Blocking, Master>,
    addr: u8,
    pub chip: u8,
}

impl Touch {
    /// Probe both addresses; `None` when no controller answers.
    pub fn new(mut i2c: I2c<'static, Blocking, Master>) -> Option<Self> {
        for addr in [0x2Au8, 0x38] {
            let mut id = [0u8; 1];
            if i2c
                .blocking_write_read(addr, &[REG_CHIP_ID], &mut id)
                .is_ok()
                && (id[0] == ID_FT6206 || id[0] == ID_FT6X36)
            {
                info!("touch: FT6x06 id {:#04x} at {:#04x}", id[0], addr);
                return Some(Self {
                    i2c,
                    addr,
                    chip: id[0],
                });
            }
        }
        info!("touch: no FT6x06 found");
        None
    }

    /// Current finger position in screen pixels (landscape), or `None`.
    pub fn read(&mut self) -> Option<(u16, u16)> {
        let mut b = [0u8; 5]; // TD_STATUS, P1_XH, P1_XL, P1_YH, P1_YL
        self.i2c
            .blocking_write_read(self.addr, &[REG_TD_STATUS], &mut b)
            .ok()?;
        let touches = b[0] & 0x0F;
        if touches == 0 || touches > 2 {
            return None;
        }
        let rx = (((b[1] & 0x0F) as u16) << 8) | b[2] as u16;
        let ry = (((b[3] & 0x0F) as u16) << 8) | b[4] as u16;
        // ST BSP for the landscape screen: swap XY, then mirror Y; the panel is
        // rotated 180° on top (MADCTL 0xA0), so mirror both again.
        let x = ry.min(799);
        let y = (479u16).saturating_sub(rx.min(479));
        Some((799 - x, 479 - y))
    }
}
