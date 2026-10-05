use super::*;
struct Accel {
    object: AccelerationStructure,
    scratch: Buffer,
    desc: AccelerationStructureDescriptor,
}
fn build(l: &Lab, desc: AccelerationStructureDescriptor) -> Result<Accel> {
    let sizes = l.d.acceleration_structure_sizes_with_descriptor(&desc);
    let a =
        l.d.new_acceleration_structure_with_size(sizes.acceleration_structure_size);
    let scratch = l.d.new_buffer(
        sizes
            .build_scratch_buffer_size
            .max(sizes.refit_scratch_buffer_size),
        MTLResourceOptions::StorageModePrivate,
    );
    let cb = l.q.new_command_buffer();
    let e = cb.new_acceleration_structure_command_encoder();
    e.build_acceleration_structure(&a, &desc, &scratch, 0);
    e.end_encoding();
    cb.commit();
    cb.wait_until_completed();
    if cb.status() != MTLCommandBufferStatus::Completed {
        bail!("AS build failed");
    }
    Ok(Accel {
        object: a,
        scratch,
        desc,
    })
}
pub(super) fn run(l: &Lab, c: &Capture, out: &Path) -> Result<Value> {
    if !l.d.supports_raytracing() {
        return Ok(json!({"supported":false}));
    }
    let begin = Instant::now();
    let mut vb = vec![];
    let mut ib = vec![];
    let mut blas = vec![];
    for g in &c.geometries {
        let v = buffer(&l.d, &g.positions);
        let i = buffer(&l.d, &g.indices);
        let gd = AccelerationStructureTriangleGeometryDescriptor::descriptor();
        gd.set_vertex_buffer(Some(&v));
        gd.set_vertex_stride(12);
        gd.set_vertex_format(MTLAttributeFormat::Float3);
        gd.set_index_buffer(Some(&i));
        gd.set_index_type(MTLIndexType::UInt32);
        gd.set_triangle_count(g.indices.len() as u64 / 3);
        gd.set_opaque(true);
        let pd = PrimitiveAccelerationStructureDescriptor::descriptor();
        pd.set_geometry_descriptors(&Array::from_owned_slice(&[gd.into()]));
        blas.push(build(l, pd.into())?);
        vb.push(v);
        ib.push(i);
    }
    let base: Vec<_> = c
        .instances
        .iter()
        .map(|i| {
            let m = i.matrix;
            MTLAccelerationStructureInstanceDescriptor {
                transformation_matrix: [
                    [m[0], m[1], m[2]],
                    [m[4], m[5], m[6]],
                    [m[8], m[9], m[10]],
                    [m[12], m[13], m[14]],
                ],
                options: MTLAccelerationStructureInstanceOptions::Opaque
                    | MTLAccelerationStructureInstanceOptions::DisableTriangleCulling,
                mask: 0xff,
                intersection_function_table_offset: 0,
                acceleration_structure_index: i.geometry as u32,
            }
        })
        .collect();
    let instance_buffer = buffer(&l.d, &base);
    let td = InstanceAccelerationStructureDescriptor::descriptor();
    td.set_instance_count(base.len() as u64);
    td.set_instance_descriptor_buffer(&instance_buffer);
    td.set_instanced_acceleration_structures(&Array::from_owned_slice(
        &blas.iter().map(|b| b.object.clone()).collect::<Vec<_>>(),
    ));
    // MTLAccelerationStructureUsageRefit = 1; metal-rs 0.31 lacks this setter.
    unsafe {
        let _: () = msg_send![&*td,setUsage:1u64];
    }
    let tlas = build(l, td.into())?;
    let build_ms = begin.elapsed().as_secs_f64() * 1000.;
    let camera = Camera {
        world: c.camera_world,
        inverse: c.inverse_projection,
        vp: (Mat4::from_cols_array(&c.projection)
            * Mat4::from_cols_array(&c.camera_world).inverse())
        .to_cols_array(),
        width: c.width,
        height: c.height,
        secondary: 0,
        pad: 0,
    };
    let targets = [
        texture(&l.d, c.width, c.height, MTLPixelFormat::RGBA32Float),
        texture(&l.d, c.width, c.height, MTLPixelFormat::RGBA32Float),
    ];
    let depth_desc = TextureDescriptor::new();
    depth_desc.set_width(c.width as u64);
    depth_desc.set_height(c.height as u64);
    depth_desc.set_pixel_format(MTLPixelFormat::Depth32Float);
    depth_desc.set_usage(MTLTextureUsage::RenderTarget);
    depth_desc.set_storage_mode(MTLStorageMode::Private);
    let z = l.d.new_texture(&depth_desc);
    let rp = RenderPipelineDescriptor::new();
    rp.set_vertex_function(Some(
        l.lib
            .get_function("primaryVertex", None)
            .map_err(anyhow::Error::msg)?
            .as_ref(),
    ));
    rp.set_fragment_function(Some(
        l.lib
            .get_function("primaryFragment", None)
            .map_err(anyhow::Error::msg)?
            .as_ref(),
    ));
    rp.color_attachments()
        .object_at(0)
        .unwrap()
        .set_pixel_format(MTLPixelFormat::RGBA32Float);
    rp.set_depth_attachment_pixel_format(MTLPixelFormat::Depth32Float);
    let raster =
        l.d.new_render_pipeline_state(&rp)
            .map_err(anyhow::Error::msg)?;
    let dd = DepthStencilDescriptor::new();
    dd.set_depth_compare_function(MTLCompareFunction::LessEqual);
    dd.set_depth_write_enabled(true);
    let ds = l.d.new_depth_stencil_state(&dd);
    let groups: Vec<Vec<Model>> = (0..c.geometries.len())
        .map(|g| {
            c.instances
                .iter()
                .enumerate()
                .filter(|(_, i)| i.geometry == g)
                .map(|(n, i)| Model {
                    matrix: i.matrix,
                    id: n as u32,
                    geometry: g as u32,
                    pad: [0; 2],
                })
                .collect()
        })
        .collect();
    let model_buffers: Vec<_> = groups.iter().map(|v| buffer(&l.d, v)).collect();
    let raster_time = bench(l, 100, |cb, _| {
        let p = RenderPassDescriptor::new();
        let color = p.color_attachments().object_at(0).unwrap();
        color.set_texture(Some(&targets[0]));
        color.set_load_action(MTLLoadAction::Clear);
        color.set_store_action(MTLStoreAction::Store);
        color.set_clear_color(MTLClearColor::new(0., 1., 0., 1.));
        let depth = p.depth_attachment().unwrap();
        depth.set_texture(Some(&z));
        depth.set_clear_depth(1.);
        depth.set_load_action(MTLLoadAction::Clear);
        depth.set_store_action(MTLStoreAction::DontCare);
        let e = cb.new_render_command_encoder(p);
        e.set_render_pipeline_state(&raster);
        e.set_depth_stencil_state(&ds);
        e.set_cull_mode(MTLCullMode::None);
        e.set_vertex_bytes(
            2,
            size_of::<Camera>() as u64,
            &camera as *const _ as *const c_void,
        );
        for g in 0..c.geometries.len() {
            e.set_vertex_buffer(0, Some(&vb[g]), 0);
            e.set_vertex_buffer(1, Some(&model_buffers[g]), 0);
            e.draw_indexed_primitives_instanced(
                MTLPrimitiveType::Triangle,
                c.geometries[g].indices.len() as u64,
                MTLIndexType::UInt32,
                &ib[g],
                0,
                groups[g].len() as u64,
            );
        }
        e.end_encoding();
    })?;
    let ray_pipeline = pipeline(l, "primaryRays")?;
    let encode_ray = |cb: &CommandBufferRef, camera: &Camera| {
        let e = cb.new_compute_command_encoder();
        e.set_compute_pipeline_state(&ray_pipeline);
        e.set_acceleration_structure(0, Some(&tlas.object));
        e.set_bytes(
            1,
            size_of::<Camera>() as u64,
            camera as *const _ as *const c_void,
        );
        e.set_texture(0, Some(&targets[1]));
        for b in &blas {
            e.use_resource(&b.object, MTLResourceUsage::Read);
        }
        e.dispatch_threads(
            MTLSize::new(c.width as u64, c.height as u64, 1),
            MTLSize::new(8, 8, 1),
        );
        e.end_encoding();
    };
    let primary = bench(l, 100, |cb, _| encode_ray(cb, &camera))?;
    let ras = read(&targets[0], 16);
    let ray = read(&targets[1], 16);
    let float_at = |b: &[u8], i: usize| f32::from_le_bytes(b[i..i + 4].try_into().unwrap());
    let mut ids = 0;
    let mut bad_depth = 0;
    let mut hit = 0;
    let mut miss = 0;
    let mut max_depth = 0f32;
    let mut ras_png = vec![];
    let mut ray_png = vec![];
    for i in (0..ras.len()).step_by(16) {
        let a = float_at(&ras, i);
        let b = float_at(&ray, i);
        let ad = float_at(&ras, i + 4);
        let bd = float_at(&ray, i + 4);
        if a != b {
            ids += 1;
        }
        if (a > 0.) != (b > 0.) {
            miss += 1;
        }
        if a > 0. || b > 0. {
            hit += 1;
            let d = (ad - bd).abs();
            max_depth = max_depth.max(d);
            if d > 0.00001 {
                bad_depth += 1;
            }
        }
        for (id, p) in [(a, &mut ras_png), (b, &mut ray_png)] {
            if id == 0. {
                p.extend([234, 229, 225, 255]);
            } else {
                let g = c.instances[id as usize - 1].geometry as u8;
                p.extend([
                    70 + g.wrapping_mul(47) % 160,
                    80 + g.wrapping_mul(71) % 150,
                    90 + g.wrapping_mul(31) % 140,
                    255,
                ]);
            }
        }
    }
    save_png(
        &out.join("raster-visibility.png"),
        &ras_png,
        c.width,
        c.height,
    )?;
    save_png(&out.join("ray-visibility.png"), &ray_png, c.width, c.height)?;
    let mut secondary = serde_json::Map::new();
    for count in [1, 4, 16] {
        let mut cam = camera;
        cam.secondary = count;
        secondary.insert(
            count.to_string(),
            bench(l, 60, |cb, _| encode_ray(cb, &cam))?,
        );
    }
    // Synthetic 8 Hz motion deliberately changes every instance. It does not claim
    // to be a replay of user interaction or a measurement of input-to-photon latency.
    let mut moved = base.clone();
    let dynamic = bench(l, 120, |cb, frame| {
        for (j, m) in moved.iter_mut().enumerate() {
            m.transformation_matrix = base[j].transformation_matrix;
            m.transformation_matrix[3][1] +=
                ((frame as f32 / 60. * 8. * std::f32::consts::TAU) + (j as f32 * 0.37)).sin()
                    * 0.12;
        }
        unsafe {
            std::ptr::copy_nonoverlapping(
                moved.as_ptr(),
                instance_buffer.contents() as *mut MTLAccelerationStructureInstanceDescriptor,
                moved.len(),
            );
        }
        let e = cb.new_acceleration_structure_command_encoder();
        e.refit_acceleration_structure(&tlas.object, &tlas.desc, None, &tlas.scratch, 0);
        e.end_encoding();
        encode_ray(cb, &camera);
    })?;
    // Validate the last refitted pose against a fresh TLAS build using the same
    // matrices. This detects stale bounds or an incorrect refit usage flag.
    let refitted = read(&targets[1], 16);
    let cb = l.q.new_command_buffer();
    let e = cb.new_acceleration_structure_command_encoder();
    e.build_acceleration_structure(&tlas.object, &tlas.desc, &tlas.scratch, 0);
    e.end_encoding();
    encode_ray(cb, &camera);
    cb.commit();
    cb.wait_until_completed();
    if cb.status() != MTLCommandBufferStatus::Completed {
        bail!("Refit validation rebuild failed");
    }
    let rebuilt = read(&targets[1], 16);
    let refit_different_values = refitted
        .chunks_exact(4)
        .zip(rebuilt.chunks_exact(4))
        .filter(|(a, b)| a != b)
        .count();
    let mut refit_max_depth = 0f32;
    let mut refit_id_changes = 0;
    for i in (0..refitted.len()).step_by(16) {
        let a = float_at(&refitted, i);
        let b = float_at(&rebuilt, i);
        if (a > 0.) != (b > 0.) {
            bail!("Refit hit mask differs from rebuilt TLAS");
        }
        if a != b {
            refit_id_changes += 1;
        }
        refit_max_depth =
            refit_max_depth.max((float_at(&refitted, i + 4) - float_at(&rebuilt, i + 4)).abs());
    }
    // BVH construction order can select a different coincident triangle at an
    // equal-depth tie. Require the same coverage and depth, report ID differences.
    if refit_max_depth > 0.00001 {
        bail!("Refit depth differs from rebuild by {refit_max_depth}");
    }
    let mem =
        blas.iter().map(|a| a.object.allocated_size()).sum::<u64>() + tlas.object.allocated_size();
    Ok(
        json!({"supported":true,"refitVersusRebuild":{"differentFloatValues":refit_different_values,"differentInstanceId":refit_id_changes,"maxDepthDifference":refit_max_depth,"sameHitMask":true},"uniqueMeshes":c.geometries.len(),"uniqueTriangles":c.geometries.iter().map(|g|g.indices.len()/3).sum::<usize>(),"instances":base.len(),"instancedTriangles":c.instances.iter().map(|i|c.geometries[i.geometry].indices.len()/3).sum::<usize>(),"accelerationStructureBytes":mem,"initialBuildWallMs":build_ms,"rasterVisibility":raster_time,"rayPrimary":primary,"rayPrimaryPlusSecondary":secondary,"synthetic8HzAllInstanceRefitAndPrimary":dynamic,"visibilityParity":{"pixels":c.width*c.height,"coveredPixels":hit,"differentInstanceId":ids,"differentHitMask":miss,"depthDifferenceAbove1eMinus5":bad_depth,"maxDepthDifference":max_depth},"scope":"Same exported geometry/transforms/camera, opaque double-sided first-hit visibility only. Extra rays are diagnostic opaque occlusion, not original glass/shadows/AO. Includes real GPU AS refit in synthetic all-instance 8 Hz motion, but not UI/input-to-photon. No path tracing, denoising, material shading or display."}),
    )
}
