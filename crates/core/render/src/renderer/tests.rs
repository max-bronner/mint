use super::*;

#[test]
fn the_uniform_block_is_exactly_what_the_shader_declares() {
    let uniforms = Uniforms {
        model: Mat4::IDENTITY.to_cols(),
        normal_matrix: Mat4::IDENTITY.to_cols(),
        tint: [1.0; 4],
        params: [0.0; 4],
        uv_linear: [1.0, 0.0, 0.0, 1.0],
        uv_offset: [0.0; 4],
        material_ambient: [1.0, 1.0, 1.0, 0.0],
        material_diffuse: [1.0, 1.0, 1.0, 0.0],
        material_emissive: [0.0; 4],
        uv2_linear: [1.0, 0.0, 0.0, 1.0],
        uv2_offset: [0.0; 4],
        detail_combine: [0.0; 4],
        house_color: [1.0; 4],
        house_skin: [0.0; 4],
        house_paint: [0.0; 4],
        env_map: [0.0; 4],
        color_source: [0.0; 4],
        surface_flags: [0.0; 4],
        height_shear: [0.0; 4],
        light_add: [0.0; 4],
    };
    assert_eq!(
        uniforms.to_bytes().len() as u64,
        UNIFORM_SIZE,
        "instance_block.wgsl's `Uniforms` declares two mat4x4 and eighteen vec4: \
             32 + 72 floats"
    );
    assert_eq!(uniforms.floats().1, UNIFORM_FLOATS);
    assert_eq!(UNIFORM_FLOATS as u64 * 4, UNIFORM_SIZE);
    let frame = FrameUniforms {
        view_proj: Mat4::IDENTITY.to_cols(),
        view: Mat4::IDENTITY.to_cols(),
        camera_eye: [0.0; 4],
        light_directions: [[0.0; 4]; MAX_DIRECTIONAL_LIGHTS],
        light_colors: [[0.0; 4]; MAX_DIRECTIONAL_LIGHTS],
        ambient_color: [0.0; 4],
        depth_params: [0.0; 4],
        clip_plane: [0.0; 4],
        shadow_cascades: [Mat4::IDENTITY.to_cols(); MAX_SHADOW_CASCADES],
        shadow_params: [0.0; 4],
        shadow_filter: [0.0; 4],
        shadow_cascade_params: [[0.0; 4]; MAX_SHADOW_CASCADES],
        set_directions: [[0.0; 4]; SET_LIGHTS],
        set_colors: [[0.0; 4]; SET_LIGHTS],
        set_ambient: [[0.0; 4]; LIGHT_SET_COUNT],
        cookie: [0.0; 4],
        cookie_tint: [0.0; 4],
        inv_view_proj: Mat4::IDENTITY.to_cols(),
        contact_shadow: [0.0; 4],
        fog: [[0.0; 4]; 2],
        visibility_mask: [[0.0; 4]; 2],
    };
    assert_eq!(
        frame.to_bytes().len() as u64,
        FRAME_UNIFORM_SIZE,
        "`Frame` declares the matrices, light and shadow arrays, the light sets' two \
             twelve-element and one four-element vec4 arrays and the trailing vec4s: \
             316 floats"
    );
    assert_eq!(frame.floats().1, FRAME_UNIFORM_FLOATS);
    assert_eq!(FRAME_UNIFORM_FLOATS as u64 * 4, FRAME_UNIFORM_SIZE);
    assert_eq!(
        UNIFORM_STRIDE, UNIFORM_SIZE,
        "the shader's `array<Uniforms>` stride is the struct size rounded up to \
             its own 16-byte alignment, and nothing may be written between blocks"
    );
    assert_eq!(
        UNIFORM_STRIDE % 16,
        0,
        "a vec4 member forces 16-byte alignment on the struct, and so on the stride"
    );
}

#[test]
fn the_vertex_attributes_tile_the_vertex_exactly() {
    let mut expected = 0u64;
    for attribute in &VERTEX_ATTRIBUTES {
        assert_eq!(
            attribute.offset, expected,
            "attribute {} is not tightly packed",
            attribute.shader_location
        );
        expected += attribute.format.size();
    }
    assert_eq!(
        expected,
        Vertex::SIZE as u64,
        "the attributes must cover the whole stride"
    );
}

#[test]
fn shader_locations_are_consecutive_from_zero() {
    let locations: Vec<u32> = VERTEX_ATTRIBUTES
        .iter()
        .map(|a| a.shader_location)
        .collect();
    assert_eq!(locations, vec![0, 1, 2, 3, 4, 5]);
}

#[test]
fn the_geometry_vertex_attributes_tile_the_vertex_exactly() {
    let mut expected = 0u64;
    for attribute in &GEOMETRY_VERTEX_ATTRIBUTES {
        assert_eq!(
            attribute.offset, expected,
            "attribute {} is not tightly packed",
            attribute.shader_location
        );
        expected += attribute.format.size();
    }
    assert_eq!(
        expected, GEOMETRY_VERTEX_SIZE,
        "the attributes must cover the whole stride"
    );
}

#[test]
fn geometry_shader_locations_are_consecutive_from_zero() {
    let locations: Vec<u32> = GEOMETRY_VERTEX_ATTRIBUTES
        .iter()
        .map(|a| a.shader_location)
        .collect();
    assert_eq!(locations, vec![0, 1, 2, 3]);
}

#[test]
fn alignment_rounds_up_and_refuses_to_wrap() {
    assert_eq!(align_up(0, 256), Some(0));
    assert_eq!(align_up(1, 256), Some(256));
    assert_eq!(align_up(256, 256), Some(256));
    assert_eq!(align_up(257, 256), Some(512));
    assert_eq!(align_up(usize::MAX, 256), None);
    assert_eq!(align_up(4, 0), None);
}

#[test]
fn a_zero_sized_surface_is_clamped_rather_than_rejected() {
    assert_eq!(clamp_size(0, 0), (1, 1));
    assert_eq!(clamp_size(800, 0), (800, 1));
    assert_eq!(clamp_size(800, 600), (800, 600));
}

#[test]
fn the_aspect_ratio_of_a_degenerate_surface_is_finite() {
    assert!((f64_ratio(1920, 1080) - 16.0 / 9.0).abs() < 1e-6);
    assert_eq!(f64_ratio(100, 0), 1.0);
}

fn instance_at(x: f32, blend: Blend) -> Instance {
    Instance {
        transform: Mat4::from_translation(Vec3::new(x, 0.0, 0.0)),
        blend,
        ..Instance::new(MeshHandle(0))
    }
}

#[test]
fn opaque_instances_draw_first_in_submission_order_unsorted() {
    let instances = [
        instance_at(10.0, Blend::Opaque),
        instance_at(1.0, Blend::Opaque),
    ];
    let eye = Vec3::new(0.0, 0.0, 0.0);
    assert_eq!(sorted_draw_order(&instances, eye, |_| true), vec![0, 1]);
}

#[test]
fn blended_instances_draw_back_to_front() {
    let instances = [
        instance_at(1.0, Blend::AlphaBlend),
        instance_at(10.0, Blend::AlphaBlend),
        instance_at(5.0, Blend::Additive),
    ];
    let eye = Vec3::new(0.0, 0.0, 0.0);
    assert_eq!(sorted_draw_order(&instances, eye, |_| true), vec![1, 2, 0]);
}

#[test]
fn opaque_always_precedes_every_blended_instance_regardless_of_distance() {
    let instances = [
        instance_at(1.0, Blend::AlphaBlend),
        instance_at(100.0, Blend::Opaque),
    ];
    let eye = Vec3::new(0.0, 0.0, 0.0);
    assert_eq!(sorted_draw_order(&instances, eye, |_| true), vec![1, 0]);
}

#[test]
fn a_mix_of_all_three_blend_modes_sorts_correctly() {
    let instances = [
        instance_at(50.0, Blend::Opaque),
        instance_at(2.0, Blend::AlphaBlend),
        instance_at(1.0, Blend::Opaque),
        instance_at(20.0, Blend::Additive),
    ];
    let eye = Vec3::ZERO;
    assert_eq!(
        sorted_draw_order(&instances, eye, |_| true),
        vec![0, 2, 3, 1]
    );
}

#[test]
fn world_space_geometry_sorts_by_where_its_bounds_are_not_by_its_origin() {
    let world_space = Instance {
        blend: Blend::AlphaBlend,
        bounds: Some(Aabb::from_corners(
            Vec3::new(94.0, -1.0, -1.0),
            Vec3::new(96.0, 1.0, 1.0),
        )),
        ..Instance::new(MeshHandle(0))
    };
    let instances = [world_space, instance_at(90.0, Blend::AlphaBlend)];
    assert_eq!(
        sorted_draw_order(&instances, Vec3::new(100.0, 0.0, 0.0), |_| true),
        vec![1, 0]
    );
}

#[test]
fn an_empty_scene_sorts_to_nothing() {
    assert!(sorted_draw_order(&[], Vec3::ZERO, |_| true).is_empty());
}

#[test]
fn sorting_only_the_survivors_gives_the_order_filtering_the_full_sort_would() {
    let blends = [
        Blend::Opaque,
        Blend::AlphaBlend,
        Blend::Additive,
        Blend::AlphaBlendWritesDepth,
        Blend::MergeMask,
        Blend::MergeAlphaBlend,
        Blend::Multiply,
    ];
    let mut seed = 0x2545_f491_u32;
    let mut next = || {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        seed
    };
    let instances: Vec<Instance> = (0..400)
        .map(|_| {
            let r = next();
            Instance {
                blend: blends[(r % 7) as usize],
                sort_level: (r % 5 == 0).then_some((r % 3) as u8),
                surface_overlay: r % 11 == 0,
                visible: r % 13 != 0,
                ..instance_at((r % 9) as f32, Blend::Opaque)
            }
        })
        .collect();
    let keep = |instance: &Instance| instance.visible && instance.transform.to_cols()[3][0] < 6.0;
    let full: Vec<usize> = sorted_draw_order(&instances, Vec3::ZERO, |_| true)
        .into_iter()
        .filter(|index| keep(&instances[*index]))
        .collect();
    assert!(
        full.len() > 100 && full.len() < 350,
        "a real filter: {}",
        full.len()
    );
    assert_eq!(sorted_draw_order(&instances, Vec3::ZERO, keep), full);
}

#[test]
fn a_non_finite_transform_does_not_panic_the_sort() {
    let instances = [
        Instance {
            transform: Mat4::from_scale(Vec3::new(f32::NAN, 0.0, 0.0)),
            blend: Blend::AlphaBlend,
            ..Instance::new(MeshHandle(0))
        },
        instance_at(3.0, Blend::AlphaBlend),
    ];
    let order = sorted_draw_order(&instances, Vec3::ZERO, |_| true);
    assert_eq!(order.len(), 2);
}

fn batches_of(instances: &[Instance], materials: &[MaterialSlot]) -> Vec<Batch> {
    let order = sorted_draw_order(instances, Vec3::ZERO, |instance| instance.visible);
    batch_draws(instances, &order, materials, None, BatchScope::Surface)
}

#[test]
fn instances_agreeing_on_everything_become_one_call() {
    let instances = [
        instance_at(1.0, Blend::Opaque),
        instance_at(2.0, Blend::Opaque),
        instance_at(3.0, Blend::Opaque),
    ];
    let batches = batches_of(&instances, &[0; 3]);
    assert_eq!(batches.len(), 1);
    assert_eq!(batches[0].first, 0);
    assert_eq!(batches[0].count, 3);
    assert_eq!(batches[0].lead, 0);
}

#[test]
fn a_different_material_splits_a_batch_between_identical_meshes() {
    let instances = [
        instance_at(1.0, Blend::Opaque),
        instance_at(2.0, Blend::Opaque),
        instance_at(3.0, Blend::Opaque),
    ];
    let materials = [0, 1, 0];
    let batches = batches_of(&instances, &materials);
    assert_eq!(batches.len(), 3);
    assert_eq!(
        batches.iter().map(|b| b.count).collect::<Vec<_>>(),
        vec![1, 1, 1]
    );
}

#[test]
fn the_opaque_and_blended_halves_never_merge_across_the_split() {
    let instances = [
        instance_at(1.0, Blend::Opaque),
        instance_at(5.0, Blend::AlphaBlend),
        instance_at(2.0, Blend::Opaque),
        instance_at(9.0, Blend::AlphaBlend),
    ];
    let batches = batches_of(&instances, &[0; 4]);
    assert_eq!(batches.len(), 2, "one opaque batch, one blended batch");
    assert_eq!((batches[0].first, batches[0].count), (0, 2));
    assert_eq!(instances[batches[0].lead].blend, Blend::Opaque);
    assert_eq!((batches[1].first, batches[1].count), (2, 2));
    assert_eq!(instances[batches[1].lead].blend, Blend::AlphaBlend);
}

#[test]
fn a_merge_mode_surface_overlay_lands_in_the_blended_half() {
    let overlay = Instance {
        surface_overlay: true,
        ..instance_at(5.0, Blend::MergeAlphaBlend)
    };
    let instances = [instance_at(1.0, Blend::Opaque), overlay];
    let batches = batches_of(&instances, &[0; 2]);
    assert_eq!(batches.len(), 2, "one opaque batch, one merge batch");
    assert_eq!(instances[batches[0].lead].blend, Blend::Opaque);
    assert!(!is_opaque_lead(&instances, &batches[1]));
}

#[test]
fn a_depth_test_difference_splits_a_batch() {
    let instances = [
        instance_at(1.0, Blend::Opaque),
        Instance {
            depth_test: DepthTest::NearerOrEqual,
            ..instance_at(2.0, Blend::Opaque)
        },
    ];
    let batches = batches_of(&instances, &[0; 2]);
    assert_eq!(batches.len(), 2);
}

#[test]
fn a_custom_material_difference_splits_a_batch() {
    let mut instances = [
        instance_at(1.0, Blend::Opaque),
        instance_at(2.0, Blend::Opaque),
        instance_at(3.0, Blend::Opaque),
    ];
    instances[0].custom_material = Some(CustomMaterialHandle(0));
    instances[1].custom_material = Some(CustomMaterialHandle(1));
    instances[2].custom_material = Some(CustomMaterialHandle(1));
    let batches = batches_of(&instances, &[0; 3]);
    assert_eq!(batches.len(), 2);
    assert_eq!(batches[0].count, 1);
    assert_eq!(batches[1].count, 2);
}

#[test]
fn an_instance_with_no_custom_material_never_merges_with_one_that_has_it() {
    let mut instances = [
        instance_at(1.0, Blend::Opaque),
        instance_at(2.0, Blend::Opaque),
    ];
    instances[1].custom_material = Some(CustomMaterialHandle(0));
    let batches = batches_of(&instances, &[0; 2]);
    assert_eq!(batches.len(), 2);
}

#[test]
fn a_hidden_instance_between_two_others_lets_them_merge() {
    let instances = [
        instance_at(1.0, Blend::Opaque),
        Instance {
            visible: false,
            ..instance_at(2.0, Blend::Opaque)
        },
        instance_at(3.0, Blend::Opaque),
    ];
    let batches = batches_of(&instances, &[0; 3]);
    assert_eq!(batches.len(), 1);
    assert_eq!(batches[0].count, 2);
}

#[test]
fn an_empty_draw_order_produces_no_calls() {
    assert!(batch_draws(&[], &[], &[], None, BatchScope::Surface).is_empty());
}

#[test]
fn the_depth_pass_batches_across_everything_a_colour_pass_splits_on() {
    let mut instances = [
        instance_at(1.0, Blend::Opaque),
        instance_at(2.0, Blend::Opaque),
        instance_at(3.0, Blend::Opaque),
        instance_at(4.0, Blend::Opaque),
    ];
    instances[1].custom_material = Some(CustomMaterialHandle(7));
    instances[2].custom_shader = Some(CustomShaderHandle(3));
    instances[3].depth_test = DepthTest::NearerOrEqual;
    let order: Vec<usize> = (0..instances.len()).collect();

    let depth = batch_draws(&instances, &order, &[], None, BatchScope::MeshOnly);
    assert_eq!(depth.len(), 1, "one mesh, one cull mode, one draw call");
    assert_eq!(depth[0].count, 4);
    assert_eq!(depth[0].first, 0);
    assert_eq!(depth[0].lead, 0);
    assert_eq!(depth[0].material, 0, "a depth pass binds no material");

    let surface = batch_draws(&instances, &order, &[0, 1, 2, 3], None, BatchScope::Surface);
    assert_eq!(
        surface.len(),
        4,
        "the same four instances differ in everything a colour pass binds"
    );
}

#[test]
fn the_depth_pass_still_splits_on_the_cull_flag() {
    let instances = [
        Instance {
            cull: true,
            ..instance_at(1.0, Blend::Opaque)
        },
        Instance {
            cull: false,
            ..instance_at(2.0, Blend::Opaque)
        },
        Instance {
            cull: true,
            ..instance_at(3.0, Blend::Opaque)
        },
    ];
    let order: Vec<usize> = (0..instances.len()).collect();
    let batches = batch_draws(&instances, &order, &[], None, BatchScope::MeshOnly);
    assert_eq!(
        batches.iter().map(|b| b.count).collect::<Vec<_>>(),
        vec![1, 1, 1]
    );

    let forced = batch_draws(&instances, &order, &[], Some(true), BatchScope::MeshOnly);
    assert_eq!(forced.len(), 1);
    assert_eq!(forced[0].count, 3);
}

#[test]
fn the_depth_pass_never_merges_two_meshes() {
    let instances = [
        instance_at(1.0, Blend::Opaque),
        Instance {
            ..Instance::new(MeshHandle(1))
        },
        instance_at(3.0, Blend::Opaque),
    ];
    let order: Vec<usize> = (0..instances.len()).collect();
    let batches = batch_draws(&instances, &order, &[], None, BatchScope::MeshOnly);
    assert_eq!(batches.len(), 3, "nothing here is adjacent in mesh order");
}

#[test]
fn uniform_bytes_are_column_major_little_endian() {
    let translated = Mat4::from_translation(Vec3::new(1.0, 2.0, 3.0));
    let uniforms = Uniforms {
        model: translated.to_cols(),
        normal_matrix: Mat4::IDENTITY.to_cols(),
        tint: [1.0; 4],
        params: [0.0; 4],
        uv_linear: [1.0, 0.0, 0.0, 1.0],
        uv_offset: [0.0; 4],
        material_ambient: [1.0, 1.0, 1.0, 0.0],
        material_diffuse: [1.0, 1.0, 1.0, 0.0],
        material_emissive: [0.0; 4],
        uv2_linear: [1.0, 0.0, 0.0, 1.0],
        uv2_offset: [0.0; 4],
        detail_combine: [0.0; 4],
        house_color: [1.0; 4],
        house_skin: [0.0; 4],
        house_paint: [0.0; 4],
        env_map: [0.0; 4],
        color_source: [0.0; 4],
        surface_flags: [0.0; 4],
        height_shear: [0.0; 4],
        light_add: [0.0; 4],
    };
    let bytes = uniforms.to_bytes();
    assert_eq!(&bytes[48..52], &1.0f32.to_le_bytes());
    assert_eq!(&bytes[52..56], &2.0f32.to_le_bytes());
    assert_eq!(&bytes[56..60], &3.0f32.to_le_bytes());
}

#[test]
fn a_second_stage_with_no_texture_combines_as_disable() {
    let asked = Instance {
        detail_combine: DetailCombine::Scale,
        detail_alpha_combine: DetailAlphaCombine::Detail,
        texture2: None,
        ..Instance::new(MeshHandle(0))
    };
    assert_eq!(
        effective_detail_combine(&asked),
        (DetailCombine::Disable, DetailAlphaCombine::Disable)
    );
}

#[test]
fn every_pipeline_is_read_back_from_the_position_its_key_was_built_at() {
    for (cull_mode, skinned, blends) in [
        (Some(wgpu::Face::Back), false, &GRID_BLENDS[..]),
        (None, true, &GRID_BLENDS[..]),
        (None, false, &MERGE_BLENDS[..]),
    ] {
        let keys: Vec<ColorKey> = grid_keys(cull_mode, skinned, blends).collect();
        assert_eq!(
            keys.len(),
            DEPTH_DIRECTION_COUNT * DEPTH_TEST_COUNT * blends.len()
        );
        for (position, key) in keys.iter().enumerate() {
            let slot = if blends.len() == BLEND_MODE_COUNT {
                key.blend.index()
            } else {
                key.blend.merge_index().unwrap()
            };
            assert_eq!(
                grid_index(key.direction, key.depth_test, slot, blends.len()),
                Some(position),
                "{key:?}"
            );
            assert_eq!((key.cull_mode, key.skinned), (cull_mode, skinned));
        }
    }
    assert_eq!(
        grid_index(
            DepthDirection::Standard,
            DepthTest::Nearer,
            Blend::MergeMask.index(),
            BLEND_MODE_COUNT
        ),
        None
    );
    let shadows: Vec<ShadowKey> = shadow_keys().collect();
    assert_eq!(shadows.len(), 8);
    for (position, key) in shadows.iter().enumerate() {
        assert_eq!(shadow_index(*key), position, "{key:?}");
    }
}
