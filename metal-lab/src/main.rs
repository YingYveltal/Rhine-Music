use anyhow::{bail, Context, Result};
use glam::Mat4;
use metal::*;
use objc::{msg_send, rc::autoreleasepool, sel, sel_impl};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{ffi::c_void, fs, mem::size_of, path::Path, time::Instant};
mod visibility;
#[derive(Deserialize)]
struct Geometry {
    positions: Vec<f32>,
    indices: Vec<u32>,
}
#[derive(Deserialize)]
struct Instance {
    geometry: usize,
    matrix: [f32; 16],
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Capture {
    width: u32,
    height: u32,
    uniforms: Value,
    exposure: f32,
    camera_world: [f32; 16],
    projection: [f32; 16],
    inverse_projection: [f32; 16],
    geometries: Vec<Geometry>,
    instances: Vec<Instance>,
}
#[repr(C)]
#[derive(Clone, Copy)]
struct Post {
    width: u32,
    height: u32,
    mode: u32,
    pad: u32,
    focus: f32,
    aspect: f32,
    aperture: f32,
    maxblur: f32,
    near: f32,
    far: f32,
    exposure: f32,
    pad2: f32,
}
#[repr(C)]
#[derive(Clone, Copy)]
struct Camera {
    world: [f32; 16],
    inverse: [f32; 16],
    vp: [f32; 16],
    width: u32,
    height: u32,
    secondary: u32,
    pad: u32,
}
#[repr(C)]
#[derive(Clone, Copy)]
struct Model {
    matrix: [f32; 16],
    id: u32,
    geometry: u32,
    pad: [u32; 2],
}
struct Lab {
    d: Device,
    q: CommandQueue,
    lib: Library,
}
fn buffer<T>(d: &Device, v: &[T]) -> Buffer {
    d.new_buffer_with_data(
        v.as_ptr() as *const c_void,
        std::mem::size_of_val(v) as u64,
        MTLResourceOptions::StorageModeShared,
    )
}
fn texture(d: &Device, w: u32, h: u32, f: MTLPixelFormat) -> Texture {
    let td = TextureDescriptor::new();
    td.set_texture_type(MTLTextureType::D2);
    td.set_width(w as u64);
    td.set_height(h as u64);
    td.set_pixel_format(f);
    td.set_usage(
        MTLTextureUsage::ShaderRead | MTLTextureUsage::ShaderWrite | MTLTextureUsage::RenderTarget,
    );
    td.set_storage_mode(MTLStorageMode::Shared);
    d.new_texture(&td)
}
fn pipeline(l: &Lab, name: &str) -> Result<ComputePipelineState> {
    l.d.new_compute_pipeline_state_with_function(
        l.lib
            .get_function(name, None)
            .map_err(anyhow::Error::msg)?
            .as_ref(),
    )
    .map_err(anyhow::Error::msg)
}
fn read(t: &Texture, bpp: usize) -> Vec<u8> {
    let mut b = vec![0; t.width() as usize * t.height() as usize * bpp];
    t.get_bytes(
        b.as_mut_ptr() as *mut c_void,
        t.width() * bpp as u64,
        MTLRegion::new_2d(0, 0, t.width(), t.height()),
        0,
    );
    b
}
fn save_png(path: &Path, b: &[u8], w: u32, h: u32) -> Result<()> {
    let mut img = image::RgbaImage::from_raw(w, h, b.to_vec()).context("png size")?;
    image::imageops::flip_vertical_in_place(&mut img);
    img.save(path)?;
    Ok(())
}
fn summary(mut v: Vec<f64>) -> Value {
    v.sort_by(f64::total_cmp);
    let n = v.len();
    json!({"samples":n,"mean":v.iter().sum::<f64>()/n as f64,"p50":v[n/2],"p95":v[(n as f64*0.95) as usize],"p99":v[(n as f64*0.99) as usize],"max":v[n-1]})
}
fn bench<F: FnMut(&CommandBufferRef, usize)>(l: &Lab, n: usize, mut encode: F) -> Result<Value> {
    let mut gpu = vec![];
    let mut wall = vec![];
    let mut cpu = vec![];
    for i in 0..n + 10 {
        autoreleasepool(|| -> Result<()> {
            let start = Instant::now();
            let cb = l.q.new_command_buffer();
            encode(cb, i);
            let submission = start.elapsed().as_secs_f64() * 1000.;
            cb.commit();
            cb.wait_until_completed();
            if cb.status() != MTLCommandBufferStatus::Completed {
                bail!("Metal command failed: {:?}", cb.status());
            }
            let (a, b): (f64, f64) =
                unsafe { (msg_send![cb, GPUStartTime], msg_send![cb, GPUEndTime]) };
            if i >= 10 {
                if b <= a {
                    bail!("GPU timestamps unavailable");
                }
                gpu.push((b - a) * 1000.);
                wall.push(start.elapsed().as_secs_f64() * 1000.);
                cpu.push(submission);
            }
            Ok(())
        })?;
    }
    Ok(
        json!({"gpuMs":summary(gpu),"submitToCompletionMs":summary(wall),"cpuEncodeMs":summary(cpu),"warmup":10,"inFlight":1}),
    )
}
fn dispatch_post(
    cb: &CommandBufferRef,
    pipeline: &ComputePipelineState,
    color: &Texture,
    depth: &Texture,
    out: &Texture,
    post: &Post,
) {
    let e = cb.new_compute_command_encoder();
    e.set_compute_pipeline_state(pipeline);
    e.set_texture(0, Some(color));
    e.set_texture(1, Some(depth));
    e.set_texture(2, Some(out));
    e.set_bytes(
        0,
        size_of::<Post>() as u64,
        post as *const _ as *const c_void,
    );
    e.dispatch_threads(
        MTLSize::new(post.width as u64, post.height as u64, 1),
        MTLSize::new(16, 16, 1),
    );
    e.end_encoding();
}
fn difference(a: &[u8], b: &[u8]) -> Value {
    let ds: Vec<_> = a
        .iter()
        .zip(b)
        .map(|(&x, &y)| x.abs_diff(y) as f64)
        .collect();
    let above = ds.iter().filter(|&&d| d > 2.).count();
    json!({"maxChannelDifference":ds.iter().copied().fold(0.,f64::max),"meanChannelDifference":ds.iter().sum::<f64>()/ds.len() as f64,"fractionAbove2":above as f64/ds.len() as f64})
}
fn postprocess(l: &Lab, c: &Capture, dir: &Path, out: &Path) -> Result<Value> {
    let color = texture(&l.d, c.width, c.height, MTLPixelFormat::RGBA16Float);
    let depth = texture(&l.d, c.width, c.height, MTLPixelFormat::RGBA16Float);
    for (t, name) in [(&color, "color.rgba16f"), (&depth, "depth.rgba16f")] {
        let b = fs::read(dir.join(name))?;
        if b.len() != c.width as usize * c.height as usize * 8 {
            bail!("Invalid {name} size");
        }
        t.replace_region(
            MTLRegion::new_2d(0, 0, c.width as u64, c.height as u64),
            0,
            b.as_ptr() as *const c_void,
            c.width as u64 * 8,
        );
    }
    let hdr = texture(&l.d, c.width, c.height, MTLPixelFormat::RGBA16Float);
    let display = texture(&l.d, c.width, c.height, MTLPixelFormat::RGBA8Unorm);
    let bokeh = pipeline(l, "bokeh")?;
    let output = pipeline(l, "outputColor")?;
    let fused = pipeline(l, "fused")?;
    let u = |s: &str| c.uniforms[s].as_f64().unwrap() as f32;
    let mut p = Post {
        width: c.width,
        height: c.height,
        mode: 0,
        pad: 0,
        focus: u("focus"),
        aspect: u("aspect"),
        aperture: u("aperture"),
        maxblur: u("maxblur"),
        near: u("nearClip"),
        far: u("farClip"),
        exposure: c.exposure,
        pad2: 0.,
    };
    let reference = fs::read(dir.join("reference.rgba8"))?;
    if reference.len() != c.width as usize * c.height as usize * 4 {
        bail!("Invalid reference image length");
    }
    save_png(
        &out.join("webgl-reference.png"),
        &reference,
        c.width,
        c.height,
    )?;
    let mut results = serde_json::Map::new();
    for (name, mode, combine) in [
        ("original-41-tap-two-pass", 0, false),
        ("exact-bilinear-two-pass", 1, false),
        ("exact-bilinear-fused", 1, true),
    ] {
        p.mode = mode;
        let timing = bench(l, 100, |cb, _| {
            if combine {
                dispatch_post(cb, &fused, &color, &depth, &display, &p)
            } else {
                dispatch_post(cb, &bokeh, &color, &depth, &hdr, &p);
                dispatch_post(cb, &output, &hdr, &depth, &display, &p);
            }
        })?;
        let image = read(&display, 4);
        save_png(&out.join(format!("{name}.png")), &image, c.width, c.height)?;
        let parity = difference(&reference, &image);
        if parity["maxChannelDifference"].as_f64().unwrap() > 2. {
            bail!("{name}: pixel parity exceeds 2/255: {parity}");
        }
        results.insert(name.into(), json!({"timing":timing,"versusWebGL":parity}));
    }
    let observed = read(&hdr, 8);
    let expected = fs::read(dir.join("bokeh.rgba16f"))?;
    if observed.len() != expected.len() {
        bail!("Invalid bokeh reference length");
    }
    let mut max = 0f32;
    let mut sum = 0f64;
    for (a, b) in observed.chunks_exact(2).zip(expected.chunks_exact(2)) {
        let a = half::f16::from_bits(u16::from_le_bytes([a[0], a[1]])).to_f32();
        let b = half::f16::from_bits(u16::from_le_bytes([b[0], b[1]])).to_f32();
        let d = (a - b).abs();
        max = max.max(d);
        sum += d as f64;
    }
    Ok(
        json!({"results":results,"hdrDifference":{"max":max,"mean":sum/(observed.len()/2) as f64},"scope":"Resident captured HDR and original packed-depth textures, same 41 taps and ACES/sRGB. Excludes scene rendering, depth generation, upload, WebView interop and presentation. These timings are not application FPS."}),
    )
}
fn main() -> Result<()> {
    autoreleasepool(|| {
        let args: Vec<_> = std::env::args().collect();
        if args.len() != 3 {
            bail!("Usage: rhine-metal-lab CAPTURE_DIRECTORY OUTPUT_DIRECTORY");
        }
        let dir = Path::new(&args[1]);
        let out = Path::new(&args[2]);
        fs::create_dir_all(out)?;
        let capture: Capture = serde_json::from_slice(&fs::read(dir.join("scene.json"))?)?;
        let d = Device::system_default().context("Metal unavailable")?;
        let options = CompileOptions::new();
        options.set_fast_math_enabled(false);
        options.set_language_version(MTLLanguageVersion::V2_4);
        let lib = d
            .new_library_with_source(include_str!("lab.metal"), &options)
            .map_err(anyhow::Error::msg)?;
        let q = d.new_command_queue();
        let lab = Lab { d, q, lib };
        let hardware = json!({"device":lab.d.name(),"supportsRaytracing":lab.d.supports_raytracing(),"hasUnifiedMemory":lab.d.has_unified_memory(),"appleFamily9":lab.d.supports_family(MTLGPUFamily::Apple9),"metalShaderVersion":"2.4","fastMath":false});
        println!("{hardware}");
        let post = postprocess(&lab, &capture, dir, out)?;
        println!("Postprocessing complete");
        let ray = visibility::run(&lab, &capture, out)?;
        let report = json!({"hardware":hardware,"capture":dir.file_name().unwrap().to_string_lossy(),"resolution":[capture.width,capture.height],"postprocessing":post,"raytracing":ray});
        fs::write(
            out.join("results.json"),
            serde_json::to_vec_pretty(&report)?,
        )?;
        println!("Saved {}", out.join("results.json").display());
        Ok(())
    })
}
