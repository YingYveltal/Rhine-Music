//! Native execution of original Three.js shaders and captured draw commands.
//! Inputs are geometry/textures/uniforms. No captured scene image is displayed.
use anyhow::{bail, Context, Result};
use base64::Engine;
use half::f16;
use metal::*;
use objc::{msg_send, rc::autoreleasepool, sel, sel_impl};
use serde_json::{json, Value};
use std::{collections::HashMap, fs, path::Path, time::Instant};
#[path = "frame_window.rs"]
mod frame_window;

fn n(v: &Value) -> u64 {
    v.as_u64().unwrap_or(0)
}
fn f(v: &Value) -> f64 {
    v.as_f64().unwrap_or(0.)
}
fn b(v: &Value) -> bool {
    v.as_bool().unwrap_or(false)
}
fn a(v: &Value) -> &[Value] {
    v.as_array().map(Vec::as_slice).unwrap_or(&[])
}
fn s(v: &Value) -> &str {
    v.as_str().unwrap_or("")
}
fn data<'a>(bytes: &'a [u8], range: &Value) -> &'a [u8] {
    &bytes[n(&range["offset"]) as usize..(n(&range["offset"]) + n(&range["length"])) as usize]
}
fn buf(d: &Device, bytes: &[u8]) -> Buffer {
    let buffer = d.new_buffer(
        bytes.len().max(16) as u64,
        MTLResourceOptions::StorageModeShared,
    );
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), buffer.contents().cast::<u8>(), bytes.len());
    }
    buffer
}
fn format(v: u64) -> Result<MTLPixelFormat> {
    Ok(match v {
        0x8058 | 0x1908 | 0x8051 | 0x1907 => MTLPixelFormat::RGBA8Unorm,
        0x8c43 => MTLPixelFormat::RGBA8Unorm_sRGB,
        0x881a | 0x881b => MTLPixelFormat::RGBA16Float,
        0x8814 | 0x8815 => MTLPixelFormat::RGBA32Float,
        0x822f => MTLPixelFormat::RG16Float,
        0x822e => MTLPixelFormat::R32Float,
        0x81a6 | 0x81a5 | 0x8cac | 0x1902 | 0x88f0 => MTLPixelFormat::Depth32Float,
        _ => bail!("Unsupported GL format {v:#x}"),
    })
}
fn tex(d: &Device, v: &Value) -> Result<Texture> {
    let desc = TextureDescriptor::new();
    desc.set_width(n(&v["width"]));
    desc.set_height(n(&v["height"]));
    let samples = n(&v["samples"]).max(1);
    desc.set_pixel_format(format(n(&v["format"]))?);
    desc.set_sample_count(samples);
    desc.set_texture_type(if samples > 1 {
        MTLTextureType::D2Multisample
    } else {
        MTLTextureType::D2
    });
    desc.set_mipmap_level_count(if b(&v["mipmap"]) {
        64 - n(&v["width"]).max(n(&v["height"])).leading_zeros() as u64
    } else {
        n(&v["levels"]).max(1)
    });
    desc.set_usage(MTLTextureUsage::RenderTarget | MTLTextureUsage::ShaderRead);
    desc.set_storage_mode(
        if samples > 1 || desc.pixel_format() == MTLPixelFormat::Depth32Float {
            MTLStorageMode::Private
        } else {
            MTLStorageMode::Shared
        },
    );
    Ok(d.new_texture(&desc))
}
fn compare(v: u64) -> MTLCompareFunction {
    match v {
        0x200 => MTLCompareFunction::Never,
        0x201 => MTLCompareFunction::Less,
        0x202 => MTLCompareFunction::Equal,
        0x203 => MTLCompareFunction::LessEqual,
        0x204 => MTLCompareFunction::Greater,
        0x205 => MTLCompareFunction::NotEqual,
        0x206 => MTLCompareFunction::GreaterEqual,
        _ => MTLCompareFunction::Always,
    }
}
fn factor(v: u64) -> Result<MTLBlendFactor> {
    Ok(match v {
        0 => MTLBlendFactor::Zero,
        1 => MTLBlendFactor::One,
        0x300 => MTLBlendFactor::SourceColor,
        0x301 => MTLBlendFactor::OneMinusSourceColor,
        0x302 => MTLBlendFactor::SourceAlpha,
        0x303 => MTLBlendFactor::OneMinusSourceAlpha,
        0x304 => MTLBlendFactor::DestinationAlpha,
        0x305 => MTLBlendFactor::OneMinusDestinationAlpha,
        0x306 => MTLBlendFactor::DestinationColor,
        0x307 => MTLBlendFactor::OneMinusDestinationColor,
        _ => bail!("Blend factor {v:#x}"),
    })
}
fn operation(v: u64) -> Result<MTLBlendOperation> {
    Ok(match v {
        0x8006 => MTLBlendOperation::Add,
        0x800a => MTLBlendOperation::Subtract,
        0x800b => MTLBlendOperation::ReverseSubtract,
        0x8007 => MTLBlendOperation::Min,
        0x8008 => MTLBlendOperation::Max,
        _ => bail!("Blend operation {v:#x}"),
    })
}
fn components(t: u64) -> usize {
    match t {
        0x8b50 | 0x8b53 | 0x8b57 => 2,
        0x8b51 | 0x8b54 | 0x8b58 => 3,
        0x8b52 | 0x8b55 | 0x8b59 | 0x8b5a => 4,
        0x8b5b => 9,
        0x8b5c => 16,
        _ => 1,
    }
}
fn uniform_values(program: &Value, draw: &Value) -> HashMap<String, Vec<f64>> {
    let mut out = HashMap::new();
    for u in a(&program["uniforms"]) {
        let name = s(&u["name"]);
        let v = &draw["uniforms"][name];
        let values: Vec<f64> = if v.is_array() {
            a(v).iter()
                .map(|v| {
                    if v.is_boolean() {
                        b(v) as u8 as f64
                    } else {
                        f(v)
                    }
                })
                .collect()
        } else {
            vec![if v.is_boolean() {
                b(v) as u8 as f64
            } else {
                f(v)
            }]
        };
        let count = n(&u["size"]) as usize;
        let c = components(n(&u["type"]));
        if count > 1 {
            for i in 0..count {
                out.insert(
                    name.replacen("[0]", &format!("[{i}]"), 1),
                    values[i * c..(i + 1) * c].to_vec(),
                );
            }
        } else {
            out.insert(name.to_string(), values);
        }
    }
    out
}
fn pack_member(
    reflection: &Value,
    member: &Value,
    offset: usize,
    name: &str,
    values: &HashMap<String, Vec<f64>>,
    out: &mut [u8],
) -> Result<()> {
    let offset = offset + n(&member["offset"]) as usize;
    let typ = s(&member["type"]);
    let count = a(&member["array"]).first().map(n).unwrap_or(1) as usize;
    for i in 0..count {
        let name = if member["array"].is_array() {
            format!("{name}[{i}]")
        } else {
            name.to_string()
        };
        let base = offset + i * n(&member["array_stride"]) as usize;
        let members = a(&reflection["types"][typ]["members"]);
        if !members.is_empty() {
            for field in members {
                pack_member(
                    reflection,
                    field,
                    base,
                    &format!("{name}.{}", s(&field["name"])),
                    values,
                    out,
                )?;
            }
            continue;
        }
        let Some(v) = values.get(&name) else {
            continue;
        }; // inactive declarations retained by glslang
        if typ.starts_with("mat") {
            let dim = typ[3..].parse::<usize>().context("matrix dimensions")?;
            let stride = n(&member["matrix_stride"]) as usize;
            for col in 0..dim {
                for row in 0..dim {
                    let pos = base + col * stride + row * 4;
                    out[pos..pos + 4].copy_from_slice(&(v[col * dim + row] as f32).to_le_bytes());
                }
            }
        } else {
            for (j, &x) in v.iter().enumerate() {
                let bytes = if typ == "bool" || typ == "uint" || typ.starts_with("uvec") {
                    (x as u32).to_le_bytes()
                } else if typ == "int" || typ.starts_with("ivec") {
                    (x as i32).to_le_bytes()
                } else {
                    (x as f32).to_le_bytes()
                };
                let pos = base + j * 4;
                out[pos..pos + 4].copy_from_slice(&bytes);
            }
        }
    }
    Ok(())
}
fn uniforms(
    d: &Device,
    r: &Value,
    values: &HashMap<String, Vec<f64>>,
) -> Result<Option<(u64, Buffer)>> {
    let Some(block) = a(&r["ubos"]).first() else {
        return Ok(None);
    };
    let mut bytes = vec![0; (n(&block["block_size"]) as usize).max(16)];
    for m in a(&r["types"][s(&block["type"])]["members"]) {
        pack_member(r, m, 0, s(&m["name"]), values, &mut bytes)?;
    }
    Ok(Some((n(&block["binding"]), buf(d, &bytes))))
}
#[derive(Clone)]
struct BoundTexture {
    slot: u64,
    texture: usize,
    sampler: SamplerState,
}
fn sampler(d: &Device, v: &Value) -> SamplerState {
    let desc = SamplerDescriptor::new();
    let min = n(&v["min"]);
    desc.set_min_filter(if [0x2600, 0x2700, 0x2702].contains(&min) {
        MTLSamplerMinMagFilter::Nearest
    } else {
        MTLSamplerMinMagFilter::Linear
    });
    desc.set_mag_filter(if n(&v["mag"]) == 0x2600 {
        MTLSamplerMinMagFilter::Nearest
    } else {
        MTLSamplerMinMagFilter::Linear
    });
    desc.set_mip_filter(if [0x2700, 0x2701].contains(&min) {
        MTLSamplerMipFilter::Nearest
    } else if [0x2702, 0x2703].contains(&min) {
        MTLSamplerMipFilter::Linear
    } else {
        MTLSamplerMipFilter::NotMipmapped
    });
    let wrap = |v: u64| match v {
        0x2901 => MTLSamplerAddressMode::Repeat,
        0x8370 => MTLSamplerAddressMode::MirrorRepeat,
        _ => MTLSamplerAddressMode::ClampToEdge,
    };
    desc.set_address_mode_s(wrap(n(&v["wrapS"])));
    desc.set_address_mode_t(wrap(n(&v["wrapT"])));
    desc.set_max_anisotropy(n(&v["anisotropy"]).clamp(1, 16));
    if n(&v["compare"]) != 0 {
        desc.set_compare_function(compare(n(&v["compareFunc"])));
    }
    d.new_sampler(&desc)
}
fn bindings(d: &Device, reflection: &Value, draw: &Value) -> Result<Vec<BoundTexture>> {
    let mut out = vec![];
    for t in a(&reflection["textures"]) {
        let name = s(&t["name"]);
        let mut v = &draw["samplers"][name];
        if v.is_null() {
            v = &draw["samplers"][format!("{name}[0]")];
        }
        if v.is_null() {
            bail!("Missing sampler {name}");
        }
        for (i, value) in a(v).iter().enumerate() {
            out.push(BoundTexture {
                slot: n(&t["binding"]) + i as u64,
                texture: n(&value["texture"]) as usize,
                sampler: sampler(d, value),
            });
        }
    }
    Ok(out)
}
struct Target {
    color: Option<Texture>,
    depth: Option<Texture>,
    width: u64,
    height: u64,
    samples: u64,
}
#[derive(Clone)]
struct Draw {
    program: usize,
    pipeline: RenderPipelineState,
    depth: DepthStencilState,
    attributes: Vec<(u64, Buffer, u64)>,
    vertex: Option<(u64, Buffer)>,
    fragment: Option<(u64, Buffer)>,
    vertex_textures: Vec<BoundTexture>,
    fragment_textures: Vec<BoundTexture>,
    state: Value,
    index: Option<Buffer>,
    index_type: MTLIndexType,
    first: u64,
    count: u64,
    instances: u64,
    primitive: MTLPrimitiveType,
}
enum Event {
    Pass {
        target: usize,
        color: Option<[f64; 4]>,
        depth: Option<f64>,
        draws: Vec<usize>,
    },
    Blit {
        from: usize,
        to: usize,
        mask: u64,
    },
    Mipmap(usize),
}
pub struct Renderer {
    pub device: Device,
    pub queue: CommandQueue,
    textures: Vec<Texture>,
    targets: Vec<Target>,
    draws: Vec<Draw>,
    events: Vec<Event>,
    pub screen: Texture,
    pub width: u32,
    pub height: u32,
    frame: Value,
    buffers: Vec<Buffer>,
    programs: Value,
    prototypes: HashMap<String, Draw>,
    updated_textures: Vec<usize>,
}
fn draw_key(command: &Value, targets: &[Target]) -> String {
    let target = &targets[n(&command["target"]) as usize];
    let state = &command["state"];
    let fields: Vec<_> = [
        "blend",
        "blendSrc",
        "blendDst",
        "blendSrcAlpha",
        "blendDstAlpha",
        "blendEquation",
        "blendEquationAlpha",
        "colorMask",
        "depthTest",
        "depthWrite",
        "depthFunc",
    ]
    .iter()
    .map(|k| state[*k].clone())
    .collect();
    let attrs: Vec<_> = a(&command["attributes"])
        .iter()
        .map(|v| {
            json!([
                v["location"],
                v["enabled"],
                v["size"],
                v["type"],
                v["normalized"],
                v["stride"],
                v["divisor"]
            ])
        })
        .collect();
    format!(
        "{}-{}-{:?}-{:?}-{}-{}",
        n(&command["program"]),
        target.samples,
        target.color.as_ref().map(|t| t.pixel_format()),
        target.depth.as_ref().map(|t| t.pixel_format()),
        json!(fields),
        json!(attrs)
    )
}
impl Renderer {
    pub fn load(capture: &Path, translated: &Path) -> Result<Self> {
        let frame: Value = serde_json::from_slice(&fs::read(capture.join("gl-frame.json"))?)?;
        let programs: Value = serde_json::from_slice(&fs::read(translated.join("programs.json"))?)?;
        let mut bytes = vec![];
        for file in a(&frame["files"]) {
            bytes.extend(fs::read(capture.join(s(file)))?);
        }
        if bytes.len() as u64 != n(&frame["byteLength"]) {
            bail!("Incomplete capture");
        }
        let device = Device::system_default().context("Metal device unavailable")?;
        let queue = device.new_command_queue();
        let mut textures = vec![];
        for v in a(&frame["textures"]) {
            let t = tex(&device, v)?;
            if !v["data"].is_null() {
                let input = data(&bytes, &v["data"]);
                let converted: Vec<u8> = if t.pixel_format() == MTLPixelFormat::RG16Float {
                    input
                        .chunks_exact(16)
                        .flat_map(|pixel| {
                            pixel[..8].chunks_exact(4).flat_map(|b| {
                                f16::from_f32(f32::from_le_bytes(b.try_into().unwrap()))
                                    .to_le_bytes()
                            })
                        })
                        .collect()
                } else if t.pixel_format() == MTLPixelFormat::R32Float {
                    input
                        .chunks_exact(16)
                        .flat_map(|p| p[..4].iter().copied())
                        .collect()
                } else if t.pixel_format() == MTLPixelFormat::RGBA16Float {
                    input
                        .chunks_exact(4)
                        .flat_map(|b| {
                            f16::from_f32(f32::from_le_bytes(b.try_into().unwrap())).to_le_bytes()
                        })
                        .collect()
                } else {
                    input.to_vec()
                };
                let bpp = match t.pixel_format() {
                    MTLPixelFormat::RGBA32Float => 16,
                    MTLPixelFormat::RGBA16Float => 8,
                    _ => 4,
                };
                if converted.len() as u64 != t.width() * t.height() * bpp {
                    bail!("Texture byte count");
                }
                t.replace_region(
                    MTLRegion::new_2d(0, 0, t.width(), t.height()),
                    0,
                    converted.as_ptr().cast(),
                    t.width() * bpp,
                );
                if t.mipmap_level_count() > 1 {
                    let cb = queue.new_command_buffer();
                    let e = cb.new_blit_command_encoder();
                    e.generate_mipmaps(&t);
                    e.end_encoding();
                    cb.commit();
                    cb.wait_until_completed();
                }
            }
            textures.push(t);
        }
        let make_attachment = |v: &Value| -> Result<Option<Texture>> {
            if v.is_null() {
                Ok(None)
            } else if let Some(id) = v["texture"].as_u64() {
                Ok(Some(textures[id as usize].clone()))
            } else {
                Ok(Some(tex(&device, v)?))
            }
        };
        let mut targets = vec![];
        let mut screen = None;
        for v in a(&frame["targets"]) {
            let color = make_attachment(&v["color"])?;
            let depth = make_attachment(&v["depth"])?;
            if b(&v["screen"]) {
                screen = color.clone();
            }
            let t = color
                .as_ref()
                .or(depth.as_ref())
                .context("Empty framebuffer")?;
            targets.push(Target {
                width: t.width(),
                height: t.height(),
                samples: t.sample_count(),
                color,
                depth,
            });
        }
        let buffers: Vec<_> = a(&frame["buffers"])
            .iter()
            .map(|v| buf(&device, data(&bytes, v)))
            .collect();
        let options = CompileOptions::new();
        options.set_language_version(MTLLanguageVersion::V2_4);
        options.set_fast_math_enabled(std::env::var_os("RHINE_METAL_FAST_MATH").is_some());
        let mut functions = vec![];
        for (i, p) in a(&programs).iter().enumerate() {
            let mut funcs = vec![];
            for stage in ["vert", "frag"] {
                let source = fs::read_to_string(translated.join(s(&p[stage]["source"])))?;
                let library = device
                    .new_library_with_source(&source, &options)
                    .map_err(anyhow::Error::msg)
                    .with_context(|| format!("program {i} {stage}"))?;
                funcs.push(
                    library
                        .get_function("main0", None)
                        .map_err(anyhow::Error::msg)?,
                );
            }
            functions.push(funcs);
        }
        let mut draws = vec![];
        let mut events = vec![];
        for command in a(&frame["commands"]) {
            match s(&command["op"]) {
                "clear" => {
                    let mask = n(&command["mask"]);
                    let c = a(&command["color"]);
                    events.push(Event::Pass {
                        target: n(&command["target"]) as usize,
                        color: if mask & 0x4000 != 0 {
                            Some([f(&c[0]), f(&c[1]), f(&c[2]), f(&c[3])])
                        } else {
                            None
                        },
                        depth: if mask & 0x100 != 0 {
                            Some(f(&command["depth"]))
                        } else {
                            None
                        },
                        draws: vec![],
                    });
                }
                "draw" => {
                    let pid = n(&command["program"]) as usize;
                    let tid = n(&command["target"]) as usize;
                    let target = &targets[tid];
                    let state = &command["state"];
                    let pd = RenderPipelineDescriptor::new();
                    pd.set_vertex_function(Some(&functions[pid][0]));
                    pd.set_fragment_function(Some(&functions[pid][1]));
                    pd.set_raster_sample_count(target.samples);
                    if let Some(color) = &target.color {
                        let att = pd.color_attachments().object_at(0).unwrap();
                        att.set_pixel_format(color.pixel_format());
                        let mask = a(&state["colorMask"])
                            .iter()
                            .enumerate()
                            .fold(0, |v, (i, x)| v | if b(x) { 1 << (3 - i) } else { 0 });
                        att.set_write_mask(MTLColorWriteMask::from_bits_truncate(mask));
                        att.set_blending_enabled(b(&state["blend"]));
                        if b(&state["blend"]) {
                            att.set_source_rgb_blend_factor(factor(n(&state["blendSrc"]))?);
                            att.set_destination_rgb_blend_factor(factor(n(&state["blendDst"]))?);
                            att.set_source_alpha_blend_factor(factor(n(&state["blendSrcAlpha"]))?);
                            att.set_destination_alpha_blend_factor(factor(n(
                                &state["blendDstAlpha"]
                            ))?);
                            att.set_rgb_blend_operation(operation(n(&state["blendEquation"]))?);
                            att.set_alpha_blend_operation(operation(n(
                                &state["blendEquationAlpha"]
                            ))?);
                        }
                    }
                    if let Some(depth) = &target.depth {
                        pd.set_depth_attachment_pixel_format(depth.pixel_format());
                    }
                    let vd = VertexDescriptor::new();
                    let mut attributes = vec![];
                    for attr in a(&command["attributes"]) {
                        let loc = n(&attr["location"]);
                        if loc >= 24 {
                            bail!("Too many attributes");
                        }
                        let (buffer, offset, stride, fmt, step, rate) = if b(&attr["enabled"]) {
                            let size = n(&attr["size"]);
                            let typ = n(&attr["type"]);
                            let fmt = match (typ, size) {
                                (0x1406, 1) => MTLVertexFormat::Float,
                                (0x1406, 2) => MTLVertexFormat::Float2,
                                (0x1406, 3) => MTLVertexFormat::Float3,
                                (0x1406, 4) => MTLVertexFormat::Float4,
                                _ => bail!("Attribute type {typ:#x}/{size}"),
                            };
                            let divisor = n(&attr["divisor"]);
                            (
                                buffers[n(&attr["buffer"]) as usize].clone(),
                                n(&attr["offset"]),
                                n(&attr["stride"]).max(size * 4),
                                fmt,
                                if divisor > 0 {
                                    MTLVertexStepFunction::PerInstance
                                } else {
                                    MTLVertexStepFunction::PerVertex
                                },
                                divisor.max(1),
                            )
                        } else {
                            let values: Vec<u8> = a(&attr["value"])
                                .iter()
                                .flat_map(|v| (f(v) as f32).to_le_bytes())
                                .collect();
                            (
                                buf(&device, &values),
                                0,
                                16,
                                MTLVertexFormat::Float4,
                                MTLVertexStepFunction::Constant,
                                1,
                            )
                        };
                        let ad = vd.attributes().object_at(loc).unwrap();
                        ad.set_format(fmt);
                        ad.set_buffer_index(loc);
                        ad.set_offset(0);
                        let layout = vd.layouts().object_at(loc).unwrap();
                        layout.set_stride(stride);
                        layout.set_step_function(step);
                        layout.set_step_rate(rate);
                        attributes.push((loc, buffer, offset));
                    }
                    pd.set_vertex_descriptor(Some(&vd));
                    let pipeline = device
                        .new_render_pipeline_state(&pd)
                        .map_err(anyhow::Error::msg)
                        .with_context(|| format!("Pipeline {} program {pid}", draws.len()))?;
                    let dd = DepthStencilDescriptor::new();
                    dd.set_depth_compare_function(if b(&state["depthTest"]) {
                        compare(n(&state["depthFunc"]))
                    } else {
                        MTLCompareFunction::Always
                    });
                    dd.set_depth_write_enabled(b(&state["depthTest"]) && b(&state["depthWrite"]));
                    let values = uniform_values(&frame["programs"][pid], command);
                    let mut stage_uniforms = vec![];
                    let mut stage_textures = vec![];
                    for stage in ["vert", "frag"] {
                        let r = &programs[pid][stage]["reflection"];
                        stage_uniforms.push(uniforms(&device, r, &values)?);
                        stage_textures.push(bindings(&device, r, command)?);
                    }
                    let primitive = match n(&command["mode"]) {
                        0 => MTLPrimitiveType::Point,
                        4 => MTLPrimitiveType::Triangle,
                        5 => MTLPrimitiveType::TriangleStrip,
                        m => bail!("Primitive {m}"),
                    };
                    let draw = Draw {
                        program: pid,
                        pipeline,
                        depth: device.new_depth_stencil_state(&dd),
                        attributes,
                        vertex: stage_uniforms.remove(0),
                        fragment: stage_uniforms.remove(0),
                        vertex_textures: stage_textures.remove(0),
                        fragment_textures: stage_textures.remove(0),
                        state: state.clone(),
                        index: command["index"]
                            .as_u64()
                            .map(|i| buffers[i as usize].clone()),
                        index_type: if n(&command["indexType"]) == 0x1405 {
                            MTLIndexType::UInt32
                        } else {
                            MTLIndexType::UInt16
                        },
                        first: n(&command["first"]),
                        count: n(&command["count"]),
                        instances: n(&command["instances"]),
                        primitive,
                    };
                    if !matches!(events.last(),Some(Event::Pass{target,..}) if *target == tid) {
                        events.push(Event::Pass {
                            target: tid,
                            color: None,
                            depth: None,
                            draws: vec![],
                        });
                    }
                    if let Some(Event::Pass { draws: list, .. }) = events.last_mut() {
                        list.push(draws.len());
                    }
                    draws.push(draw);
                }
                "blit" => {
                    let args = a(&command["args"]);
                    events.push(Event::Blit {
                        from: n(&command["from"]) as usize,
                        to: n(&command["to"]) as usize,
                        mask: n(&args[8]),
                    });
                }
                "mipmap" => events.push(Event::Mipmap(n(&command["texture"]) as usize)),
                op => bail!("Unknown frame operation {op}"),
            }
        }
        println!(
            "Metal: {} programs, {} draws, {} passes/operations, {} textures",
            functions.len(),
            draws.len(),
            events.len(),
            textures.len()
        );
        let prototypes = a(&frame["commands"])
            .iter()
            .filter(|c| s(&c["op"]) == "draw")
            .zip(&draws)
            .map(|(c, d)| (draw_key(c, &targets), d.clone()))
            .collect();
        Ok(Self {
            device,
            queue,
            textures,
            targets,
            draws,
            events,
            screen: screen.context("No screen target")?,
            width: n(&frame["width"]) as u32,
            height: n(&frame["height"]) as u32,
            frame,
            buffers,
            programs,
            prototypes,
            updated_textures: vec![],
        })
    }
    /// Caller must wait for the previous command buffer before changing shared
    /// instance/uniform storage. Requests are latest-only on the frontend.
    pub fn update_frame(&mut self, update: &Value) -> Result<()> {
        let mut allocations: Vec<_> = a(&update["allocations"]).iter().collect();
        allocations.sort_by_key(|v| (s(&v["kind"]), n(&v["id"])));
        for allocation in allocations {
            let id = n(&allocation["id"]) as usize;
            match s(&allocation["kind"]) {
                "texture" => {
                    let texture = tex(&self.device, allocation)?;
                    if id == self.textures.len() {
                        self.textures.push(texture);
                    } else if id < self.textures.len() {
                        self.textures[id] = texture;
                    } else {
                        bail!("Texture allocation sequence");
                    }
                }
                "buffer" => {
                    let buffer = self.device.new_buffer(
                        n(&allocation["length"]).max(16),
                        MTLResourceOptions::StorageModeShared,
                    );
                    if id == self.buffers.len() {
                        self.buffers.push(buffer);
                    } else if id < self.buffers.len() {
                        self.buffers[id] = buffer;
                    } else {
                        bail!("Buffer allocation sequence");
                    }
                }
                _ => bail!("Unknown resource allocation"),
            }
        }
        self.updated_textures.clear();
        for upload in a(&update["textureUploads"]) {
            let id = n(&upload["texture"]) as usize;
            let texture = self.textures.get(id).context("Unknown upload texture")?;
            let x = n(&upload["x"]);
            let y = n(&upload["y"]);
            let width = n(&upload["width"]);
            let height = n(&upload["height"]);
            let bytes = base64::engine::general_purpose::STANDARD.decode(s(&upload["data"]))?;
            if !matches!(
                texture.pixel_format(),
                MTLPixelFormat::RGBA8Unorm | MTLPixelFormat::RGBA8Unorm_sRGB
            ) || x + width > texture.width()
                || y + height > texture.height()
                || bytes.len() as u64 != width * height * 4
            {
                bail!("Invalid RGBA texture upload");
            }
            texture.replace_region(
                MTLRegion::new_2d(x, y, width, height),
                0,
                bytes.as_ptr().cast(),
                width * 4,
            );
            if !self.updated_textures.contains(&id) {
                self.updated_textures.push(id);
            }
        }
        for upload in a(&update["uploads"]) {
            let id = n(&upload["buffer"]) as usize;
            let buffer = self.buffers.get(id).context("Unknown buffer")?;
            let bytes = base64::engine::general_purpose::STANDARD.decode(s(&upload["data"]))?;
            let offset = n(&upload["offset"]);
            if offset + bytes.len() as u64 > buffer.length() {
                bail!("Buffer upload exceeds allocation");
            }
            unsafe {
                std::ptr::copy_nonoverlapping(
                    bytes.as_ptr(),
                    buffer.contents().cast::<u8>().add(offset as usize),
                    bytes.len(),
                );
            }
        }
        let mut events = vec![];
        let mut index = 0;
        for c in a(&update["commands"]) {
            match s(&c["op"]) {
                "clear" => {
                    let mask = n(&c["mask"]);
                    let color = a(&c["color"]);
                    events.push(Event::Pass {
                        target: n(&c["target"]) as usize,
                        color: if mask & 0x4000 != 0 {
                            Some([f(&color[0]), f(&color[1]), f(&color[2]), f(&color[3])])
                        } else {
                            None
                        },
                        depth: if mask & 0x100 != 0 {
                            Some(f(&c["depth"]))
                        } else {
                            None
                        },
                        draws: vec![],
                    });
                }
                "draw" => {
                    let target = n(&c["target"]) as usize;
                    if target >= self.targets.len() {
                        bail!("Unknown render target");
                    }
                    let key = draw_key(c, &self.targets);
                    let prototype = self.prototypes.get(&key).with_context(|| {
                        format!("Unprepared pipeline: program {}", c["program"])
                    })?;
                    if index >= self.draws.len() {
                        let mut d = prototype.clone();
                        d.vertex = None;
                        d.fragment = None;
                        self.draws.push(d);
                    }
                    let d = &mut self.draws[index];
                    d.program = n(&c["program"]) as usize;
                    d.pipeline = prototype.pipeline.clone();
                    d.depth = prototype.depth.clone();
                    d.attributes = prototype.attributes.clone();
                    for attr in a(&c["attributes"]) {
                        if b(&attr["enabled"]) {
                            let id = n(&attr["buffer"]) as usize;
                            let v = d
                                .attributes
                                .iter_mut()
                                .find(|(slot, _, _)| *slot == n(&attr["location"]))
                                .context("Missing vertex slot")?;
                            v.1 = self
                                .buffers
                                .get(id)
                                .context("Unknown vertex buffer")?
                                .clone();
                            v.2 = n(&attr["offset"]);
                        }
                    }
                    let pid = n(&c["program"]) as usize;
                    let values = uniform_values(&self.frame["programs"][pid], c);
                    for (stage, storage) in [("vert", &mut d.vertex), ("frag", &mut d.fragment)] {
                        let r = &self.programs[pid][stage]["reflection"];
                        let Some(block) = a(&r["ubos"]).first() else {
                            *storage = None;
                            continue;
                        };
                        let mut bytes = vec![0; (n(&block["block_size"]) as usize).max(16)];
                        for m in a(&r["types"][s(&block["type"])]["members"]) {
                            pack_member(r, m, 0, s(&m["name"]), &values, &mut bytes)?;
                        }
                        let slot = n(&block["binding"]);
                        if let Some((old_slot, buffer)) = storage {
                            if *old_slot == slot && buffer.length() >= bytes.len() as u64 {
                                unsafe {
                                    std::ptr::copy_nonoverlapping(
                                        bytes.as_ptr(),
                                        buffer.contents().cast(),
                                        bytes.len(),
                                    );
                                }
                                continue;
                            }
                        }
                        *storage = Some((slot, buf(&self.device, &bytes)));
                    }
                    d.vertex_textures = prototype.vertex_textures.clone();
                    d.fragment_textures = prototype.fragment_textures.clone();
                    for (stage, list) in [
                        ("vert", &mut d.vertex_textures),
                        ("frag", &mut d.fragment_textures),
                    ] {
                        for tex in a(&self.programs[pid][stage]["reflection"]["textures"]) {
                            let name = s(&tex["name"]);
                            let mut samplers = &c["samplers"][name];
                            if samplers.is_null() {
                                samplers = &c["samplers"][format!("{name}[0]")];
                            }
                            for (i, v) in a(samplers).iter().enumerate() {
                                let slot = n(&tex["binding"]) + i as u64;
                                let binding = list
                                    .iter_mut()
                                    .find(|b| b.slot == slot)
                                    .context("Sampler binding")?;
                                binding.texture = n(&v["texture"]) as usize;
                                if binding.texture >= self.textures.len() {
                                    bail!("Unknown native texture");
                                }
                            }
                        }
                    }
                    d.state = c["state"].clone();
                    d.index = c["index"]
                        .as_u64()
                        .map(|id| self.buffers[id as usize].clone());
                    d.index_type = if n(&c["indexType"]) == 0x1405 {
                        MTLIndexType::UInt32
                    } else {
                        MTLIndexType::UInt16
                    };
                    d.first = n(&c["first"]);
                    d.count = n(&c["count"]);
                    d.instances = n(&c["instances"]);
                    if !matches!(events.last(),Some(Event::Pass{target:t,..})if *t==target) {
                        events.push(Event::Pass {
                            target,
                            color: None,
                            depth: None,
                            draws: vec![],
                        });
                    }
                    if let Some(Event::Pass { draws, .. }) = events.last_mut() {
                        draws.push(index);
                    }
                    index += 1;
                }
                "blit" => events.push(Event::Blit {
                    from: n(&c["from"]) as usize,
                    to: n(&c["to"]) as usize,
                    mask: n(&c["args"][8]),
                }),
                "mipmap" => events.push(Event::Mipmap(n(&c["texture"]) as usize)),
                op => bail!("Unsupported dynamic operation {op}"),
            }
        }
        self.events = events;
        Ok(())
    }
    /// Release transient cover snapshots after their final submitted frame has
    /// completed. IDs stay stable, while obsolete GPU allocations are dropped.
    pub fn release_resources(&mut self, update: &Value) -> Result<()> {
        for resource in a(&update["deletions"]) {
            let id = n(&resource["id"]) as usize;
            match s(&resource["kind"]) {
                "texture" => {
                    let placeholder =
                        tex(&self.device, &json!({"width":1,"height":1,"format":0x8058}))?;
                    *self
                        .textures
                        .get_mut(id)
                        .context("Unknown released texture")? = placeholder;
                }
                "buffer" => {
                    let placeholder = buf(&self.device, &[0; 16]);
                    *self
                        .buffers
                        .get_mut(id)
                        .context("Unknown released buffer")? = placeholder;
                }
                _ => bail!("Unknown released resource"),
            }
        }
        Ok(())
    }
    pub fn encode(&self, cb: &CommandBufferRef) {
        if !self.updated_textures.is_empty() {
            let encoder = cb.new_blit_command_encoder();
            for id in &self.updated_textures {
                let texture = &self.textures[*id];
                if texture.mipmap_level_count() > 1 {
                    encoder.generate_mipmaps(texture);
                }
            }
            encoder.end_encoding();
        }
        self.encode_events(cb, &self.events);
    }
    fn encode_events(&self, cb: &CommandBufferRef, events: &[Event]) {
        let fuse_resolve = std::env::var_os("RHINE_METAL_SEPARATE_RESOLVE").is_none();
        let mut skip = None;
        for (event_index, event) in events.iter().enumerate() {
            if skip == Some(event_index) { continue; }
            match event {
                Event::Pass {
                    target,
                    color,
                    depth,
                    draws,
                } => {
                    let target_id = *target;
                    let target = &self.targets[target_id];
                    // Resolve directly from tile memory at the end of the producer
                    // pass. Keep the multisample source because back faces reuse it.
                    // The old path stored it, then immediately loaded it to resolve.
                    let resolve = if fuse_resolve && target.samples > 1 {
                        match events.get(event_index + 1) {
                            Some(Event::Blit {from,to,mask}) if *from == target_id => {
                                skip = Some(event_index + 1);
                                Some((&self.targets[*to], *mask))
                            }
                            _ => None,
                        }
                    } else { None };
                    let desc = RenderPassDescriptor::new();
                    if let Some(t) = &target.color {
                        let c = desc.color_attachments().object_at(0).unwrap();
                        c.set_texture(Some(t));
                        c.set_load_action(if color.is_some() {
                            MTLLoadAction::Clear
                        } else {
                            MTLLoadAction::Load
                        });
                        c.set_store_action(MTLStoreAction::Store);
                        if let Some((destination, mask)) = resolve {
                            if mask & 0x4000 != 0 {
                                c.set_resolve_texture(destination.color.as_deref());
                                c.set_store_action(MTLStoreAction::StoreAndMultisampleResolve);
                            }
                        }
                        if let Some(v) = color {
                            c.set_clear_color(MTLClearColor::new(v[0], v[1], v[2], v[3]));
                        }
                    }
                    if let Some(t) = &target.depth {
                        let d = desc.depth_attachment().unwrap();
                        d.set_texture(Some(t));
                        d.set_load_action(if depth.is_some() {
                            MTLLoadAction::Clear
                        } else {
                            MTLLoadAction::Load
                        });
                        d.set_store_action(MTLStoreAction::Store);
                        if let Some((destination, mask)) = resolve {
                            if mask & 0x100 != 0 {
                                d.set_resolve_texture(destination.depth.as_deref());
                                d.set_store_action(MTLStoreAction::StoreAndMultisampleResolve);
                            }
                        }
                        if let Some(v) = depth {
                            d.set_clear_depth(*v);
                        }
                    }
                    let e = cb.new_render_command_encoder(desc);
                    for index in draws {
                        let d = &self.draws[*index];
                        let st = &d.state;
                        e.set_render_pipeline_state(&d.pipeline);
                        e.set_depth_stencil_state(&d.depth);
                        let v = a(&st["viewport"]);
                        e.set_viewport(MTLViewport {
                            originX: f(&v[0]),
                            originY: f(&v[1]),
                            width: f(&v[2]),
                            height: f(&v[3]),
                            znear: 0.,
                            zfar: 1.,
                        });
                        let sc = a(&st["scissor"]);
                        e.set_scissor_rect(if sc.len() == 4 {
                            MTLScissorRect {
                                x: n(&sc[0]),
                                y: n(&sc[1]),
                                width: n(&sc[2]),
                                height: n(&sc[3]),
                            }
                        } else {
                            MTLScissorRect {
                                x: 0,
                                y: 0,
                                width: target.width,
                                height: target.height,
                            }
                        });
                        e.set_cull_mode(match n(&st["cull"]) {
                            0x404 => MTLCullMode::Front,
                            0x405 => MTLCullMode::Back,
                            _ => MTLCullMode::None,
                        });
                        e.set_front_facing_winding(if n(&st["frontFace"]) == 0x901 {
                            MTLWinding::Clockwise
                        } else {
                            MTLWinding::CounterClockwise
                        });
                        let offset = a(&st["polygonOffset"]);
                        if offset.len() == 2 {
                            e.set_depth_bias(f(&offset[1]) as f32, f(&offset[0]) as f32, 0.);
                        } else {
                            e.set_depth_bias(0., 0., 0.);
                        }
                        for (slot, buffer, offset) in &d.attributes {
                            e.set_vertex_buffer(*slot, Some(buffer), *offset);
                        }
                        if let Some((slot, buffer)) = &d.vertex {
                            e.set_vertex_buffer(*slot, Some(buffer), 0);
                        }
                        if let Some((slot, buffer)) = &d.fragment {
                            e.set_fragment_buffer(*slot, Some(buffer), 0);
                        }
                        for t in &d.vertex_textures {
                            e.set_vertex_texture(t.slot, Some(&self.textures[t.texture]));
                            e.set_vertex_sampler_state(t.slot, Some(&t.sampler));
                        }
                        for t in &d.fragment_textures {
                            e.set_fragment_texture(t.slot, Some(&self.textures[t.texture]));
                            e.set_fragment_sampler_state(t.slot, Some(&t.sampler));
                        }
                        if let Some(index) = &d.index {
                            e.draw_indexed_primitives_instanced(
                                d.primitive,
                                d.count,
                                d.index_type,
                                index,
                                d.first,
                                d.instances,
                            );
                        } else {
                            e.draw_primitives_instanced(d.primitive, d.first, d.count, d.instances);
                        }
                    }
                    e.end_encoding();
                }
                Event::Mipmap(i) => {
                    let e = cb.new_blit_command_encoder();
                    e.generate_mipmaps(&self.textures[*i]);
                    e.end_encoding();
                }
                Event::Blit { from, to, mask } => {
                    let src = &self.targets[*from];
                    let dst = &self.targets[*to];
                    if src.samples > 1 {
                        let desc = RenderPassDescriptor::new();
                        if mask & 0x4000 != 0 {
                            let c = desc.color_attachments().object_at(0).unwrap();
                            c.set_texture(src.color.as_deref());
                            c.set_resolve_texture(dst.color.as_deref());
                            c.set_load_action(MTLLoadAction::Load);
                            c.set_store_action(MTLStoreAction::StoreAndMultisampleResolve);
                        }
                        if mask & 0x100 != 0 {
                            let d = desc.depth_attachment().unwrap();
                            d.set_texture(src.depth.as_deref());
                            d.set_resolve_texture(dst.depth.as_deref());
                            d.set_load_action(MTLLoadAction::Load);
                            d.set_store_action(MTLStoreAction::StoreAndMultisampleResolve);
                        }
                        let e = cb.new_render_command_encoder(desc);
                        e.end_encoding();
                    } else {
                        let e = cb.new_blit_command_encoder();
                        for (bit, a, b) in [
                            (0x4000, &src.color, &dst.color),
                            (0x100, &src.depth, &dst.depth),
                        ] {
                            if mask & bit != 0 {
                                if let (Some(a), Some(b)) = (a, b) {
                                    e.copy_from_texture(
                                        a,
                                        0,
                                        0,
                                        MTLOrigin { x: 0, y: 0, z: 0 },
                                        MTLSize::new(src.width, src.height, 1),
                                        b,
                                        0,
                                        0,
                                        MTLOrigin { x: 0, y: 0, z: 0 },
                                    );
                                }
                            }
                        }
                        e.end_encoding();
                    }
                }
            }
        }
    }
    /// Diagnostic only: split the captured stream at existing encoder boundaries.
    /// This adds command-buffer overhead and is never used by normal playback.
    pub fn profile_events(&self) -> Result<Value> {
        let mut samples = vec![vec![]; self.events.len()];
        for iteration in 0..14 {
            for (index, event) in self.events.iter().enumerate() {
                let cb = self.queue.new_command_buffer();
                self.encode_events(cb, std::slice::from_ref(event));
                cb.commit();
                cb.wait_until_completed();
                if cb.status() != MTLCommandBufferStatus::Completed { bail!("Profile command failed"); }
                let (start,end):(f64,f64)=unsafe{(msg_send![cb,GPUStartTime],msg_send![cb,GPUEndTime])};
                if iteration >= 4 { samples[index].push((end-start)*1000.); }
            }
        }
        Ok(Value::Array(self.events.iter().enumerate().map(|(index,event)| {
            let mut row = match event {
                Event::Pass {target,draws,..} => json!({"op":"pass","target":target,
                    "width":self.targets[*target].width,"height":self.targets[*target].height,
                    "samples":self.targets[*target].samples,"drawIndices":draws,
                    "programs":draws.iter().map(|i|self.draws[*i].program).collect::<Vec<_>>(),
                    "triangles":draws.iter().map(|i|self.draws[*i].count*self.draws[*i].instances/3).sum::<u64>()}),
                Event::Blit {from,to,mask}=>json!({"op":"resolve/copy","from":from,"to":to,"mask":mask}),
                Event::Mipmap(texture)=>json!({"op":"mipmap","texture":texture}),
            };
            row["event"]=json!(index);row["gpuMs"]=stats(samples[index].clone());row
        }).collect()))
    }
    pub fn save_image(&self, path: &Path) -> Result<()> {
        png(path, self.pixels(), self.width, self.height)
    }
    fn pixels(&self) -> Vec<u8> {
        let mut bytes = vec![0; self.width as usize * self.height as usize * 4];
        self.screen.get_bytes(
            bytes.as_mut_ptr().cast(),
            self.width as u64 * 4,
            MTLRegion::new_2d(0, 0, self.width as u64, self.height as u64),
            0,
        );
        bytes
    }
}
fn stats(mut v: Vec<f64>) -> Value {
    v.sort_by(f64::total_cmp);
    let n = v.len();
    json!({"samples":n,"mean":v.iter().sum::<f64>()/n as f64,"p50":v[n/2],"p95":v[(n as f64*0.95) as usize],"p99":v[(n as f64*0.99) as usize],"max":v[n-1]})
}
fn png(path: &Path, bytes: Vec<u8>, width: u32, height: u32) -> Result<()> {
    let mut image = image::RgbaImage::from_raw(width, height, bytes).context("Image bytes")?;
    image::imageops::flip_vertical_in_place(&mut image);
    image.save(path)?;
    Ok(())
}
pub fn main() -> Result<()> {
    autoreleasepool(|| {
        let args: Vec<_> = std::env::args().collect();
        if args.len() < 4 {
            bail!("frame <capture-directory> <translated-directory> <output-directory>");
        }
        let capture = Path::new(&args[1]);
        let out = Path::new(&args[3]);
        fs::create_dir_all(out)?;
        let mut renderer = Renderer::load(capture, Path::new(&args[2]))?;
        if std::env::var_os("RHINE_METAL_PROFILE").is_some() {
            let profile=renderer.profile_events()?;
            fs::write(out.join("pass-profile.json"),serde_json::to_vec_pretty(&profile)?)?;
        }
        if args.iter().any(|v| v == "--window") {
            return frame_window::run(renderer);
        }
        let update = if let Some(path) = std::env::var_os("RHINE_METAL_UPDATE_FILE") {
            serde_json::from_slice::<Value>(&fs::read(path)?)?["frame"].clone()
        } else {
            json!({"commands":renderer.frame["commands"],"uploads":[]})
        };
        let mut gpu = vec![];
        let mut cpu = vec![];
        let mut wall = vec![];
        for i in 0..110 {
            autoreleasepool(|| -> Result<()> {
                let start = Instant::now();
                if std::env::var_os("RHINE_METAL_REPLAY_UPDATES").is_some() {
                    renderer.update_frame(&update)?;
                }
                let cb = renderer.queue.new_command_buffer();
                renderer.encode(cb);
                let encode = start.elapsed().as_secs_f64() * 1000.;
                cb.commit();
                cb.wait_until_completed();
                if cb.status() != MTLCommandBufferStatus::Completed {
                    bail!("GPU failed {:?}", cb.status());
                }
                if i >= 10 {
                    let (a, b): (f64, f64) =
                        unsafe { (msg_send![cb, GPUStartTime], msg_send![cb, GPUEndTime]) };
                    gpu.push((b - a) * 1000.);
                    cpu.push(encode);
                    wall.push(start.elapsed().as_secs_f64() * 1000.);
                }
                Ok(())
            })?;
        }
        let pixels = renderer.pixels();
        let mut blob = vec![];
        for file in a(&renderer.frame["files"]) {
            blob.extend(fs::read(capture.join(s(file)))?);
        }
        let reference = data(&blob, &renderer.frame["reference"]);
        let diffs: Vec<_> = pixels
            .iter()
            .zip(reference)
            .map(|(&a, &b)| a.abs_diff(b) as f64)
            .collect();
        let max = diffs.iter().copied().fold(0., f64::max);
        let mean = diffs.iter().sum::<f64>() / diffs.len() as f64;
        let mse = diffs.iter().map(|v| v * v).sum::<f64>() / diffs.len() as f64;
        png(
            &out.join("native-metal.png"),
            pixels,
            renderer.width,
            renderer.height,
        )?;
        png(
            &out.join("webgl-reference.png"),
            reference.to_vec(),
            renderer.width,
            renderer.height,
        )?;
        let report = json!({"capture":renderer.frame["id"],"device":renderer.device.name(),"width":renderer.width,"height":renderer.height,"gpuMs":stats(gpu),"cpuEncodeMs":stats(cpu),"submitToCompletionMs":stats(wall),"image":{"maxChannelDifference":max,"meanChannelDifference":mean,"fractionAbove2":diffs.iter().filter(|&&d|d>2.).count() as f64/diffs.len() as f64,"psnrDb":10.*(255.*255./mse).log10()},"dynamicReplay":std::env::var_os("RHINE_METAL_REPLAY_UPDATES").is_some(),"fastMath":std::env::var_os("RHINE_METAL_FAST_MATH").is_some(),"draws":renderer.draws.len(),"passesAndOperations":renderer.events.len(),"note":"Complete captured GL command stream reconstructed in native Metal, original translated GLSL. Static frame, one in flight, no UI/presentation or live scene updates in these timings."});
        fs::write(
            out.join("results.json"),
            serde_json::to_vec_pretty(&report)?,
        )?;
        println!("{}", serde_json::to_string_pretty(&report)?);
        Ok(())
    })
}
