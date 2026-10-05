//! Development native renderer behind the unchanged WKWebView controls.
//! The protocol is local and explicit; captures are never sent to a service.
use anyhow::{bail, Context, Result};
use core_graphics_types::geometry::{CGRect, CGSize};
use metal::foreign_types::ForeignType;
use metal::*;
use objc::{class, msg_send, runtime::Object, sel, sel_impl};
use rhine_metal_lab::frame_renderer::Renderer;
use serde_json::{json, Value};
use std::{
    path::PathBuf,
    sync::{Mutex, OnceLock},
    sync::atomic::{AtomicBool, Ordering},
    time::Instant,
};
use tauri::Manager;

const PRESENT: &str = r#"
#include <metal_stdlib>
using namespace metal;
struct V { float4 p [[position]]; float2 uv; };
vertex V present_vertex(uint id [[vertex_id]]) {float2 p=float2((id<<1)&2,id&2);return {float4(p*2.-1.,0.,1.),p};}
fragment float4 present_fragment(V v [[stage_in]],texture2d<float> scene [[texture(0)]]) {
 constexpr sampler s(coord::normalized,filter::linear,address::clamp_to_edge);return scene.sample(s,v.uv);
}
"#;
struct Native {
    renderer: Renderer,
    layer: MetalLayer,
    pipeline: RenderPipelineState,
    view: usize,
    frames: u64,
    capture_dir: PathBuf,
}
static NATIVE: OnceLock<Mutex<Option<Native>>> = OnceLock::new();
static PROFILE_SLOW: AtomicBool = AtomicBool::new(false);
#[tauri::command]
pub fn metal_profile_slow() { PROFILE_SLOW.store(true, Ordering::Relaxed); }
fn state() -> &'static Mutex<Option<Native>> {
    NATIVE.get_or_init(|| Mutex::new(None))
}

struct Prepared {
    renderer: Renderer,
    pipeline: RenderPipelineState,
    capture_dir: PathBuf,
}
fn prepare(dir: PathBuf, compiler_dir: PathBuf) -> Result<Prepared> {
    let shaders = dir.join("metal-shaders");
    if !shaders.join("programs.json").exists() {
        rhine_metal_lab::shader_translate::translate(&dir, &shaders, &compiler_dir)?;
    }
    std::env::set_var("RHINE_METAL_FAST_MATH", "1");
    let renderer = Renderer::load(&dir, &shaders)?;
    // Populate persistent targets (including static shadow maps) before live
    // frames start omitting their unchanged passes.
    let warmup = renderer.queue.new_command_buffer();
    renderer.encode(warmup);
    warmup.commit();
    warmup.wait_until_completed();
    if warmup.status() != MTLCommandBufferStatus::Completed {
        bail!("Native scene warmup failed");
    }
    let library = renderer
        .device
        .new_library_with_source(PRESENT, &CompileOptions::new())
        .map_err(anyhow::Error::msg)?;
    let p = RenderPipelineDescriptor::new();
    p.set_vertex_function(Some(
        library
            .get_function("present_vertex", None)
            .map_err(anyhow::Error::msg)?
            .as_ref(),
    ));
    p.set_fragment_function(Some(
        library
            .get_function("present_fragment", None)
            .map_err(anyhow::Error::msg)?
            .as_ref(),
    ));
    p.color_attachments()
        .object_at(0)
        .context("color attachment")?
        .set_pixel_format(MTLPixelFormat::BGRA8Unorm);
    let pipeline = renderer
        .device
        .new_render_pipeline_state(&p)
        .map_err(anyhow::Error::msg)?;
    Ok(Prepared {
        renderer,
        pipeline,
        capture_dir: dir,
    })
}
#[tauri::command]
pub async fn metal_prepare(
    window: tauri::WebviewWindow,
    id: String,
) -> std::result::Result<Value, String> {
    let result = async {
        if id.is_empty()
            || id.len() > 100
            || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        {
            bail!("Invalid capture id");
        }
        let dir = std::env::var_os("MUSIC_NATIVE_DATA_DIR").map(PathBuf::from).unwrap_or(window.app_handle().path().app_data_dir()?)
            .join("metal-captures")
            .join(id);
        let compiler_dir=window.app_handle().path().resource_dir()?.join("metal-tools");
        let prepared = tauri::async_runtime::spawn_blocking(move || prepare(dir,compiler_dir)).await??;
        let device=prepared.renderer.device.clone();
        let diagnostic_dir=prepared.capture_dir.clone();
        window.set_background_color(Some(tauri::window::Color(0, 0, 0, 0)))?;
        let (tx, rx) = std::sync::mpsc::channel();
        window.with_webview(move |webview| unsafe {
    let layer = MetalLayer::new();
    layer.set_device(&device);
    layer.set_pixel_format(MTLPixelFormat::BGRA8Unorm);
    layer.set_presents_with_transaction(false);
    layer.set_display_sync_enabled(true);
    layer.set_maximum_drawable_count(2);
    layer.set_framebuffer_only(true);

            let view = webview.inner() as *mut Object;
            let web_bounds: CGRect = msg_send![view, frame];
            let parent: *mut Object = msg_send![view, superview];
            let ns_window = webview.ns_window() as *mut Object;
            let scale: f64 = msg_send![ns_window, backingScaleFactor];
            let _:()=msg_send![ns_window,setOpaque:false];
            let bounds:CGRect=msg_send![ns_window,contentLayoutRect];
            let was_backed:bool=msg_send![parent,wantsLayer];
            let _:()=msg_send![parent,setWantsLayer:true];
            let metal_view: *mut Object = msg_send![class!(NSView), alloc];
            let metal_view: *mut Object = msg_send![metal_view,initWithFrame:bounds];
            let _: () = msg_send![metal_view,setLayer:layer.as_ref()];
            let _: () = msg_send![metal_view,setWantsLayer:true];
            let _: () = msg_send![metal_view,setAutoresizingMask:18u64];
            let _: () = msg_send![parent,addSubview:metal_view positioned:-1i64 relativeTo:view];
            layer.set_contents_scale(scale);
            layer.set_drawable_size(CGSize::new(
                bounds.size.width * scale,
                bounds.size.height * scale,
            ));
            let layer_bounds: CGRect = msg_send![metal_view, bounds];
            let _: () = msg_send![layer.as_ref(),setFrame:layer_bounds];
            let actual_layer:*mut Object=msg_send![metal_view,layer];
            let info=json!({"parentClass":(*parent).class().name(),"webviewClass":(*view).class().name(),"parentWasLayerBacked":was_backed,"webviewFrame":format!("{:?}",web_bounds),"contentLayout":format!("{:?}",bounds),"layerFrame":format!("{:?}",layer_bounds),"correctLayer":actual_layer==layer.as_ptr() as *mut Object});
            let _=std::fs::write(diagnostic_dir.join("native-view.json"),info.to_string());
            let _: () = msg_send![metal_view, release];
            let _ = tx.send((metal_view as usize,layer));
        })?;
        let(view,layer) = tauri::async_runtime::spawn_blocking(move || rx.recv()).await??;
        let native=Native{renderer:prepared.renderer,pipeline:prepared.pipeline,capture_dir:prepared.capture_dir,view,layer,frames:0};
        let width = native.renderer.width;
        let height = native.renderer.height;
        *state().lock().unwrap() = Some(native);
        window.set_background_color(Some(tauri::window::Color(0, 0, 0, 0)))?;
        Ok::<_, anyhow::Error>(
            json!({"width":width,"height":height,"backend":"Metal","fastMath":true}),
        )
    }
    .await;
    result.map_err(|e| format!("{e:#}"))
}
#[tauri::command]
pub async fn metal_frame(frame: Value) -> std::result::Result<Value, String> {
    tauri::async_runtime::spawn_blocking(move||->Result<Value>{objc::rc::autoreleasepool(||{
        let start=Instant::now();let mut guard=state().lock().unwrap();let native=guard.as_mut().context("Metal inactive")?;
        native.renderer.update_frame(&frame)?;
        let Some(drawable)=native.layer.next_drawable()else{return Ok(json!({"paused":true}));};
        let cb=native.renderer.queue.new_command_buffer();native.renderer.encode(cb);
        let p=RenderPassDescriptor::new();let c=p.color_attachments().object_at(0).unwrap();c.set_texture(Some(drawable.texture()));c.set_load_action(MTLLoadAction::DontCare);c.set_store_action(MTLStoreAction::Store);
        let e=cb.new_render_command_encoder(p);e.set_render_pipeline_state(&native.pipeline);e.set_fragment_texture(0,Some(&native.renderer.screen));e.draw_primitives(MTLPrimitiveType::Triangle,0,3);e.end_encoding();
        let cpu=start.elapsed().as_secs_f64()*1000.;cb.present_drawable(drawable);cb.commit();cb.wait_until_completed();
        if cb.status()!=MTLCommandBufferStatus::Completed{bail!("Metal command failed {:?}",cb.status());}
        let(a,b):(f64,f64)=unsafe{(msg_send![cb,GPUStartTime],msg_send![cb,GPUEndTime])};native.frames+=1;native.renderer.release_resources(&frame)?;
        let profiled=(b-a)*1000. > 25. && PROFILE_SLOW.swap(false,Ordering::Relaxed);
        if profiled {
            let profile=native.renderer.profile_events()?;
            let report=json!({"gpuMs":(b-a)*1000.,"cpuMs":cpu,"frame":native.frames,"commands":frame["commands"],"events":profile,
                "note":"Diagnostic replay splits existing encoder boundaries into command buffers; do not use the interrupted live run for end-to-end FPS."});
            std::fs::write(native.capture_dir.join("slow-frame-profile.json"),serde_json::to_vec_pretty(&report)?)?;
        }
        Ok(json!({"profiled":profiled,"drawableWidth":drawable.texture().width(),"drawableHeight":drawable.texture().height(),"frames":native.frames,"gpuMs":(b-a)*1000.,"cpuMs":cpu,"completionMs":start.elapsed().as_secs_f64()*1000.}))
    })}).await.map_err(|e|e.to_string())?.map_err(|e|format!("{e:#}"))
}
#[tauri::command]
pub async fn metal_stop(window: tauri::WebviewWindow) -> std::result::Result<(), String> {
    let old = state().lock().unwrap().take();
    if let Some(native) = old {
        let view = native.view;
        window
            .with_webview(move |_| unsafe {
                let view = view as *mut Object;
                let _: () = msg_send![view, removeFromSuperview];
            })
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub fn metal_capabilities() -> std::result::Result<Value, String> {
    let device = Device::system_default().ok_or("Metal device unavailable")?;
    Ok(json!({"backend":"Metal","device":device.name(),"experimental":true}))
}
