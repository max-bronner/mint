use super::*;
use mint_render::{Camera, MeshData};
use mint_sage_ini::particle::KEY_SLOTS;
use mint_sage_ini::{BlockTable, Document, ParticleIndex, ParticleSystemDef};

fn definition(text: &str) -> ParticleSystemDef {
    let table = BlockTable::particle_path();
    let document = Document::parse("test.ini", text, &table).unwrap();
    let mut index = ParticleIndex::new();
    index.insert_document(&document);
    let held = index.iter().next().unwrap().1.clone();
    held
}

#[test]
fn a_degenerate_camera_still_gives_a_finite_billboard_basis() {
    let usable = |right: Vec3, up: Vec3| {
        right.is_finite()
            && up.is_finite()
            && (right.length() - 1.0).abs() < 1e-5
            && (up.length() - 1.0).abs() < 1e-5
    };

    let mut camera = Camera::default();
    camera.target = camera.eye;
    let (right, up) = billboard_axes(&camera);
    assert!(usable(right, up), "eye at the target: {right:?} {up:?}");

    camera.eye = Vec3::new(0.0, 0.0, 10.0);
    camera.target = Vec3::ZERO;
    camera.up = Vec3::new(0.0, 0.0, 1.0);
    let (right, up) = billboard_axes(&camera);
    assert!(usable(right, up), "up along the view: {right:?} {up:?}");

    camera.up = Vec3::Y;
    let (right, up) = billboard_axes(&camera);
    assert!(usable(right, up));
    assert!(right.dot(up).abs() < 1e-5, "the two axes are perpendicular");
}

#[test]
fn a_ground_aligned_quad_lies_in_the_world_plane_and_a_billboard_faces_the_camera() {
    let flat = definition(
        "FXParticleSystem Glow\n  System\n    IsGroundAligned = Yes\n    Size = 10 10\n  \
             End\nEnd\n",
    );
    assert!(flat.ground_aligned, "the fixture has to set the flag");

    let particles = [particle_at(Vec3::ZERO, 10.0)];
    let camera = Camera {
        eye: Vec3::new(0.0, -10.0, 10.0),
        target: Vec3::ZERO,
        up: Vec3::Z,
        ..Camera::default()
    };
    let (right, up) = billboard_axes(&camera);
    let mut scratch = MeshData {
        vertices: Vec::new(),
        indices: Vec::new(),
    };

    for mode in [BillboardMode::Data, BillboardMode::Ground] {
        let (r, u) = quad_axes(mode, flat.ground_aligned, right, up);
        fill_quads(
            &mut scratch,
            &particles,
            Vec3::ZERO,
            1,
            &QuadFill {
                right_axis: r,
                up_axis: u,
                keys: true,
                flipbook: None,
                smudge_seed: None,
                blend: 1.0,
            },
        );
        let spread = z_spread(&scratch);
        assert!(
            spread < 1e-4,
            "{} left the quad {spread} deep in Z",
            mode.label()
        );
    }

    let (r, u) = quad_axes(BillboardMode::Screen, flat.ground_aligned, right, up);
    fill_quads(
        &mut scratch,
        &particles,
        Vec3::ZERO,
        1,
        &QuadFill {
            right_axis: r,
            up_axis: u,
            keys: true,
            flipbook: None,
            smudge_seed: None,
            blend: 1.0,
        },
    );
    assert!(
        z_spread(&scratch) > 1.0,
        "a screen-facing quad at a tilted camera stands up out of the ground plane"
    );
    let (r, u) = quad_axes(BillboardMode::Ground, flat.ground_aligned, right, up);
    assert_eq!((r, u), (FLAT_RIGHT, FLAT_UP));

    assert_eq!(
        quad_axes(BillboardMode::Ground, false, right, up),
        (FLAT_RIGHT, FLAT_UP),
        "the ground rung must flatten a system that authored nothing"
    );
    assert_eq!(
        quad_axes(BillboardMode::Data, false, right, up),
        (right, up),
        "and the data rung must leave it screen-facing"
    );
}

#[test]
fn the_three_sort_rungs_are_three_different_answers() {
    assert_eq!(
        SortMode::None.level_for(Some(4)),
        Some(SUBMISSION_SORT_LEVEL)
    );
    assert_eq!(SortMode::None.level_for(None), Some(SUBMISSION_SORT_LEVEL));
    assert_eq!(SortMode::Level.level_for(Some(4)), Some(4));
    assert_eq!(
        SortMode::Level.level_for(None),
        None,
        "an unauthored level is WW3D2's SORT_LEVEL_NONE, which is the distance sort"
    );
    assert_eq!(SortMode::Depth.level_for(Some(4)), None);
    assert_eq!(SortMode::Depth.level_for(None), None);

    let mut rung = SortMode::None;
    for _ in 0..3 {
        assert_eq!(SortMode::parse(rung.label()), Some(rung));
        rung = rung.next();
    }
    assert_eq!(rung, SortMode::None, "the ladder wraps in three");
    assert_eq!(SortMode::parse("distance"), None);

    let mut rung = BillboardMode::Data;
    for _ in 0..3 {
        assert_eq!(BillboardMode::parse(rung.label()), Some(rung));
        rung = rung.next();
    }
    assert_eq!(rung, BillboardMode::Data);
}

#[test]
fn alpha_test_is_the_one_opaque_depth_writing_preset() {
    let (blend, cutoff) = shader_draw_state(ParticleShader::AlphaTest);
    assert_eq!(blend, Blend::Opaque);
    assert!(blend.writes_depth());
    assert_eq!(cutoff, 96.0 / 255.0);

    for shader in [
        ParticleShader::Additive,
        ParticleShader::Alpha,
        ParticleShader::Multiply,
        ParticleShader::None,
    ] {
        let (blend, cutoff) = shader_draw_state(shader);
        assert!(!blend.writes_depth(), "{shader:?} must not write depth");
        assert_eq!(cutoff, 0.0, "{shader:?} must not cut out");
    }
    assert_eq!(
        shader_draw_state(ParticleShader::Additive).0,
        Blend::Additive
    );
    assert_eq!(
        shader_draw_state(ParticleShader::Multiply).0,
        Blend::AlphaBlend
    );
}

fn particle_at(position: Vec3, size: f32) -> Particle {
    Particle {
        position,
        previous: position,
        previous_size: size,
        previous_angle: 0.0,
        velocity: Vec3::ZERO,
        size,
        age: 0,
        lifetime: 100,
        color: [1.0; 3],
        previous_color: [1.0; 3],
        color_rate: [0.0; 3],
        color_slot: 0,
        alpha: 1.0,
        previous_alpha: 1.0,
        alpha_rate: 0.0,
        alpha_slot: 0,
        alpha_values: [0.0; KEY_SLOTS],
        color_values: [[0.0; 3]; 8],
        velocity_damping: 1.0,
        size_rate: 0.0,
        size_rate_damping: 1.0,
        angle: 0.0,
        angular_rate: 0.0,
        angular_damping: 1.0,
        wind_jitter: 0.0,
        sheet_start: 0,
        attached: None,
        collided: false,
        emitter: position,
        render_size: None,
        previous_render_size: None,
        render_size_rate: [0.0; 3],
        render_size_damping: [1.0; 3],
    }
}

#[test]
fn a_flat_quad_matches_the_retail_ground_layout() {
    let mut particle = particle_at(Vec3::ZERO, 10.0);
    particle.angle = std::f32::consts::FRAC_PI_2;
    particle.previous_angle = particle.angle;
    let mut scratch = MeshData {
        vertices: Vec::new(),
        indices: Vec::new(),
    };
    fill_quads(
        &mut scratch,
        std::slice::from_ref(&particle),
        Vec3::ZERO,
        1,
        &QuadFill {
            right_axis: FLAT_RIGHT,
            up_axis: FLAT_UP,
            keys: true,
            flipbook: None,
            smudge_seed: None,
            blend: 1.0,
        },
    );
    let at = |uv: [f32; 2]| {
        scratch.vertices[..4]
            .iter()
            .find(|vertex| vertex.uv == uv)
            .map(|vertex| vertex.position)
            .unwrap()
    };
    let near = |a: [f32; 3], b: [f32; 3]| a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-4);
    assert!(
        near(at([0.0, 0.0]), [10.0, -10.0, 0.0]),
        "{:?}",
        at([0.0, 0.0])
    );
    assert!(
        near(at([0.0, 1.0]), [-10.0, -10.0, 0.0]),
        "{:?}",
        at([0.0, 1.0])
    );
    assert!(
        near(at([1.0, 1.0]), [-10.0, 10.0, 0.0]),
        "{:?}",
        at([1.0, 1.0])
    );
    assert!(
        near(at([1.0, 0.0]), [10.0, 10.0, 0.0]),
        "{:?}",
        at([1.0, 0.0])
    );
}

fn z_spread(mesh: &MeshData) -> f32 {
    let mut low = f32::MAX;
    let mut high = f32::MIN;
    for vertex in &mesh.vertices {
        low = low.min(vertex.position[2]);
        high = high.max(vertex.position[2]);
    }
    high - low
}

fn sheet(total: u32, per_row: u32, speed: f32) -> Flipbook {
    Flipbook {
        total_frames: total,
        frames_per_row: per_row,
        speed_multiplier: speed,
    }
}

#[test]
fn a_flipbook_cell_is_a_rect_of_the_sheet_and_wraps_at_the_end() {
    let book = sheet(16, 4, 1.0);
    assert_eq!(cell_uv(book, 0), ([0.0, 0.0], [0.25, 0.25]));
    assert_eq!(cell_uv(book, 1), ([0.25, 0.0], [0.25, 0.25]));
    assert_eq!(cell_uv(book, 4), ([0.0, 0.25], [0.25, 0.25]));
    assert_eq!(cell_uv(book, 15), ([0.75, 0.75], [0.25, 0.25]));
    assert_eq!(cell_uv(book, 16), cell_uv(book, 0), "the sheet loops");
    assert_eq!(cell_uv(book, 21), cell_uv(book, 5));
    assert_eq!(cell_uv(sheet(1, 1, 1.0), 7), ([0.0, 0.0], [1.0, 1.0]));
    let ragged = sheet(7, 4, 1.0);
    let (origin, size) = cell_uv(ragged, 6);
    assert!(origin[0] + size[0] <= 1.0 && origin[1] + size[1] <= 1.0);
    assert_eq!(cell_uv(ragged, 7), cell_uv(ragged, 0));
    assert_eq!(cell_uv(ragged, 8), cell_uv(ragged, 1));
    assert_ne!(cell_uv(ragged, 8), cell_uv(ragged, 0));
}

#[test]
fn the_decoded_timing_is_cells_per_logic_frame_from_the_particles_own_start() {
    let book = sheet(16, 4, 1.0);
    for mode in [
        FlipbookMode::Frame,
        FlipbookMode::Blend,
        FlipbookMode::Retail,
    ] {
        assert_eq!(mode.cell(book, 4.0, 5, 0), 4.0);
        assert_eq!(mode.cell(book, 4.0, 5, 3), 7.0);
        assert_eq!(mode.cell(book, 4.0, 5, 19), 7.0);
    }
    for mode in [FlipbookMode::Life, FlipbookMode::LifeBlend] {
        assert_eq!(mode.cell(book, 4.0, 5, 3), 4.0 / 5.0 * 16.0, "{mode:?}");
    }
    assert_eq!(FlipbookMode::Off.cell(book, 99.0, 5, 7), 0.0);
    assert_eq!(FlipbookMode::Blend.cell(sheet(16, 4, 2.0), 3.0, 30, 0), 6.0);
    assert_eq!(FlipbookMode::Life.cell(sheet(16, 4, 0.5), 15.0, 30, 0), 4.0);
    assert_eq!(FlipbookMode::Blend.cell(book, 2.25, 30, 0).fract(), 0.25);
    assert!(FlipbookMode::Life.cell(book, 0.0, 0, 0).is_finite());
    assert!(FlipbookMode::Life
        .cell(sheet(16, 4, f32::INFINITY), 3.0, 30, 0)
        .is_finite());
    assert!(FlipbookMode::Blend.cell(book, f32::NAN, 30, 0).is_finite());
}

#[test]
fn the_default_flipbook_cross_fades_on_the_decoded_timing() {
    assert_eq!(FlipbookMode::default(), FlipbookMode::Blend);
    assert!(FlipbookMode::default().needs_shader());
    assert_eq!(
        FlipbookMode::default().cell(sheet(16, 4, 1.0), 1.0, 99, 5),
        6.0
    );
    let mut mode = FlipbookMode::Off;
    for _ in 0..FlipbookMode::ALL.len() {
        assert_eq!(FlipbookMode::parse(mode.label()), Some(mode));
        mode = mode.next();
    }
    assert_eq!(mode, FlipbookMode::Off);
    assert_eq!(FlipbookMode::Retail.rung(), 6);
}

#[test]
fn the_retail_traversal_never_reaches_the_last_column() {
    let book = sheet(16, 4, 1.0);
    let cells: Vec<([f32; 2], [f32; 2])> = (0..16).map(|n| retail_cell_uv(book, n)).collect();
    assert!(cells.iter().all(|(origin, _)| origin[0] < 0.75 - 1e-6));
    let repeats = cells.windows(2).filter(|pair| pair[0] == pair[1]).count();
    assert_eq!(repeats, 5, "{cells:?}");
    let mut distinct = cells.clone();
    distinct.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    distinct.dedup();
    assert_eq!(distinct.len(), 11);
    assert!(cells.iter().any(|(origin, _)| origin[1] > 0.7));
    assert_eq!(retail_cell_uv(book, 16), retail_cell_uv(book, 0));
    assert!((0..16).any(|n| cell_uv(book, n).0[0] >= 0.75));
}

#[test]
fn the_cross_fade_weight_follows_the_render_blend() {
    let mut particle = particle_at(Vec3::ZERO, 2.0);
    particle.age = 3;
    particle.lifetime = 30;
    let book = sheet(16, 4, 1.0);
    let weight = |particle: &Particle, blend: f32| {
        cells_for(Some((book, FlipbookMode::Blend)), particle, blend).2
    };
    assert_eq!(weight(&particle, 1.0), 0.0);
    assert!((weight(&particle, 0.25) - 0.25).abs() < 1e-5);
    assert!((weight(&particle, 0.75) - 0.75).abs() < 1e-5);
    let one = cells_for(
        Some((sheet(1, 1, 1.0), FlipbookMode::Blend)),
        &particle,
        0.5,
    );
    assert_eq!(one.2, 0.0);
    particle.sheet_start = 5;
    let (this, _, _) = cells_for(Some((book, FlipbookMode::Blend)), &particle, 1.0);
    assert_eq!(this, cell_uv(book, 8));
    let (retail, _, _) = cells_for(Some((book, FlipbookMode::Retail)), &particle, 1.0);
    assert_eq!(retail, retail_cell_uv(book, 8));
}

#[test]
fn a_flipbook_writes_two_cells_and_the_weight_between_them() {
    let mut scratch = MeshData {
        vertices: Vec::new(),
        indices: Vec::new(),
    };
    let mut particle = particle_at(Vec3::ZERO, 2.0);
    particle.age = 2;
    particle.lifetime = 16;
    let book = sheet(16, 4, 1.0);
    fill_quads(
        &mut scratch,
        std::slice::from_ref(&particle),
        Vec3::ZERO,
        1,
        &QuadFill {
            right_axis: Vec3::X,
            up_axis: Vec3::Y,
            keys: true,
            flipbook: Some((book, FlipbookMode::Life)),
            smudge_seed: None,
            blend: 1.0,
        },
    );
    let (this, size) = cell_uv(book, 2);
    let (next, _) = cell_uv(book, 3);
    let corner = scratch.vertices[0];
    assert_eq!(corner.uv, [this[0], this[1] + size[1]]);
    assert_eq!(corner.uv2, [next[0], next[1] + size[1]]);
    assert_eq!(corner.tangent[0], 0.0, "age 2 lands on a cell boundary");

    particle.age = 3;
    particle.lifetime = 32;
    fill_quads(
        &mut scratch,
        std::slice::from_ref(&particle),
        Vec3::ZERO,
        1,
        &QuadFill {
            right_axis: Vec3::X,
            up_axis: Vec3::Y,
            keys: true,
            flipbook: Some((book, FlipbookMode::Life)),
            smudge_seed: None,
            blend: 1.0,
        },
    );
    assert!((scratch.vertices[0].tangent[0] - 0.5).abs() < 1e-5);

    fill_quads(
        &mut scratch,
        std::slice::from_ref(&particle),
        Vec3::ZERO,
        1,
        &QuadFill {
            right_axis: Vec3::X,
            up_axis: Vec3::Y,
            keys: true,
            flipbook: None,
            smudge_seed: None,
            blend: 1.0,
        },
    );
    assert_eq!(scratch.vertices[0].uv, [0.0, 1.0]);
    assert_eq!(scratch.vertices[0].tangent[0], 0.0);
}

#[test]
fn a_smudge_quad_carries_one_bounded_shift_and_the_particle_alpha() {
    let mut scratch = MeshData {
        vertices: Vec::new(),
        indices: Vec::new(),
    };
    let mut particle = particle_at(Vec3::ZERO, 2.0);
    particle.color = [0.5, 0.25, 1.0];
    particle.alpha = 0.5;
    let fill = |seed: Option<u64>| QuadFill {
        right_axis: Vec3::X,
        up_axis: Vec3::Y,
        keys: true,
        flipbook: None,
        smudge_seed: seed,
        blend: 1.0,
    };
    fill_quads(
        &mut scratch,
        std::slice::from_ref(&particle),
        Vec3::ZERO,
        1,
        &fill(None),
    );
    assert_eq!(scratch.vertices[0].tangent[1], 0.0);
    for seed in 1..64 {
        fill_quads(
            &mut scratch,
            std::slice::from_ref(&particle),
            Vec3::ZERO,
            1,
            &fill(Some(seed)),
        );
        let shift = scratch.vertices[0].tangent[1];
        assert!(shift.abs() <= SMUDGE_MAX_OFFSET);
        assert!(scratch.vertices[..4].iter().all(|v| v.tangent[1] == shift));
        assert_eq!(scratch.vertices[0].color, [0.5, 0.25, 1.0, 0.5]);
    }
}

#[test]
fn a_ribbon_is_a_strip_across_its_own_direction_with_a_transparent_tail() {
    let mut scratch = MeshData {
        vertices: Vec::new(),
        indices: Vec::new(),
    };
    let particles: Vec<Particle> = [0.0f32, 10.0, 20.0]
        .into_iter()
        .map(|x| {
            let mut held = particle_at(Vec3::new(x, 0.0, 0.0), 4.0);
            held.color = [1.0, 1.0, 1.0];
            held.alpha = 1.0;
            held
        })
        .collect();
    fill_ribbon(
        &mut scratch,
        &particles,
        Vec3::ZERO,
        4,
        Vec3::Z,
        true,
        1.0,
        None,
    );

    let (left, right) = (scratch.vertices[2], scratch.vertices[3]);
    assert_eq!(left.position[0], 10.0);
    assert_eq!(right.position[0], 10.0);
    assert!((left.position[1] - right.position[1]).abs() - 4.0 < 1e-5);
    assert_eq!(left.position[2], 0.0, "the strip does not leave the plane");

    assert_eq!(scratch.vertices[0].color, [0.0; 4]);
    assert_eq!(scratch.vertices[1].color, [0.0; 4]);
    assert_eq!(scratch.vertices[2].color[3], 1.0, "and only the first");

    assert_eq!(scratch.vertices[6].position, scratch.vertices[4].position);
    assert_eq!(scratch.vertices[7].position, scratch.vertices[5].position);
}

#[test]
fn a_ribbon_of_coincident_particles_has_no_width() {
    let mut scratch = MeshData {
        vertices: Vec::new(),
        indices: Vec::new(),
    };
    let particles: Vec<Particle> = (0..3).map(|_| particle_at(Vec3::ZERO, 8.0)).collect();
    fill_ribbon(
        &mut scratch,
        &particles,
        Vec3::ZERO,
        3,
        Vec3::Z,
        true,
        1.0,
        None,
    );
    for vertex in &scratch.vertices {
        assert_eq!(vertex.position, [0.0; 3]);
    }
}

#[test]
fn a_ribbon_has_one_quad_per_gap_and_a_quad_batch_one_per_particle() {
    assert_eq!(ribbon_mesh(4).vertices.len(), 8);
    assert_eq!(ribbon_mesh(4).indices.len(), 3 * INDICES_PER_QUAD);
    assert_eq!(ribbon_mesh(1).indices.len(), 0);
    assert_eq!(quad_batch_mesh(4).vertices.len(), 16);
    assert_eq!(quad_batch_mesh(4).indices.len(), 4 * INDICES_PER_QUAD);
    assert_ne!(
        ribbon_mesh(4).vertices.len(),
        quad_batch_mesh(4).vertices.len()
    );
}

#[test]
fn a_model_particle_is_placed_and_scaled() {
    let mut particle = particle_at(Vec3::new(3.0, -4.0, 5.0), 2.0);
    particle.angle = 0.0;
    let placed = model_transform(&particle, 1.0, None, ParticleRotation::Z);
    assert_eq!(
        [placed.get(0, 3), placed.get(1, 3), placed.get(2, 3)],
        [3.0, -4.0, 5.0]
    );
    assert!((placed.get(0, 0) - 2.0).abs() < 1e-5);
    assert!((placed.get(1, 1) - 2.0).abs() < 1e-5);
    assert!((placed.get(2, 2) - 2.0).abs() < 1e-5);

    particle.size = 0.0;
    let unit = model_transform(&particle, 1.0, None, ParticleRotation::Z);
    assert!((unit.get(2, 2) - 1.0).abs() < 1e-5);
}

#[test]
fn a_mesh_turns_about_the_axis_its_rotation_names_by_the_negated_angle() {
    let mut particle = particle_at(Vec3::ZERO, 1.0);
    particle.angle = 1.6;
    let shaft = |rotation| {
        model_transform(&particle, 1.0, None, rotation).transform_vector(Vec3::new(0.0, -1.0, 0.0))
    };
    let up = shaft(ParticleRotation::X);
    assert!(up.z > 0.99, "ROTATE_X stands the shaft up: {up:?}");
    let level = shaft(ParticleRotation::Z);
    assert!(level.z.abs() < 1e-5, "ROTATE_Z keeps it level: {level:?}");
    assert!(level.x < -0.99, "and turns -Y by -angle, to -X: {level:?}");
    let sideways = shaft(ParticleRotation::Y);
    assert!(
        (sideways.y + 1.0).abs() < 1e-5,
        "ROTATE_Y leaves its own axis: {sideways:?}"
    );
    assert_eq!(
        shaft(ParticleRotation::Off),
        Vec3::new(0.0, -1.0, 0.0),
        "no Rotation, no turn"
    );
}

#[test]
fn rotate_v_turns_about_the_velocity_with_y_negated() {
    let mut particle = particle_at(Vec3::ZERO, 1.0);
    particle.angle = std::f32::consts::FRAC_PI_2;
    particle.velocity = Vec3::new(0.0, -2.0, 0.0);
    let turned = model_transform(&particle, 1.0, None, ParticleRotation::Velocity);
    let z = turned.transform_vector(Vec3::Z);
    assert!((z.x + 1.0).abs() < 1e-5, "{z:?}");
    particle.velocity = Vec3::ZERO;
    let still = model_transform(&particle, 1.0, None, ParticleRotation::Velocity);
    assert_eq!(
        still.transform_vector(Vec3::Z),
        Vec3::Z,
        "no velocity, no axis"
    );
}

#[test]
fn a_model_particle_is_placed_in_its_system_s_frame_with_per_axis_size() {
    let mut particle = particle_at(Vec3::new(500.0, 1700.0, 300.0), 1.0);
    particle.angle = 0.0;
    particle.render_size = Some([0.1, 1.0, 0.1]);
    particle.previous_render_size = Some([0.1, 1.0, 0.1]);

    let quarter_turn = Mat4::from_cols([
        [0.0, 1.0, 0.0, 0.0],
        [-1.0, 0.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [500.0, 1700.0, 300.0, 1.0],
    ]);
    let placed = model_transform(&particle, 1.0, Some(quarter_turn), ParticleRotation::Z);
    assert_eq!(
        placed.transform_point(Vec3::ZERO),
        Vec3::new(500.0, 1700.0, 300.0),
        "the particle's own position, not that plus the frame's origin"
    );
    assert!((placed.get(1, 0) - 0.1).abs() < 1e-5, "{placed:?}");
    assert!(placed.get(0, 0).abs() < 1e-5);
    assert!((placed.get(0, 1) - (-1.0)).abs() < 1e-5);
    assert!((placed.get(1, 1)).abs() < 1e-5);
}

#[test]
fn a_per_axis_size_at_or_below_zero_draws_nothing_on_that_axis() {
    let mut particle = particle_at(Vec3::ZERO, 0.1);
    particle.angle = 0.0;
    particle.render_size = Some([0.0, 0.5, -0.2]);
    particle.previous_render_size = particle.render_size;
    let placed = model_transform(&particle, 1.0, None, ParticleRotation::Z);
    assert!(placed.get(0, 0).abs() < 1e-6, "{placed:?}");
    assert!((placed.get(1, 1) - 0.5).abs() < 1e-6, "{placed:?}");
    assert!(placed.get(2, 2).abs() < 1e-6, "{placed:?}");
}

const BUILT: PartKeys = PartKeys {
    tint: [1.0, 1.0, 1.0, 0.75],
    diffuse: [0.25, 0.25, 0.25],
    emissive: [0.5, 0.5, 0.5],
};

#[test]
fn each_w3d_shader_writes_its_own_material_term() {
    let colour = [0.5, 1.0, 0.25];
    let emissive = part_keys(Some(ParticleShader::W3dEmissive), BUILT, colour, 0.5, true);
    assert_eq!(emissive.emissive, [0.25, 0.5, 0.125]);
    assert_eq!(
        (emissive.tint, emissive.diffuse),
        (BUILT.tint, BUILT.diffuse)
    );

    let diffuse = part_keys(Some(ParticleShader::W3dDiffuse), BUILT, colour, 0.5, true);
    assert_eq!(diffuse.diffuse, [0.25, 0.5, 0.125]);
    assert_eq!(
        (diffuse.tint, diffuse.emissive),
        (BUILT.tint, BUILT.emissive)
    );

    let opacity = part_keys(Some(ParticleShader::W3dAlpha), BUILT, colour, 0.5, true);
    assert_eq!(
        opacity.tint,
        [1.0, 1.0, 1.0, 0.5],
        "W3D_ALPHA carries no colour"
    );
    assert_eq!(
        (opacity.diffuse, opacity.emissive),
        (BUILT.diffuse, BUILT.emissive)
    );

    let dropped = part_keys(Some(ParticleShader::W3dEmissive), BUILT, colour, 0.5, false);
    assert_eq!(dropped.emissive, colour);
}

#[test]
fn a_sprite_shader_leaves_a_model_part_as_built() {
    for shader in [
        Some(ParticleShader::Alpha),
        Some(ParticleShader::Additive),
        None,
    ] {
        assert_eq!(part_keys(shader, BUILT, [0.5; 3], 0.5, true), BUILT);
    }
}

#[test]
fn a_volume_particle_is_six_attenuated_copies_stepped_toward_the_camera() {
    let copies = volume_copies(Vec3::Z, 12.0);
    assert_eq!(copies.len(), VOLUME_PARTICLE_COPIES);
    let z: Vec<f32> = copies.iter().map(|held| held.0.z).collect();
    assert!(z.windows(2).all(|pair| pair[0] > pair[1]), "{z:?}");
    assert!(z[VOLUME_PARTICLE_COPIES - 1].abs() < 1e-5);
    assert!((z[0] - 10.0).abs() < 1e-5);
    for (_, attenuation) in &copies {
        assert!((attenuation - VOLUME_PARTICLE_ATTENUATION).abs() < 1e-6);
    }
    #[allow(clippy::cast_precision_loss)]
    let summed: f32 = copies.iter().map(|held| held.1).sum();
    assert!((summed - 1.0).abs() < 1e-5, "six copies sum to one");
    assert!(volume_copies(Vec3::Z, f32::NAN)
        .iter()
        .all(|held| held.0.z == 0.0));
}

#[test]
fn the_draw_module_decides_the_shape_and_the_type_fills_the_gaps() {
    use mint_sage_ini::ParticleKind;
    assert_eq!(
        DrawShape::of(ParticleKind::Particle, Some("RenderObjectDraw"), None),
        DrawShape::Model
    );
    assert_eq!(
        DrawShape::of(ParticleKind::Particle, Some("LightningDraw"), None),
        DrawShape::Streak
    );
    assert_eq!(
        DrawShape::of(ParticleKind::Particle, Some("StreakDraw"), None),
        DrawShape::Streak
    );
    assert_eq!(
        DrawShape::of(ParticleKind::VolumeParticle, None, None),
        DrawShape::Volume
    );
    assert_eq!(
        DrawShape::of(ParticleKind::VolumeParticle, Some("DefaultDraw"), None),
        DrawShape::Volume
    );
    assert_eq!(
        DrawShape::of(ParticleKind::Smudge, None, None),
        DrawShape::Smudge
    );
    assert_eq!(
        DrawShape::of(
            ParticleKind::Particle,
            Some("DefaultDraw"),
            Some("SMUDGE.tga")
        ),
        DrawShape::Smudge
    );
    assert_eq!(
        DrawShape::of(ParticleKind::Particle, None, Some("smudge.tga")),
        DrawShape::Sprite
    );
    assert_eq!(
        DrawShape::of(ParticleKind::Drawable, None, Some("SMUDGE.w3d")),
        DrawShape::Model
    );
    assert_eq!(
        DrawShape::of(ParticleKind::GpuParticle, Some("GpuDraw"), None),
        DrawShape::Sprite
    );
    assert_eq!(
        DrawShape::of(ParticleKind::GpuTerrainFire, Some("GpuDraw"), None),
        DrawShape::Sprite
    );
    assert_eq!(
        DrawShape::of(ParticleKind::Particle, Some("ButterflyDraw"), None),
        DrawShape::Sprite
    );
    assert_eq!(
        DrawShape::of(ParticleKind::Particle, Some("QuadDraw"), None),
        DrawShape::Sprite
    );
}

#[test]
fn the_shape_rung_forces_every_system_to_the_sprite_path() {
    for shape in [
        DrawShape::Sprite,
        DrawShape::Model,
        DrawShape::Streak,
        DrawShape::Volume,
        DrawShape::Smudge,
    ] {
        assert_eq!(ShapeMode::Data.shape(shape), shape);
        assert_eq!(ShapeMode::Sprite.shape(shape), DrawShape::Sprite);
    }
    assert_eq!(DrawShape::Streak.batch(), Some(BatchShape::Ribbon));
    assert_eq!(DrawShape::Sprite.batch(), Some(BatchShape::Quads));
    assert_eq!(DrawShape::Volume.batch(), Some(BatchShape::Quads));
    assert_eq!(DrawShape::Smudge.batch(), Some(BatchShape::Quads));
    assert_eq!(
        DrawShape::Model.batch(),
        None,
        "a model draws the caller's parts and uploads no geometry here"
    );
}
