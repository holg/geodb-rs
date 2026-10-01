//! The core of the geodb firmware, `no_std` and without `alloc`:
//!
//! - [`image`]: a flash-resident, zero-copy image of the cities. Geoids are
//!   32-bit Morton codes on 16-bit axes (0.0027° of latitude, about 305 m),
//!   sorted, so the sort order is the spatial index, as in `geodb-core`.
//! - [`geo`], [`query`]: the covering Z-order ranges of a circle (integer
//!   arithmetic) and radius / nearest queries with an f32 haversine, the
//!   method of the GPU kernels.
//! - [`render`]: a software globe into an RGB565 framebuffer, dots and text,
//!   the same code on the host (PNG previews) and on the board.
//! - [`ui`]: the whole 800 x 480 screen, drawn from the image.
//!
//! With the `std` feature, the `build` module writes the image from plain slices.

#![cfg_attr(not(any(feature = "std", test)), no_std)]

#[cfg(any(feature = "std", test))]
extern crate alloc;

#[cfg(any(feature = "std", test))]
pub mod build;
pub mod coast;
pub mod fmath;
pub mod geo;
pub mod image;
pub mod link;
pub mod query;
pub mod relief;
pub mod render;
pub mod ui;

pub use image::{FwImage, ImageError};
pub use query::{Answer, Hit};

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::{String, ToString};
    use alloc::vec::Vec;

    /// A deterministic scatter of cities, sorted by geoid.
    fn cities(n: usize) -> (Vec<u32>, Vec<u8>) {
        let mut state = 0x9e37_79b9_7f4a_7c15u64;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        let mut v: Vec<(u32, u8)> = (0..n)
            .map(|_| {
                // Denser at mid latitudes, like people.
                let lat = ((next() % 1400) as f32 / 10.0 - 70.0) * 0.9 + 12.0;
                let lon = (next() % 3600) as f32 / 10.0 - 180.0;
                (geo::from_deg(lat, lon), (next() % 7) as u8)
            })
            .collect();
        v.sort_unstable();
        v.into_iter().unzip()
    }

    fn image_of(n: usize) -> Vec<u8> {
        let (geoids, country_ids) = cities(n);
        let countries: Vec<(String, String)> = (0..7)
            .map(|i| {
                (
                    ["AA", "BB", "CC", "DD", "EE", "FF", "GG"][i].to_string(),
                    alloc::format!("Land {i}"),
                )
            })
            .collect();
        let names: Vec<(u32, String)> = (0..n as u32)
            .step_by(37)
            .map(|i| (i, alloc::format!("City {i}")))
            .collect();
        let coast = crate::coast::tests::square();
        build::build(&build::Source {
            geoids: &geoids,
            country_ids: &country_ids,
            countries: &countries,
            names: &names,
            coast: &coast,
            marble: &[],
        })
    }

    #[test]
    fn the_image_reads_back() {
        let bytes = image_of(5000);
        assert_eq!(bytes.len() % 4, 0);
        let img = FwImage::parse(&bytes).unwrap();
        assert_eq!((img.len(), img.countries(), img.named()), (5000, 7, 136));
        assert!((0..img.len() - 1).all(|i| img.geoid(i) <= img.geoid(i + 1)));
        assert_eq!(img.country_iso(3), "DD");
        assert_eq!(img.country_name(5), "Land 5");
        assert_eq!(img.name(37), Some("City 37"));
        assert_eq!(img.name(38), None);
        assert_eq!(img.named_at(2), (74, "City 74"));
        assert_eq!(img.coast().bytes(), &crate::coast::tests::square()[..]);
        // Damage is caught.
        assert_eq!(
            FwImage::parse(&bytes[..bytes.len() - 4]).err(),
            Some(ImageError::Layout)
        );
        let mut bad = bytes.clone();
        bad[0] = b'X';
        assert_eq!(FwImage::parse(&bad).err(), Some(ImageError::Magic));
        let mut bad = bytes.clone();
        bad[4] = 9;
        assert_eq!(FwImage::parse(&bad).err(), Some(ImageError::Version));
    }

    #[test]
    fn radius_equals_brute_force() {
        let bytes = image_of(20_000);
        let img = FwImage::parse(&bytes).unwrap();
        for &(lat, lon, r) in &[
            (48.14f32, 11.58f32, 30.0f32),
            (35.68, 139.69, 300.0),
            (-17.7, 179.99, 500.0),
            (-89.0, 0.0, 800.0),
            (0.0, 0.0, 2000.0),
            (10.0, -170.0, 20_000.0),
            (61.0, -150.0, 50.0),
            (48.14, 11.58, 0.05),
        ] {
            let mut got = Vec::new();
            let tested = img.radius(lat, lon, r, |h| got.push(h.index));
            got.sort_unstable();
            let limit = geo::hav_of_km(r);
            let want: Vec<u32> = (0..img.len())
                .filter(|&i| {
                    let (la, lo) = geo::to_deg(img.geoid(i));
                    geo::haversine(lat, lon, la, lo) <= limit
                })
                .map(|i| i as u32)
                .collect();
            assert_eq!(got, want, "({lat}, {lon}) {r} km");
            if r <= 500.0 {
                assert!(
                    tested < img.len() / 4,
                    "the index limits the work: {tested}"
                );
            }
        }
    }

    #[test]
    fn the_link_round_trips() {
        use link::Command;
        for c in [
            Command::Tap { x: 560, y: 390 },
            Command::Drag { dx: -7, dy: 3 },
            Command::Release,
            Command::Query {
                lat: 48.14,
                lon: 11.58,
                km: 300.0,
            },
        ] {
            assert_eq!(Command::decode(c.encode()), Some(c));
        }
        assert_eq!(
            Command::parse("!query 35.68 139.69 300"),
            Some(Command::Query {
                lat: 35.68,
                lon: 139.69,
                km: 300.0
            })
        );
        let view = render::View::new(-33.9, 151.2);
        let p = ui::encode_state(view, ui::Spin::new(), 31);
        assert_eq!(link::state_of(&link::state_words(&p)), p);
        let bytes = image_of(5000);
        let img = FwImage::parse(&bytes).unwrap();
        let mut t = 0;
        let a = img.answer(10.0, 20.0, 800.0, || {
            t += 7;
            t
        });
        let b = link::answer_of(&link::answer_words(&a));
        assert_eq!(
            (b.count, b.tested, b.radius_us, b.found),
            (a.count, a.tested, 7, a.found)
        );
        assert_eq!(b.nearest[..b.found], a.nearest[..a.found]);
    }

    #[test]
    fn nearest_equals_brute_force() {
        let bytes = image_of(20_000);
        let img = FwImage::parse(&bytes).unwrap();
        for &(lat, lon) in &[
            (48.14f32, 11.58f32),
            (35.68, 139.69),
            (-40.0, -140.0),
            (-89.9, 0.0),
            (89.9, 10.0),
            (0.0, -179.99),
            (25.0, 15.0),
        ] {
            for k in [1usize, 5, 10] {
                let mut out = alloc::vec![Hit { index: 0, km: 0.0 }; k];
                let n = img.nearest(lat, lon, &mut out);
                assert_eq!(n, k);
                let mut all: Vec<f32> = (0..img.len())
                    .map(|i| {
                        let (la, lo) = geo::to_deg(img.geoid(i));
                        geo::km(geo::haversine(lat, lon, la, lo))
                    })
                    .collect();
                all.sort_by(f32::total_cmp);
                for (h, w) in out.iter().zip(&all) {
                    assert!(
                        (h.km - w).abs() < 0.01,
                        "({lat},{lon}) k={k}: {} vs {w}",
                        h.km
                    );
                }
            }
        }
    }
}
