//! Writes the image from plain data (host side).

use crate::geo;
use crate::image::{HEADER_LEN, MAGIC, VERSION};
use alloc::string::String;
use alloc::vec::Vec;

/// What goes into an image.
pub struct Source<'a> {
    /// One geoid per city, sorted ascending.
    pub geoids: &'a [u32],
    /// One country index per city.
    pub country_ids: &'a [u8],
    /// (ISO2, ASCII name) per country.
    pub countries: &'a [(String, String)],
    /// (city index, ASCII name), ascending by city.
    pub names: &'a [(u32, String)],
    /// The packed coastline rings (`coast::Coast` payload).
    pub coast: &'a [u8],
}

fn align4(v: &mut Vec<u8>) {
    while !v.len().is_multiple_of(4) {
        v.push(0);
    }
}

/// The image bytes. Panics on inconsistent input (unsorted geoids, ...).
pub fn build(src: &Source<'_>) -> Vec<u8> {
    assert!(src.geoids.windows(2).all(|w| w[0] <= w[1]), "geoids sorted");
    assert_eq!(src.geoids.len(), src.country_ids.len());
    assert!(
        src.names.windows(2).all(|w| w[0].0 < w[1].0),
        "names ascending"
    );
    let mut out = alloc::vec![0u8; HEADER_LEN];
    let put = |out: &mut Vec<u8>, bytes: &[u8]| -> usize {
        align4(out);
        let at = out.len();
        out.extend_from_slice(bytes);
        at
    };
    let geoids: Vec<u8> = src.geoids.iter().flat_map(|g| g.to_le_bytes()).collect();
    let off_geoids = put(&mut out, &geoids);
    let off_country = put(&mut out, src.country_ids);
    let named: Vec<u8> = src.names.iter().flat_map(|n| n.0.to_le_bytes()).collect();
    let off_named = put(&mut out, &named);
    let mut blob = Vec::new();
    let mut offsets: Vec<u8> = Vec::new();
    for (_, name) in src.names {
        assert!(name.is_ascii() && !name.contains('\0'), "ascii name");
        offsets.extend((blob.len() as u32).to_le_bytes());
        blob.extend_from_slice(name.as_bytes());
        blob.push(0);
    }
    offsets.extend((blob.len() as u32).to_le_bytes());
    let off_name_off = put(&mut out, &offsets);
    let off_names = put(&mut out, &blob);
    let names_len = blob.len();
    let mut table = Vec::new();
    let mut cnames = Vec::new();
    for (iso, name) in src.countries {
        assert!(iso.len() == 2 && name.is_ascii());
        table.extend_from_slice(iso.as_bytes());
        table.extend((cnames.len() as u16).to_le_bytes());
        cnames.extend_from_slice(name.as_bytes());
        cnames.push(0);
    }
    assert!(cnames.len() <= usize::from(u16::MAX));
    let off_countries = put(&mut out, &table);
    let off_country_names = put(&mut out, &cnames);
    let off_coast = put(&mut out, src.coast);
    align4(&mut out);
    let total = out.len();
    let w32 = |out: &mut Vec<u8>, at: usize, v: usize| {
        out[at..at + 4].copy_from_slice(&(v as u32).to_le_bytes())
    };
    out[..4].copy_from_slice(&MAGIC);
    out[4..6].copy_from_slice(&VERSION.to_le_bytes());
    w32(&mut out, 8, src.geoids.len());
    w32(&mut out, 12, src.countries.len());
    w32(&mut out, 16, src.names.len());
    w32(&mut out, 20, src.coast.len());
    for (at, v) in [
        (24, off_geoids),
        (28, off_country),
        (32, off_named),
        (36, off_name_off),
        (40, off_names),
        (44, names_len),
        (48, off_countries),
        (52, off_country_names),
        (56, off_coast),
        (60, total),
    ] {
        w32(&mut out, at, v);
    }
    out
}

/// A geoid for (lat, lon) degrees, for callers that build the image from
/// coordinates.
pub fn geoid_of(lat: f32, lon: f32) -> u32 {
    geo::from_deg(lat, lon)
}
