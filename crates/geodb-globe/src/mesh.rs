//! UV sphere with a duplicated seam column so texture coordinates stay
//! continuous (and mipmapping works) across the antimeridian.

use crate::geo;
use bytemuck::{Pod, Zeroable};

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct Vertex {
    pub pos: [f32; 3],
    pub uv: [f32; 2],
}

pub fn uv_sphere(lon_segments: u32, lat_segments: u32) -> (Vec<Vertex>, Vec<u32>) {
    let mut vertices = Vec::with_capacity(((lon_segments + 1) * (lat_segments + 1)) as usize);
    for j in 0..=lat_segments {
        let v = j as f32 / lat_segments as f32;
        let lat = 90.0 - v as f64 * 180.0;
        for i in 0..=lon_segments {
            let u = i as f32 / lon_segments as f32;
            let lon = u as f64 * 360.0 - 180.0;
            vertices.push(Vertex {
                pos: geo::to_vec(lat, lon).to_array(),
                uv: [u, v],
            });
        }
    }
    let row = lon_segments + 1;
    let mut indices = Vec::with_capacity((lon_segments * lat_segments * 6) as usize);
    for j in 0..lat_segments {
        for i in 0..lon_segments {
            let a = j * row + i;
            let b = a + row;
            // Counter-clockwise when viewed from outside.
            indices.extend_from_slice(&[a, b, a + 1, a + 1, b, b + 1]);
        }
    }
    (vertices, indices)
}
