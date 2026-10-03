//! The globe as a scopekit [`GpuView`]: the same [`Renderer`] as the web
//! demo, drawn into whatever region scopekit gives it (a terminal image or
//! part of a window surface).

use crate::camera::OrbitCamera;
use crate::render::{Marker, Renderer};
use crate::texture::EarthTexture;
use crate::view::{self, Query};
use scopekit::{wgpu, Gpu, GpuView, Target};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

pub struct GlobeView {
    earth: Arc<EarthTexture>,
    renderer: Option<Renderer>,
    camera: OrbitCamera,
    query: Option<Query>,
    markers: Vec<Marker>,
    markers_dirty: bool,
    changed: bool,
    /// Elevation (metres) for the relief layers, and which of them show: hill shading, isohypses.
    relief: Option<(Arc<Vec<f32>>, bool, bool)>,
    /// A detail patch (RGBA8 sRGB, `size` x `size`, with its `detail` uniform) for tests and tools.
    patch: Option<(Arc<Vec<u8>>, u32, [f32; 4])>,
}

impl GlobeView {
    /// `earth` is baked once and shared: a mode switch prepares the view
    /// again on another device without baking it again.
    pub fn new(earth: Arc<EarthTexture>, camera: OrbitCamera) -> GlobeView {
        GlobeView {
            earth,
            renderer: None,
            camera,
            query: None,
            markers: Vec::new(),
            markers_dirty: true,
            changed: true,
            relief: None,
            patch: None,
        }
    }

    /// Puts a detail patch on the globe: `rgba` is `size` x `size` pixels, `uniform` places it
    /// ([`crate::tiles::Window::uniform`]).
    pub fn set_patch(&mut self, rgba: Arc<Vec<u8>>, size: u32, uniform: [f32; 4]) {
        self.patch = Some((rgba, size, uniform));
        if let (Some(r), Some(p)) = (self.renderer.as_mut(), self.patch.as_ref()) {
            Self::apply_patch(r, p);
        }
        self.changed = true;
    }

    fn apply_patch(r: &mut Renderer, patch: &(Arc<Vec<u8>>, u32, [f32; 4])) {
        let (rgba, size, uniform) = patch;
        let view = r.patch_view(rgba, *size);
        r.set_patch(Some((view, *uniform)));
    }

    /// Relief layers on the globe: `heights` is the elevation picture ([`crate::relief::heights`]).
    pub fn set_relief(&mut self, heights: Arc<Vec<f32>>, shading: bool, isohypses: bool) {
        self.relief = Some((heights, shading, isohypses));
        if let Some(r) = self.renderer.as_mut() {
            Self::apply_relief(r, &self.relief);
        }
        self.changed = true;
    }

    fn apply_relief(r: &mut Renderer, relief: &Option<(Arc<Vec<f32>>, bool, bool)>) {
        match relief {
            Some((h, shading, iso)) => {
                r.set_relief(Some((
                    h.as_slice(),
                    crate::relief::WIDTH,
                    crate::relief::HEIGHT,
                )));
                r.show_relief(*shading, *iso);
            }
            None => r.set_relief(None),
        }
    }

    pub fn set_camera(&mut self, camera: &OrbitCamera) {
        let c = &self.camera;
        if (c.lat, c.lon, c.dist) != (camera.lat, camera.lon, camera.dist) {
            self.camera = camera.clone();
            self.changed = true;
        }
    }

    pub fn set_query(&mut self, query: Option<Query>) {
        if self.query != query {
            self.query = query;
            self.changed = true;
        }
    }

    pub fn set_markers(&mut self, markers: Vec<Marker>) {
        self.markers = markers;
        self.markers_dirty = true;
        self.changed = true;
    }
}

fn unix_ms() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64() * 1000.0)
        .unwrap_or(0.0)
}

impl GpuView for GlobeView {
    fn prepare(&mut self, gpu: &Gpu, format: wgpu::TextureFormat) {
        let earth = &self.earth;
        self.renderer = Some(Renderer::new(
            &gpu.device,
            &gpu.queue,
            format,
            (1, 1),
            |_| (**earth).clone(),
        ));
        if let Some(r) = self.renderer.as_mut() {
            Self::apply_relief(r, &self.relief);
            if let Some(p) = self.patch.as_ref() {
                Self::apply_patch(r, p);
            }
        }
        self.markers_dirty = true;
        self.changed = true;
    }

    fn render(&mut self, _gpu: &Gpu, encoder: &mut wgpu::CommandEncoder, target: &Target<'_>) {
        let Some(renderer) = self.renderer.as_mut() else {
            return;
        };
        if self.markers_dirty {
            renderer.set_markers(&self.markers);
            self.markers_dirty = false;
        }
        let r = target.region;
        let mut camera = self.camera.clone();
        camera.aspect = r.width.max(1) as f32 / r.height.max(1) as f32;
        let scene = view::scene(self.query, unix_ms());
        renderer.render_region(
            encoder,
            target.view,
            (r.x, r.y, r.width, r.height),
            &camera,
            &scene,
        );
        self.changed = false;
    }

    fn changed(&self) -> bool {
        self.changed
    }
}
