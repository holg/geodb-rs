//! geodb-globe — an interactive 3D globe demo for `geodb-core`, rendered with
//! wgpu (WebGPU with a WebGL2 fallback) and compiled to WebAssembly.
//!
//! The earth texture is baked at start-up from the database itself (see
//! [`texture`]). Clicking the globe, zooming or searching queries the embedded
//! database for the cities around the centre of view.
//!
//! Run the demo with `trunk serve` from this crate's directory.

pub mod camera;
pub mod coast;
pub mod compare;
pub mod data;
pub mod geo;
pub mod geoid;
pub mod mesh;
pub mod mini;
pub mod places;
pub mod render;
pub mod single_file;
pub mod source;
pub mod texture;
pub mod tiles;
pub mod view;

#[cfg(all(target_arch = "wasm32", not(feature = "mini")))]
mod app;
#[cfg(all(target_arch = "wasm32", feature = "mini"))]
mod mini_app;

#[cfg(feature = "native")]
pub mod api_bench;
#[cfg(feature = "native")]
pub mod globe_view;
pub mod gpu_query;
