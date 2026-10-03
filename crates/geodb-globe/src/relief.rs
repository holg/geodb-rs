//! The elevation picture of the relief layers (hill shading and isohypses, drawn by the globe's shader).
//!
//! `elev.bin` is the picture of the firmware (`geodb-fw-core`, relief.rs): ETOPO 2022 reduced to 1024 x 512,
//! one byte a cell, north up, longitude -180..180. The byte codes the height non-linearly: 0..64 is the sea
//! floor from -6000 m to 0 in 93.75 m steps, 64..255 the land from 0 to 6500 m as
//! `v = 64 + 191 sqrt(h / 6500)`. The file may be gzipped (the single-file page packs it so).

use std::io::Read;

/// Width and height of the picture.
pub const WIDTH: u32 = 1024;
pub const HEIGHT: u32 = 512;

/// The height (m) a code stands for (the same as `geodb_fw_core::relief::elev_m`).
pub fn elev_m(v: u8) -> f32 {
    if v < 64 {
        (f32::from(v) - 64.0) * (6000.0 / 64.0)
    } else {
        let t = f32::from(v - 64) / 191.0;
        t * t * 6500.0
    }
}

/// The heights in metres (`WIDTH` x `HEIGHT`) from the bytes of `elev.bin` (plain or gzipped).
pub fn heights(file: &[u8]) -> Result<Vec<f32>, String> {
    let mut raw = Vec::new();
    if file.starts_with(&[0x1f, 0x8b]) {
        flate2::read::GzDecoder::new(file)
            .read_to_end(&mut raw)
            .map_err(|e| format!("elev.bin: {e}"))?;
    } else {
        raw = file.to_vec();
    }
    if raw.len() != (WIDTH * HEIGHT) as usize {
        return Err(format!(
            "elev.bin: {} bytes, {} expected",
            raw.len(),
            WIDTH * HEIGHT
        ));
    }
    Ok(raw.into_iter().map(elev_m).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_codes_stand_for_the_heights_of_the_firmware() {
        assert_eq!(elev_m(0), -6000.0);
        assert_eq!(elev_m(64), 0.0);
        assert!((elev_m(255) - 6500.0).abs() < 1.0);
        assert!(elev_m(100) > 0.0 && elev_m(100) < elev_m(101));
    }

    #[test]
    fn the_shipped_file_reads_plain_and_gzipped() {
        let gz = include_bytes!("../assets/mini/elev.bin");
        let h = heights(gz).expect("gzipped");
        assert_eq!(h.len(), (WIDTH * HEIGHT) as usize);
        // the Himalaya and the Mariana trench are in it
        let max = h.iter().cloned().fold(f32::MIN, f32::max);
        let min = h.iter().cloned().fold(f32::MAX, f32::min);
        assert!(max > 5000.0 && min < -4000.0, "{min} .. {max}");
        assert!(heights(&[0u8; 10]).is_err());
    }
}
