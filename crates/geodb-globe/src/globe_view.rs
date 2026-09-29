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
