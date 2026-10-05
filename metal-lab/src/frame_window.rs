use super::Renderer;
use anyhow::{Context, Result};
use core_graphics_types::geometry::CGSize;
use metal::*;
use objc::{msg_send, rc::autoreleasepool, runtime::Object, sel, sel_impl};
use std::time::Instant;
use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop},
    raw_window_handle::{HasWindowHandle, RawWindowHandle},
    window::{Window, WindowId},
};

const PRESENT: &str = r#"
#include <metal_stdlib>
using namespace metal;
struct V { float4 p [[position]]; float2 uv; };
vertex V vertex_main(uint id [[vertex_id]]) {
    float2 p = float2((id << 1) & 2, id & 2);
    return {float4(p * 2. - 1., 0., 1.), p};
}
fragment float4 fragment_main(V v [[stage_in]], texture2d<float> scene [[texture(0)]]) {
    constexpr sampler s(coord::normalized, filter::linear, address::clamp_to_edge);
    return scene.sample(s, v.uv); // original GL rows are bottom-up
}
"#;
struct App {
    renderer: Renderer,
    window: Option<Window>,
    layer: MetalLayer,
    pipeline: RenderPipelineState,
    count: usize,
    since: Instant,
    in_flight: Option<CommandBuffer>,
}
impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let window = event_loop
            .create_window(
                Window::default_attributes()
                    .with_title("Rhine Metal — 原版着色器验证")
                    .with_inner_size(LogicalSize::new(1280., 788.)),
            )
            .expect("native window");
        if let RawWindowHandle::AppKit(handle) = window.window_handle().unwrap().as_raw() {
            unsafe {
                let view = handle.ns_view.as_ptr() as *mut Object;
                let _: () = msg_send![view, setWantsLayer: true];
                let _: () = msg_send![view, setLayer: self.layer.as_ref()];
            }
        }
        let size = window.inner_size();
        self.layer
            .set_drawable_size(CGSize::new(size.width as f64, size.height as f64));
        window.request_redraw();
        self.window = Some(window);
    }
    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                self.layer
                    .set_drawable_size(CGSize::new(size.width as f64, size.height as f64));
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
            }
            WindowEvent::RedrawRequested => autoreleasepool(|| {
                // One in flight bounds queueing latency; no CPU texture readback.
                if let Some(previous) = self.in_flight.take() {
                    previous.wait_until_completed();
                }
                let Some(drawable) = self.layer.next_drawable() else {
                    return;
                };
                let cb = self.renderer.queue.new_command_buffer();
                self.renderer.encode(cb);
                let pass = RenderPassDescriptor::new();
                let c = pass.color_attachments().object_at(0).unwrap();
                c.set_texture(Some(drawable.texture()));
                c.set_load_action(MTLLoadAction::DontCare);
                c.set_store_action(MTLStoreAction::Store);
                let e = cb.new_render_command_encoder(pass);
                e.set_render_pipeline_state(&self.pipeline);
                e.set_fragment_texture(0, Some(&self.renderer.screen));
                e.draw_primitives(MTLPrimitiveType::Triangle, 0, 3);
                e.end_encoding();
                cb.present_drawable(drawable);
                cb.commit();
                self.in_flight = Some(cb.to_owned());
                self.count += 1;
                if self.since.elapsed().as_secs_f64() > 1. {
                    if let Some(w) = &self.window {
                        w.set_title(&format!(
                            "Rhine Metal — {:.0} FPS — 原画面 {}×{} — 静态重绘验证",
                            self.count as f64 / self.since.elapsed().as_secs_f64(),
                            self.renderer.width,
                            self.renderer.height
                        ));
                    }
                    self.count = 0;
                    self.since = Instant::now();
                }
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
            }),
            _ => (),
        }
    }
}
pub fn run(renderer: Renderer) -> Result<()> {
    let event_loop = EventLoop::new()?;
    let layer = MetalLayer::new();
    layer.set_device(&renderer.device);
    layer.set_pixel_format(MTLPixelFormat::BGRA8Unorm);
    layer.set_presents_with_transaction(false);
    layer.set_display_sync_enabled(true);
    layer.set_maximum_drawable_count(2);
    let lib = renderer
        .device
        .new_library_with_source(PRESENT, &CompileOptions::new())
        .map_err(anyhow::Error::msg)?;
    let pd = RenderPipelineDescriptor::new();
    pd.set_vertex_function(Some(
        lib.get_function("vertex_main", None)
            .map_err(anyhow::Error::msg)?
            .as_ref(),
    ));
    pd.set_fragment_function(Some(
        lib.get_function("fragment_main", None)
            .map_err(anyhow::Error::msg)?
            .as_ref(),
    ));
    pd.color_attachments()
        .object_at(0)
        .context("color")?
        .set_pixel_format(MTLPixelFormat::BGRA8Unorm);
    let pipeline = renderer
        .device
        .new_render_pipeline_state(&pd)
        .map_err(anyhow::Error::msg)?;
    event_loop.run_app(&mut App {
        renderer,
        window: None,
        layer,
        pipeline,
        count: 0,
        since: Instant::now(),
        in_flight: None,
    })?;
    Ok(())
}
