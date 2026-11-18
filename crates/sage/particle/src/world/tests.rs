use super::*;
use crate::budget::DetailLevel;
use crate::ground::{FlatGround, NoGround};
use mint_sage_ini::{BlockTable, Document, ParticleIndex, ParticleRotation};

#[test]
fn a_particles_start_cell_is_stable_and_scatters_over_the_sheet() {
    let a = sheet_start(Vec3::new(1.0, 2.0, 3.0), 4.0, 0.5, 30);
    assert_eq!(a, sheet_start(Vec3::new(1.0, 2.0, 3.0), 4.0, 0.5, 30));
    assert_ne!(a, sheet_start(Vec3::new(1.0, 2.0, 3.0), 4.0, 0.6, 30));
    let mut cells = [false; 16];
    for i in 0..200u16 {
        let x = f32::from(i) * 0.37;
        let start = sheet_start(Vec3::new(x, 0.0, 0.0), 4.0, 0.0, 30);
        cells[usize::try_from(start % 16).unwrap()] = true;
    }
    assert!(cells.iter().all(|hit| *hit), "{cells:?}");
}

fn definition(text: &str) -> ParticleSystemDef {
    let table = BlockTable::particle_path();
    let document = Document::parse("test.ini", text, &table).unwrap();
    assert!(document.is_balanced(), "{:?}", document.diagnostics());
    let mut index = ParticleIndex::new();
    index.insert_document(&document);
    let first = index.iter().next().unwrap().1.clone();
    first
}

fn emission(text: &str) -> Emission {
    Emission::compile(&definition(text))
}

#[test]
fn the_batch_is_sized_from_the_data_and_clamped_at_both_ends() {
    let small = definition(
        "FXParticleSystem Small\n  System\n    Lifetime = 10 10\n    BurstCount = 1 1\n    \
             BurstDelay = 5 5\n  End\nEnd\n",
    );
    assert_eq!(batch_capacity(&small), 4, "3 live, floored at the minimum");

    let huge = definition(
        "FXParticleSystem Huge\n  System\n    Lifetime = 10000 10000\n    BurstCount = 2000 \
             2000\n  End\nEnd\n",
    );
    assert_eq!(batch_capacity(&huge), MAX_PARTICLES_PER_SYSTEM);

    let mut broken = small;
    broken.lifetime = RandomRange {
        low: f32::NAN,
        high: f32::INFINITY,
    };
    broken.burst_count = RandomRange::constant(f32::INFINITY);
    let capacity = batch_capacity(&broken);
    assert!((MIN_PARTICLES_PER_SYSTEM..=MAX_PARTICLES_PER_SYSTEM).contains(&capacity));
}

#[test]
fn colour_ramps_from_the_unauthored_first_key_to_the_authored_one() {
    let dust = emission(
        "FXParticleSystem Dust\n\
             \x20 System\n    Shader = ALPHA\n    Lifetime = 100 100\n  End\n\
             \x20 Color = DefaultColor\n    Color2 = R:255 G:0 B:0 10\n  End\n\
             \x20 Alpha = DefaultAlpha\n    Alpha2 = 1 1 10\n  End\n\
             End\n",
    );
    let mut rng = ClientRng::new(1);
    let mut particle = birth(&dust, &Placement::at(Vec3::ZERO), &mut rng);
    assert_eq!(particle.color, [0.0; 3], "born black");
    assert_eq!(particle.alpha, 0.0, "born transparent");

    for _ in 0..5 {
        update(&mut particle, &dust, true, None, &[], None);
    }
    assert!(
        (particle.color[0] - 0.5).abs() < 1e-5,
        "half way at frame 5, got {}",
        particle.color[0]
    );
    assert!((particle.alpha - 0.5).abs() < 1e-5);

    for _ in 5..10 {
        update(&mut particle, &dust, true, None, &[], None);
    }
    assert!((particle.color[0] - 1.0).abs() < 1e-5);
    assert_eq!(particle.alpha, 1.0, "alpha snaps to the key it reaches");

    for _ in 0..20 {
        update(&mut particle, &dust, true, None, &[], None);
    }
    assert!((particle.color[0] - 1.0).abs() < 1e-5);
    assert_eq!(particle.alpha, 1.0);
}

#[test]
fn color_scale_moves_the_later_keys_once_at_birth() {
    let tinted = emission(
        "FXParticleSystem Tinted
               System
    Lifetime = 200 200
  End
               Color = DefaultColor
    Color1 = R:255 G:0 B:0 0
                 Color2 = R:255 G:0 B:102 10
    ColorScale = 51 51
  End
             End
",
    );
    let mut rng = ClientRng::new(2);
    let mut particle = birth(&tinted, &Placement::at(Vec3::ZERO), &mut rng);
    assert_eq!(particle.color, [1.0, 0.0, 0.0]);
    for _ in 0..10 {
        update(&mut particle, &tinted, true, None, &[], None);
    }
    let expected = [1.0, 0.0, 153.0 / 255.0];
    for (got, want) in particle.color.iter().zip(expected) {
        assert!(
            (got - want).abs() < 1e-4,
            "{:?} against {expected:?}",
            particle.color
        );
    }
    for _ in 0..50 {
        update(&mut particle, &tinted, true, None, &[], None);
    }
    for (got, want) in particle.color.iter().zip(expected) {
        assert!(
            (got - want).abs() < 1e-4,
            "nothing added per frame: {:?}",
            particle.color
        );
    }
}

#[test]
fn an_additive_system_leaves_its_alpha_where_it_was_born() {
    let glow = emission(
        "FXParticleSystem Glow\n\
             \x20 System\n    Lifetime = 100 100\n  End\n\
             \x20 Alpha = DefaultAlpha\n    Alpha1 = 1 1 0\n    Alpha2 = 0 0 10\n  End\n\
             End\n",
    );
    assert!(!glow.integrate_alpha);
    let mut rng = ClientRng::new(3);
    let mut particle = birth(&glow, &Placement::at(Vec3::ZERO), &mut rng);
    for _ in 0..50 {
        update(&mut particle, &glow, true, None, &[], None);
    }
    assert_eq!(particle.alpha, 1.0);
}

#[test]
fn a_zero_frame_terminates_the_list_and_a_huge_one_is_never_reached() {
    let frames = [0, 10, 0, 5, 0, 0, 0, 0];
    assert_eq!(reached(frames, 0, 9), None, "not yet");
    assert_eq!(reached(frames, 0, 10), Some(1));
    assert_eq!(reached(frames, 1, 10_000), None, "slot 2's frame is 0");

    let sentinel = [0, 3_703_221_197, 0, 0, 0, 0, 0, 0];
    assert_eq!(reached(sentinel, 0, u32::MAX / 2), None);
    assert_eq!(reached(sentinel, 0, 3_703_221_197), Some(1));
}

#[test]
fn moving_a_system_spreads_its_next_burst_along_the_path_and_leaves_its_last_ones() {
    let definition = definition(
        "FXParticleSystem Torch
  System
    Lifetime = 100 100
    BurstCount = 2 2
                 BurstDelay = 0 0
  End
End
",
    );
    let mut world = ParticleWorld::new(303);
    let handle = world.register(&definition, None);
    let id = world.spawn(handle, Vec3::new(0.0, 0.0, 0.0)).unwrap();
    world.warm_up(1.0 / 30.0, &NoGround);
    assert_eq!(world.live_particles(), 2);

    assert!(world.set_origin(id, Vec3::new(100.0, 0.0, 0.0)));
    world.warm_up(1.0 / 30.0, &NoGround);
    let places: Vec<f32> = world.systems[0]
        .particles
        .iter()
        .map(|particle| particle.position.x)
        .collect();
    assert_eq!(places, [0.0, 0.0, 0.0, 50.0]);

    world.remove(id);
    assert!(!world.set_origin(id, Vec3::ZERO));
}

#[test]
fn a_held_system_births_nothing_and_its_burst_countdown_stands_still() {
    let definition = definition(
        "FXParticleSystem HeldTest
  System
    Lifetime = 100 100
    BurstCount = 1 1
    BurstDelay = 2 2
  End
End
",
    );
    let mut world = ParticleWorld::new(7);
    let handle = world.register(&definition, None);
    let id = world.spawn(handle, Vec3::ZERO).unwrap();
    let step = |world: &mut ParticleWorld, frames: u8| {
        world.warm_up(f32::from(frames) / 30.0, &NoGround);
    };
    step(&mut world, 1);
    assert_eq!(world.live_particles(), 1);

    assert!(world.set_suppressed(id, true));
    step(&mut world, 5);
    assert_eq!(world.live_particles(), 1, "a held system emitted");

    assert!(world.set_suppressed(id, false));
    step(&mut world, 2);
    assert_eq!(world.live_particles(), 1, "the countdown ran while held");
    step(&mut world, 1);
    assert_eq!(world.live_particles(), 2);
}

#[test]
fn particles_attached_to_the_bone_ride_the_system_and_ignore_their_velocity() {
    let definition = definition(
        "FXParticleSystem PinnedGlowTest
  System
    Lifetime = 100 100
    BurstCount = 1 1
    BurstDelay = 0 0
  End
  Physics = DefaultPhysics
    VelocityDamping = 1 1
    DriftVelocity = X:0 Y:2 Z:0
    ParticlesAttachToBone = Yes
  End
  EmissionVelocity = OrthoEmissionVelocity
    X = 5 5
  End
End
",
    );
    assert!(definition.particles_attach_to_bone);
    let mut world = ParticleWorld::new(17);
    let handle = world.register(&definition, None);
    let id = world.spawn(handle, Vec3::new(10.0, 0.0, 3.0)).unwrap();
    world.warm_up(3.0 / 30.0, &NoGround);
    assert!(world.set_origin(id, Vec3::new(40.0, -6.0, 3.0)));
    world.warm_up(1.0 / 30.0, &NoGround);
    let places: Vec<Vec3> = world.systems[0]
        .particles
        .iter()
        .map(|particle| particle.position)
        .collect();
    assert_eq!(places.len(), 4);
    assert!(
        places.iter().all(|at| *at == Vec3::new(40.0, -6.0, 3.0)),
        "{places:?}"
    );
}

#[test]
fn set_frame_turns_the_next_births_velocity_with_it() {
    let (mut world, handle) = world(
        "FXParticleSystem Bone\n  System\n    Lifetime = 100 100\n    BurstCount = 1 1\n    \
             Size = 1 1\n  End\n  Physics = DefaultPhysics\n    VelocityDamping = 1 1\n  End\n\
             \x20 EmissionVelocity = OrthoEmissionVelocity\n    X = -1 -1\n  End\nEnd\n",
    );
    let id = world.spawn(handle, Vec3::ZERO).unwrap();
    let quarter_turn = Mat4::from_cols([
        [0.0, 1.0, 0.0, 0.0],
        [-1.0, 0.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [5.0, 0.0, 0.0, 1.0],
    ]);
    assert!(world.set_frame(id, quarter_turn));
    world.warm_up(1.0 / 30.0, &NoGround);
    assert_eq!(world.systems[0].origin, Vec3::new(5.0, 0.0, 0.0));
    assert_eq!(
        world.systems[0].particles[0].velocity,
        Vec3::new(0.0, -1.0, 0.0),
        "local X:-1 became -Y under the quarter turn"
    );
}

fn drifting_world(seed: u64) -> ParticleWorld {
    let definition = definition(
        "FXParticleSystem Drift\n\
             \x20 System\n    Lifetime = 600 600\n    Size = 4 4\n\
             \x20   BurstCount = 1 1\n    BurstDelay = 0 0\n  End\n\
             \x20 Physics = DefaultPhysics\n    DriftVelocity = X:2 Y:0 Z:0\n  End\n\
             End\n",
    );
    let mut world = ParticleWorld::new(seed);
    let handle = world.register(&definition, None);
    world.spawn(handle, Vec3::ZERO).unwrap();
    world.warm_up(2.0 / 30.0, &NoGround);
    assert!(world.live_particles() >= 1);
    world
}

#[test]
fn a_simulated_position_changes_only_on_the_frames_that_carry_a_step() {
    let mut world = drifting_world(505);
    let mut steps = 0u32;
    let mut still = 0u32;
    let mut last = world.systems[0].particles[0].position;
    for _ in 0..144 {
        steps = steps.saturating_add(world.advance(1.0 / 144.0, &NoGround));
        let now = world.systems[0].particles[0].position;
        if (now.x - last.x).abs() < 1e-9 {
            still = still.saturating_add(1);
        }
        last = now;
    }
    assert!(
        (29..=30).contains(&steps),
        "144 frames produced {steps} steps, not the 30 a 30 Hz world owes"
    );
    assert_eq!(
        still,
        144 - steps,
        "every frame that did not step should have seen an unchanged position"
    );
}

#[test]
fn the_smooth_rung_moves_the_drawn_position_on_every_frame() {
    for smooth in [true, false] {
        let mut world = drifting_world(505);
        world.set_smooth(smooth);
        let mut steps = 0u32;
        let mut still = 0u32;
        let mut last = {
            let blend = world.draw_blend();
            world.systems[0].particles[0].drawn(blend).0
        };
        for _ in 0..144 {
            steps = steps.saturating_add(world.advance(1.0 / 144.0, &NoGround));
            let blend = world.draw_blend();
            let now = world.systems[0].particles[0].drawn(blend).0;
            if (now.x - last.x).abs() < 1e-9 {
                still = still.saturating_add(1);
            }
            last = now;
        }
        let want = if smooth { 0 } else { 144 - steps };
        assert_eq!(
            still, want,
            "with smooth = {smooth} the drawn position stood still on {still} of 144 frames"
        );
    }
}

#[test]
fn the_draw_blend_stays_between_the_two_steps_it_interpolates() {
    let world = drifting_world(606);
    let particle = &world.systems[0].particles[0];
    assert_ne!(
        particle.previous, particle.position,
        "the test needs a particle that actually moved"
    );
    assert_eq!(particle.drawn(0.0).0, particle.previous);
    assert_eq!(particle.drawn(1.0).0, particle.position);
    for bad in [f32::NAN, f32::INFINITY, -1.0, 2.0] {
        let (position, size, angle) = particle.drawn(bad);
        assert!(position.is_finite(), "blend {bad} gave {position:?}");
        assert!(size.is_finite() && angle.is_finite());
    }
}

#[test]
fn removing_a_system_takes_its_particles_and_returns_its_buffer() {
    let definition = definition(
        "FXParticleSystem Puff\n  System\n    Lifetime = 100 100\n    BurstCount = 4 4\n\
             \x20   BurstDelay = 0 0\n  End\nEnd\n",
    );
    let mut world = ParticleWorld::new(404);
    let handle = world.register(&definition, None);
    let first = world.spawn(handle, Vec3::ZERO).unwrap();
    let second = world.spawn(handle, Vec3::ZERO).unwrap();
    world.warm_up(0.1, &NoGround);
    assert_eq!(world.live_systems(), 2);
    let before = world.live_particles();

    assert!(world.remove(first));
    assert_eq!(world.live_systems(), 1);
    assert!(world.live_particles() < before);
    assert!(!world.remove(first), "removing it twice is not an error");
    assert!(world.set_origin(second, Vec3::ZERO), "the other survived");
}

#[test]
fn a_hollow_box_emits_from_its_surface_and_a_solid_one_from_inside() {
    let hollow = emission(
        "FXParticleSystem Shell\n\
             \x20 System\n    Lifetime = 10 10\n  End\n\
             \x20 EmissionVolume = BoxEmissionVolume\n\
             \x20   HalfSize = X:10 Y:20 Z:30\n    IsHollow = Yes\n  End\n\
             End\n",
    );
    let mut rng = ClientRng::new(66);
    let mut faces = [0u32; 3];
    for _ in 0..3_000 {
        let at = birth(&hollow, &Placement::at(Vec3::ZERO), &mut rng).position;
        let on = [
            (at.x.abs() - 10.0).abs() < 1e-3,
            (at.y.abs() - 20.0).abs() < 1e-3,
            (at.z.abs() - 30.0).abs() < 1e-3,
        ];
        let hit = on.iter().position(|held| *held);
        assert!(hit.is_some(), "{at:?} is not on any face");
        assert!(at.x.abs() <= 10.001 && at.y.abs() <= 20.001 && at.z.abs() <= 30.001);
        if let Some(slot) = hit {
            faces[slot] += 1;
        }
    }
    assert!(
        faces.iter().all(|count| *count > 500),
        "one of the three axis pairs was never chosen: {faces:?}"
    );

    let solid = emission(
        "FXParticleSystem Solid\n\
             \x20 System\n    Lifetime = 10 10\n  End\n\
             \x20 EmissionVolume = BoxEmissionVolume\n\
             \x20   HalfSize = X:10 Y:20 Z:30\n  End\n\
             End\n",
    );
    let mut inside = 0u32;
    for _ in 0..1_000 {
        let at = birth(&solid, &Placement::at(Vec3::ZERO), &mut rng).position;
        assert!(at.x.abs() <= 10.001 && at.y.abs() <= 20.001 && at.z.abs() <= 30.001);
        inside += u32::from(at.x.abs() < 9.0 && at.y.abs() < 18.0 && at.z.abs() < 27.0);
    }
    assert!(
        inside > 500,
        "a solid box hugged its faces: {inside} of 1,000"
    );
}

#[test]
fn the_integration_order_is_accelerate_damp_then_move() {
    let motion = emission(
        "FXParticleSystem Puff\n\
             \x20 System\n    Lifetime = 100 100\n    Size = 10 10\n  End\n\
             \x20 EmissionVelocity = OrthoEmissionVelocity\n    Z = 4 4\n  End\n\
             \x20 Physics = DefaultPhysics\n\
             \x20   Gravity = -1\n    VelocityDamping = 0.5 0.5\n\
             \x20   DriftVelocity = X:2 Y:0 Z:0\n  End\n\
             End\n",
    );
    let mut rng = ClientRng::new(77);
    let mut particle = birth(&motion, &Placement::at(Vec3::ZERO), &mut rng);
    assert_eq!(particle.velocity.z, 4.0);

    assert!(update(&mut particle, &motion, true, None, &[], None));
    assert!((particle.velocity.z - 1.5).abs() < 1e-5);
    assert!((particle.position.z - 1.5).abs() < 1e-5);
    assert!((particle.position.x - 2.0).abs() < 1e-5);

    assert!(update(&mut particle, &motion, true, None, &[], None));
    assert!((particle.velocity.z - 0.25).abs() < 1e-5);
    assert!((particle.position.z - 1.75).abs() < 1e-5);
    assert!((particle.position.x - 4.0).abs() < 1e-5);
}

#[test]
fn drift_is_not_damped_and_not_integrated_through_the_velocity() {
    let drift = emission(
        "FXParticleSystem Smoke\n\
             \x20 System\n    Lifetime = 100 100\n  End\n\
             \x20 Physics = DefaultPhysics\n\
             \x20   VelocityDamping = 0.5 0.5\n    DriftVelocity = X:0 Y:0 Z:1\n  End\n\
             End\n",
    );
    let mut rng = ClientRng::new(88);
    let mut particle = birth(&drift, &Placement::at(Vec3::ZERO), &mut rng);
    for frame in 1..=20u16 {
        assert!(update(&mut particle, &drift, true, None, &[], None));
        let expected = f32::from(frame);
        assert!(
            (particle.position.z - expected).abs() < 1e-4,
            "frame {frame}: {} instead of {expected}",
            particle.position.z
        );
    }
}

#[test]
fn size_and_spin_are_rates_that_damp_toward_a_limit() {
    let grow = emission(
        "FXParticleSystem Dust\n\
             \x20 System\n    Lifetime = 500 500\n  End\n\
             \x20 Update = DefaultUpdate\n\
             \x20   SizeRate = 4 4\n    SizeRateDamping = 0.5 0.5\n\
             \x20   AngleZ = 0 0\n    AngularRateZ = 1 1\n    AngularDamping = 0.5 0.5\n  End\n\
             End\n",
    );
    let mut rng = ClientRng::new(99);
    let mut particle = birth(&grow, &Placement::at(Vec3::ZERO), &mut rng);
    assert_eq!(particle.size, 0.0, "no Size authored");
    assert!(update(&mut particle, &grow, true, None, &[], None));
    assert!((particle.size - 4.0).abs() < 1e-5);
    assert!((particle.angle - 1.0).abs() < 1e-5);
    for _ in 0..60 {
        assert!(update(&mut particle, &grow, true, None, &[], None));
    }
    assert!((particle.size - 8.0).abs() < 1e-3, "size {}", particle.size);
    assert!(
        (particle.angle - 2.0).abs() < 1e-3,
        "angle {}",
        particle.angle
    );
}

#[test]
fn motion_off_is_step_threes_straight_line() {
    let motion = emission(
        "FXParticleSystem Puff\n\
             \x20 System\n    Lifetime = 100 100\n  End\n\
             \x20 EmissionVelocity = OrthoEmissionVelocity\n    Z = 4 4\n  End\n\
             \x20 Physics = DefaultPhysics\n\
             \x20   Gravity = -1\n    VelocityDamping = 0.5 0.5\n\
             \x20   DriftVelocity = X:2 Y:0 Z:0\n  End\n\
             End\n",
    );
    let mut rng = ClientRng::new(101);
    let mut particle = birth(&motion, &Placement::at(Vec3::ZERO), &mut rng);
    for frame in 1..=10u16 {
        assert!(update(&mut particle, &motion, false, None, &[], None));
        assert!((particle.position.z - 4.0 * f32::from(frame)).abs() < 1e-4);
        assert_eq!(particle.position.x, 0.0, "the drift leaked in");
    }
}

#[test]
fn a_particle_that_leaves_the_numbers_is_dropped_rather_than_drawn() {
    let runaway = emission(
        "FXParticleSystem Runaway\n\
             \x20 System\n    Lifetime = 1000 1000\n  End\n\
             \x20 EmissionVelocity = OrthoEmissionVelocity\n    Z = 1 1\n  End\n\
             \x20 Physics = DefaultPhysics\n\
             \x20   VelocityDamping = 8.07921e+33 8.07921e+33\n  End\n\
             End\n",
    );
    let mut rng = ClientRng::new(202);
    let mut particle = birth(&runaway, &Placement::at(Vec3::ZERO), &mut rng);
    let mut alive = 0u32;
    while update(&mut particle, &runaway, true, None, &[], None) {
        alive += 1;
        assert!(alive < 50, "the runaway never left the numbers");
    }
    assert!(
        particle.age < 50,
        "dropped long before its lifetime ran out"
    );
}

#[test]
fn a_sphere_is_hollow_at_its_surface_and_solid_inside_it() {
    let shell = emission(
        "FXParticleSystem Shell\n\
             \x20 System\n    Lifetime = 10 10\n  End\n\
             \x20 EmissionVolume = SphereEmissionVolume\n\
             \x20   Radius = 20\n    IsHollow = Yes\n  End\n\
             End\n",
    );
    let mut rng = ClientRng::new(21);
    for _ in 0..2_000 {
        let at = birth(&shell, &Placement::at(Vec3::ZERO), &mut rng).position;
        let reach = at.length();
        assert!((reach - 20.0).abs() < 1e-2, "{reach} is not on the shell");
    }

    let solid = emission(
        "FXParticleSystem Solid\n\
             \x20 System\n    Lifetime = 10 10\n  End\n\
             \x20 EmissionVolume = SphereEmissionVolume\n\
             \x20   Radius = 20\n  End\n\
             End\n",
    );
    let mut near = 0u32;
    for _ in 0..2_000 {
        let reach = birth(&solid, &Placement::at(Vec3::ZERO), &mut rng)
            .position
            .length();
        assert!(reach <= 20.001, "{reach} left the sphere");
        near += u32::from(reach < 10.0);
    }
    assert!(near > 500, "only {near} of 2,000 births were inside r/2");
}

#[test]
fn a_cylinder_spreads_in_xy_and_is_bounded_and_offset_on_z() {
    let ring = emission(
        "FXParticleSystem Ring\n\
             \x20 System\n    Lifetime = 10 10\n  End\n\
             \x20 EmissionVolume = CylinderEmissionVolume\n\
             \x20   Radius = 50\n    Length = 10\n    Offset = X:0 Y:0 Z:25\n    IsHollow = Yes\n\
             \x20 End\n\
             End\n",
    );
    let mut rng = ClientRng::new(33);
    let mut widest: f32 = 0.0;
    for _ in 0..2_000 {
        let at = birth(&ring, &Placement::at(Vec3::ZERO), &mut rng).position;
        let flat = Vec3::new(at.x, at.y, 0.0).length();
        assert!((flat - 50.0).abs() < 1e-2, "{flat} is not on the wall");
        assert!((20.0..=30.0).contains(&at.z), "{} left the length", at.z);
        widest = widest.max(at.x);
    }
    assert!(widest > 49.0, "the ring never reached +X: {widest}");
}

#[test]
fn a_cylinder_with_no_radius_emits_on_its_axis_and_outward_gives_it_nothing() {
    let fireflies = emission(
        "FXParticleSystem FireFlies02\n\
             \x20 System\n    Lifetime = 40 40\n    Size = 80 90\n    BurstCount = 1 1\n  End\n\
             \x20 Physics = DefaultPhysics\n    VelocityDamping = 1 1\n  End\n\
             \x20 EmissionVelocity = OutwardEmissionVelocity\n  End\n\
             \x20 EmissionVolume = CylinderEmissionVolume\n    Length = 75\n  End\n\
             End\n",
    );
    let mut rng = ClientRng::new(2141);
    let (mut lowest, mut highest) = (f32::MAX, f32::MIN);
    for _ in 0..2_000 {
        let particle = birth(&fireflies, &Placement::at(Vec3::ZERO), &mut rng);
        let at = particle.position;
        assert!(
            at.x == 0.0 && at.y == 0.0,
            "a radius of 0 must put the birth on the axis, not at ({}, {})",
            at.x,
            at.y
        );
        assert!((-37.5..=37.5).contains(&at.z), "{} left the length", at.z);
        let velocity = particle.velocity;
        assert!(
            velocity.x == 0.0 && velocity.y == 0.0 && velocity.z == 0.0,
            "OUTWARD with no Speed must give no velocity, not {velocity:?}"
        );
        lowest = lowest.min(at.z);
        highest = highest.max(at.z);
    }
    assert!(
        lowest < -36.0 && highest > 36.0,
        "the axis was only covered over {lowest}..{highest} of -37.5..37.5"
    );
}

#[test]
fn an_outward_velocity_reads_the_volume_it_is_paired_with() {
    let ring = emission(
        "FXParticleSystem Ring\n\
             \x20 System\n    Lifetime = 10 10\n  End\n\
             \x20 EmissionVolume = CylinderEmissionVolume\n\
             \x20   Radius = 50\n    Length = 120\n    IsHollow = Yes\n  End\n\
             \x20 EmissionVelocity = OutwardEmissionVelocity\n\
             \x20   Speed = 4 4\n    OtherSpeed = 1 1\n  End\n\
             End\n",
    );
    let mut rng = ClientRng::new(44);
    for _ in 0..500 {
        let particle = birth(&ring, &Placement::at(Vec3::ZERO), &mut rng);
        let flat = Vec3::new(particle.position.x, particle.position.y, 0.0);
        let velocity = particle.velocity;
        assert!((velocity.z - 1.0).abs() < 1e-4, "OtherSpeed is not on Z");
        let speed = Vec3::new(velocity.x, velocity.y, 0.0).length();
        assert!((speed - 4.0).abs() < 1e-3, "{speed} is not Speed");
        let dot = (flat.x * velocity.x + flat.y * velocity.y) / (flat.length() * speed);
        assert!(dot > 0.999, "the velocity is not outward: {dot}");
    }

    let axis = emission(
        "FXParticleSystem Axis\n\
             \x20 System\n    Lifetime = 10 10\n  End\n\
             \x20 EmissionVolume = CylinderEmissionVolume\n    Length = 20\n  End\n\
             \x20 EmissionVelocity = OutwardEmissionVelocity\n\
             \x20   Speed = 4 4\n    OtherSpeed = 1 1\n  End\n\
             End\n",
    );
    let velocity = birth(&axis, &Placement::at(Vec3::ZERO), &mut rng).velocity;
    assert_eq!(
        velocity,
        Vec3::new(0.0, 0.0, 1.0),
        "an axis birth was pushed"
    );

    let burst = emission(
        "FXParticleSystem Burst\n\
             \x20 System\n    Lifetime = 10 10\n  End\n\
             \x20 EmissionVelocity = OutwardEmissionVelocity\n\
             \x20   Speed = 3 3\n  End\n\
             End\n",
    );
    let mut up = 0u32;
    for _ in 0..500 {
        let velocity = birth(&burst, &Placement::at(Vec3::ZERO), &mut rng).velocity;
        assert!((velocity.length() - 3.0).abs() < 1e-3);
        up += u32::from(velocity.z > 0.0);
    }
    assert!(
        (100..400).contains(&up),
        "not a sphere: {up} of 500 point up"
    );
}

#[test]
fn the_spherical_family_splits_its_speeds_the_way_the_names_say() {
    let dome = emission(
        "FXParticleSystem Dome\n\
             \x20 System\n    Lifetime = 10 10\n  End\n\
             \x20 EmissionVelocity = HemisphericalEmissionVelocity\n\
             \x20   Speed = 5 5\n  End\n\
             End\n",
    );
    let mut rng = ClientRng::new(55);
    for _ in 0..1_000 {
        let velocity = birth(&dome, &Placement::at(Vec3::ZERO), &mut rng).velocity;
        assert!(velocity.z >= 0.0, "a hemisphere pointed down: {velocity:?}");
        assert!((velocity.length() - 5.0).abs() < 1e-3);
    }

    let tube = emission(
        "FXParticleSystem Tube\n\
             \x20 System\n    Lifetime = 10 10\n  End\n\
             \x20 EmissionVelocity = CylindricalEmissionVelocity\n\
             \x20   Radial = 2 2\n    Normal = 7 7\n  End\n\
             End\n",
    );
    for _ in 0..1_000 {
        let velocity = birth(&tube, &Placement::at(Vec3::ZERO), &mut rng).velocity;
        assert!((velocity.z - 7.0).abs() < 1e-4, "Normal is not on Z");
        let radial = Vec3::new(velocity.x, velocity.y, 0.0).length();
        assert!((radial - 2.0).abs() < 1e-3, "Radial is not in XY");
    }
}

#[test]
fn a_line_volume_spreads_births_along_the_line() {
    let line = emission(
        "FXParticleSystem Line\n\
             \x20 System\n    Lifetime = 10 10\n  End\n\
             \x20 EmissionVolume = LineEmissionVolume\n\
             \x20   StartPoint = X:0 Y:0 Z:0\n    EndPoint = X:0 Y:0 Z:100\n  End\n\
             End\n",
    );
    let mut rng = ClientRng::new(4);
    let mut low = f32::MAX;
    let mut high = f32::MIN;
    for _ in 0..2_000 {
        let z = birth(&line, &Placement::at(Vec3::new(5.0, 6.0, 7.0)), &mut rng)
            .position
            .z;
        assert!((7.0..=107.0).contains(&z), "{z} left the line");
        low = low.min(z);
        high = high.max(z);
    }
    assert!(low < 12.0 && high > 102.0, "covered {low}..{high}");

    let point = emission("FXParticleSystem Point\n  System\n    Lifetime = 10 10\n  End\nEnd\n");
    for _ in 0..100 {
        let at = birth(&point, &Placement::at(Vec3::new(1.0, 2.0, 3.0)), &mut rng).position;
        assert_eq!(at.to_array(), [1.0, 2.0, 3.0]);
    }
}

fn world(text: &str) -> (ParticleWorld, TemplateHandle) {
    let mut world = ParticleWorld::new(11);
    let handle = world.register(&definition(text), None);
    (world, handle)
}

#[test]
fn a_system_emits_a_burst_every_burst_delay_frames() {
    let (mut world, handle) = world(
        "FXParticleSystem Steady
  System
    Lifetime = 20 20
    BurstCount = 3 3
                 BurstDelay = 5 5
  End
End
",
    );
    world.spawn(handle, Vec3::ZERO);
    assert_eq!(world.live_particles(), 0, "nothing until the first step");
    world.warm_up(1.0 / 30.0, &NoGround);
    assert_eq!(world.live_particles(), 3, "the first burst is on frame 1");
    world.warm_up(18.0 / 30.0, &NoGround);
    assert_eq!(world.live_particles(), 12, "four bursts alive at frame 19");
    world.warm_up(1.0 / 30.0, &NoGround);
    assert_eq!(world.live_particles(), 9);
    world.warm_up(10.0, &NoGround);
    assert_eq!(world.live_particles(), 9);
    assert_eq!(world.live_systems(), 1);
}

#[test]
fn start_size_rate_makes_each_new_particle_a_little_bigger_than_the_last() {
    let (mut world, handle) = world(
        "FXParticleSystem Growing
  System
    Lifetime = 100 100
    Size = 4 4
    BurstCount = 1 1
                 BurstDelay = 0 0
    StartSizeRate = 2 2
  End
End
",
    );
    world.spawn(handle, Vec3::ZERO);
    let mut sizes = Vec::new();
    for _ in 0..5 {
        world.warm_up(1.0 / 30.0, &NoGround);
        let system = world.systems.last().expect("one system");
        let newest = system.particles.last().expect("a particle was born");
        sizes.push(newest.size);
    }
    assert_eq!(sizes, vec![4.0, 6.0, 8.0, 10.0, 12.0]);
}

#[test]
fn start_size_rate_clamps_at_the_max_bonus() {
    let (mut world, handle) = world(
        "FXParticleSystem Ballooning
  System
    Lifetime = 1000 1000
    Size = 0 0
    BurstCount = 1 1
                 BurstDelay = 0 0
    StartSizeRate = 30 30
  End
End
",
    );
    world.spawn(handle, Vec3::ZERO);
    for _ in 0..4 {
        world.warm_up(1.0 / 30.0, &NoGround);
    }
    let system = world.systems.last().expect("one system");
    let newest = system.particles.last().expect("a particle was born");
    assert_eq!(
        newest.size, MAX_SIZE_BONUS,
        "the running bonus must not exceed EA's clamp"
    );
}

#[test]
fn no_start_size_rate_means_no_extra_rng_draw() {
    let (mut plain, handle) = world(
        "FXParticleSystem Plain
  System
    Lifetime = 20 20
    Size = 4 4
    BurstCount = 1 1
                 BurstDelay = 0 0
  End
End
",
    );
    plain.spawn(handle, Vec3::ZERO);
    for _ in 0..3 {
        plain.warm_up(1.0 / 30.0, &NoGround);
    }
    let system = plain.systems.last().expect("one system");
    for particle in &system.particles {
        assert_eq!(particle.size, 4.0, "no StartSizeRate, no accumulated bonus");
    }
}

#[test]
fn system_lifetime_stops_emission_and_leaves_the_particles_alive() {
    let (mut world, handle) = world(
        "FXParticleSystem Burst
  System
    Lifetime = 100 100
    BurstCount = 30 30
                 SystemLifetime = 1
  End
End
",
    );
    world.spawn(handle, Vec3::ZERO);
    world.warm_up(1.0, &NoGround);
    assert_eq!(world.live_particles(), 30, "one burst, 30 frames later");
    world.warm_up(2.0, &NoGround);
    assert_eq!(world.live_particles(), 30, "still one burst at 90 frames");
    world.warm_up(1.0, &NoGround);
    assert_eq!(world.live_particles(), 0);
    assert_eq!(world.live_systems(), 0, "and the system is reaped");
}

fn burst_world(extra: &str) -> (ParticleWorld, TemplateHandle) {
    world(&format!(
        "FXParticleSystem Pop\n  System\n    Lifetime = 1000 1000\n    BurstCount = 4 4\n{extra}  End\nEnd\n"
    ))
}

#[test]
fn a_one_shot_with_no_delay_bursts_every_update_for_its_system_lifetime() {
    let (mut world, handle) = burst_world("    IsOneShot = Yes\n    SystemLifetime = 5\n");
    world.spawn(handle, Vec3::ZERO);
    for _ in 0..10 {
        world.warm_up(1.0 / 30.0, &NoGround);
    }
    assert_eq!(world.live_particles(), 20, "five bursts of four");
}

#[test]
fn a_one_shot_with_a_delay_bursts_once_and_stays_alive() {
    let (mut world, handle) = burst_world("    IsOneShot = Yes\n    BurstDelay = 5 5\n");
    world.spawn(handle, Vec3::ZERO);
    for _ in 0..40 {
        world.warm_up(1.0 / 30.0, &NoGround);
    }
    assert_eq!(world.live_particles(), 4);
    assert_eq!(world.live_systems(), 1);
}

#[test]
fn a_fractional_burst_delay_is_truncated_to_zero() {
    for (extra, updates, expected) in [
        (
            "    IsOneShot = Yes\n    SystemLifetime = 5\n    BurstDelay = 0 1\n",
            10,
            20,
        ),
        ("    BurstDelay = 0.8 0.8\n", 4, 16),
    ] {
        let (mut world, handle) = burst_world(extra);
        world.spawn(handle, Vec3::ZERO);
        for _ in 0..updates {
            world.warm_up(1.0 / 30.0, &NoGround);
        }
        assert_eq!(world.live_particles(), expected, "{extra}");
    }
}

#[test]
fn an_initial_delay_holds_the_whole_system_back() {
    let (mut world, handle) = world(
        "FXParticleSystem Late
  System
    Lifetime = 50 50
    BurstCount = 2 2
                 InitialDelay = 20 20
  End
End
",
    );
    world.spawn(handle, Vec3::ZERO);
    world.warm_up(19.0 / 30.0, &NoGround);
    assert_eq!(world.live_particles(), 0, "still waiting at frame 19");
    world.warm_up(2.0 / 30.0, &NoGround);
    assert!(world.live_particles() > 0, "emitting by frame 21");
}

#[test]
fn a_system_past_its_batch_refuses_the_surplus_and_counts_it() {
    let (mut world, handle) = world(
        "FXParticleSystem Flood
  System
    Lifetime = 10000 10000
    BurstCount = 200              200
  End
End
",
    );
    assert_eq!(world.capacity(handle), Some(MAX_PARTICLES_PER_SYSTEM));
    world.spawn(handle, Vec3::ZERO);
    world.warm_up(1.0, &NoGround);
    assert_eq!(world.live_particles(), MAX_PARTICLES_PER_SYSTEM);
    assert!(
        world.dropped() > 0,
        "the surplus is counted, not grown into"
    );
}

fn steady(name: &str, priority: &str) -> String {
    format!(
        "FXParticleSystem {name}
  System
    Priority = {priority}
    Lifetime = 10000 10000
    BurstCount = 1 1
    Size = 1 1
  End
End
"
    )
}

#[test]
fn the_ceiling_holds_and_destroys_the_oldest_particle_first() {
    let mut world = ParticleWorld::new(3);
    let handle = world.register(&definition(&steady("Dust", "VERY_LOW_OR_ABOVE")), None);
    world.set_budget(Budget {
        max_particles: Some(20),
        skip_mask: 0,
        detail: DetailLevel::Off,
    });
    world.spawn(handle, Vec3::ZERO);
    world.warm_up(2.0, &NoGround);

    let report = world.refusals();
    assert_eq!(report.counted, 20, "the ceiling is held exactly");
    assert!(report.evicted > 30, "60 births into 20 slots: {report:?}");
    let oldest = world
        .systems
        .iter()
        .flat_map(|system| system.particles.iter())
        .map(|particle| particle.age)
        .max()
        .unwrap();
    assert!(
        oldest <= 20,
        "the survivors should be the newest twenty; the oldest is {oldest} frames old"
    );
}

#[test]
fn the_ceiling_takes_from_the_system_holding_the_oldest_particle() {
    let mut world = ParticleWorld::new(3);
    let old = world.register(&definition(&steady("Old", "VERY_LOW_OR_ABOVE")), None);
    let new = world.register(&definition(&steady("New", "VERY_LOW_OR_ABOVE")), None);
    world.set_budget(Budget {
        max_particles: Some(20),
        skip_mask: 0,
        detail: DetailLevel::Off,
    });
    world.spawn(old, Vec3::ZERO);
    world.warm_up(1.0, &NoGround);
    assert_eq!(world.live_particles(), 20, "the ceiling is full");

    world.spawn(new, Vec3::ZERO);
    world.warm_up(0.5, &NoGround);

    let held: Vec<usize> = world
        .systems
        .iter()
        .map(|system| system.particles.len())
        .collect();
    assert_eq!(held.iter().sum::<usize>(), 20, "still exactly the ceiling");
    assert!(
        held[1] >= 7,
        "the newcomer should have grown into the room it made: {held:?}"
    );
    assert!(
        held[0] <= 13,
        "and the system holding the oldest particles is the one that paid: {held:?}"
    );
}

#[test]
fn a_lower_priority_particle_is_refused_rather_than_evicting_a_higher_one() {
    let mut world = ParticleWorld::new(3);
    let keep = world.register(&definition(&steady("Keep", "VERY_LOW_OR_ABOVE")), None);
    let drop = world.register(&definition(&steady("Drop", "HIGH_OR_ABOVE")), None);
    world.set_budget(Budget {
        max_particles: Some(20),
        skip_mask: 0,
        detail: DetailLevel::Off,
    });
    world.spawn(keep, Vec3::ZERO);
    world.warm_up(1.0, &NoGround);
    assert_eq!(world.live_particles(), 20, "the ceiling is full");

    world.spawn(drop, Vec3::ZERO);
    world.warm_up(1.0, &NoGround);

    let counts: Vec<(ParticlePriority, usize)> = world
        .systems
        .iter()
        .map(|system| (system.priority, system.particles.len()))
        .collect();
    assert_eq!(
        counts
            .iter()
            .find(|(priority, _)| *priority == ParticlePriority::HighOrAbove)
            .map(|(_, held)| *held),
        Some(0),
        "a HIGH_OR_ABOVE particle may not evict a VERY_LOW_OR_ABOVE one: {counts:?}"
    );
    assert_eq!(
        counts
            .iter()
            .find(|(priority, _)| *priority == ParticlePriority::VeryLowOrAbove)
            .map(|(_, held)| *held),
        Some(20),
        "and the system that was already there keeps every particle"
    );
    assert!(world.refusals().refused > 0, "the refusals are counted");
}

#[test]
fn an_always_render_system_is_neither_counted_nor_evicted() {
    let mut world = ParticleWorld::new(4);
    let exempt = world.register(&definition(&steady("Torch", "ALWAYS_RENDER")), None);
    let ordinary = world.register(
        &definition(
            "FXParticleSystem Dust
  System
    Priority = VERY_LOW_OR_ABOVE
    Lifetime = 10000 10000
    SystemLifetime = 10
    BurstCount = 1 1
    Size = 1 1
  End
End
",
        ),
        None,
    );
    world.set_budget(Budget {
        max_particles: Some(10),
        skip_mask: 0,
        detail: DetailLevel::Off,
    });
    world.spawn(exempt, Vec3::ZERO);
    world.spawn(ordinary, Vec3::ZERO);
    world.warm_up(1.0, &NoGround);

    let report = world.refusals();
    assert_eq!(report.counted, 10, "the ordinary system keeps its ten");
    assert!(
        report.exempt >= 25,
        "the exempt system kept growing past the ceiling: {} live",
        report.exempt
    );
    assert_eq!(
        world.live_particles(),
        report.counted + report.exempt,
        "every live particle is in exactly one of the two halves"
    );
    assert_eq!(
        report.evicted, 0,
        "an ALWAYS_RENDER system must not cost another system a particle"
    );
}

#[test]
fn the_skip_mask_thins_creation_by_the_factor_gamelod_asks_for() {
    let run = |mask: u32, priority: &str| {
        let mut world = ParticleWorld::new(7);
        let handle = world.register(&definition(&steady("Dust", priority)), None);
        world.set_budget(Budget {
            max_particles: None,
            skip_mask: mask,
            detail: DetailLevel::Off,
        });
        world.spawn(handle, Vec3::ZERO);
        world.warm_up(2.0, &NoGround);
        world.live_particles()
    };
    let all = run(0, "VERY_LOW_OR_ABOVE");
    assert!(all >= 55, "one a frame for 60 frames: {all}");
    assert_eq!(
        run(1, "VERY_LOW_OR_ABOVE"),
        all.div_euclid(2),
        "mask 1 is every second"
    );
    assert_eq!(
        run(3, "VERY_LOW_OR_ABOVE"),
        all.div_euclid(4),
        "mask 3 is every fourth"
    );
    assert_eq!(
        run(3, "ALWAYS_RENDER"),
        all,
        "an ALWAYS_RENDER system is not thinned by the skip mask"
    );
}

#[test]
fn the_detail_floor_refuses_the_system_and_says_that_is_what_it_did() {
    let mut world = ParticleWorld::new(9);
    let needs_high = world.register(&definition(&steady("Rare", "HIGH_OR_ABOVE")), None);
    let always = world.register(&definition(&steady("Common", "VERY_LOW_OR_ABOVE")), None);
    world.set_budget(Budget {
        max_particles: None,
        skip_mask: 0,
        detail: DetailLevel::VeryLow,
    });
    assert_eq!(world.spawn(needs_high, Vec3::ZERO), None);
    assert!(world.spawn(always, Vec3::ZERO).is_some());
    assert_eq!(world.refusals().lod_refused, 1);
    assert_eq!(world.live_systems(), 1);

    let mut budget = world.budget();
    budget.detail = DetailLevel::UltraHigh;
    world.set_budget(budget);
    assert!(world.spawn(needs_high, Vec3::ZERO).is_some());
    assert_eq!(world.live_systems(), 2);
}

#[test]
fn the_step_is_fixed_and_a_long_frame_is_capped() {
    let mut world = ParticleWorld::new(5);
    assert_eq!(world.advance(1.0 / 60.0, &NoGround), 0, "half a step waits");
    assert_eq!(
        world.advance(1.0 / 60.0, &NoGround),
        1,
        "the halves make one"
    );
    assert_eq!(world.advance(3600.0, &NoGround), MAX_STEPS_PER_CALL);
    assert_eq!(world.advance(f32::NAN, &NoGround), 0);
    assert_eq!(world.advance(-1.0, &NoGround), 0);
}

fn plain_nugget(template: TemplateHandle) -> FxSpawn {
    FxSpawn {
        template,
        offset: Vec3::ZERO,
        delay: RandomRange::default(),
        at_ground_height: false,
        orient_to_object: false,
        attach_to_object: false,
        set_target_matrix: false,
    }
}

fn compound_source(name: &str, life: u32, extra: &str) -> String {
    format!(
        "FXParticleSystem {name}\n  System\n    ParticleName = EX.tga\n    \
             Lifetime = {life} {life}\n    Size = 1 1\n    BurstCount = 1 1\n{extra}  End\n\
             \x20 Physics = DefaultPhysics\n    VelocityDamping = 1 1\n  End\n\
             \x20 EmissionVolume = PointEmissionVolume\n  End\n\
             \x20 EmissionVelocity = OrthoEmissionVelocity\n  End\nEnd\n"
    )
}

fn two(a: &str, b: &str) -> (ParticleSystemDef, ParticleSystemDef) {
    let table = BlockTable::particle_path();
    let text = format!("{a}{b}");
    let document = Document::parse("test.ini", &text, &table).unwrap();
    assert!(document.is_balanced(), "{:?}", document.diagnostics());
    let mut index = ParticleIndex::new();
    index.insert_document(&document);
    let mut held = index.iter().map(|(_, def)| def.clone());
    let (first, second) = (held.next().unwrap(), held.next().unwrap());
    if first.name.eq_ignore_ascii_case("master") {
        (first, second)
    } else {
        (second, first)
    }
}

#[test]
fn a_slave_emits_one_particle_per_master_particle_and_none_of_its_own() {
    let (master, slave) = two(
        &compound_source("master", 10, "    SlaveSystem = slave\n"),
        &compound_source("slave", 10, ""),
    );
    let mut world = ParticleWorld::new(3);
    let slave_handle = world.register(&slave, None);
    let master_handle = world.register(&master, None);
    world.link(master_handle, Some(slave_handle), None, None);

    world.spawn(master_handle, Vec3::ZERO).unwrap();
    assert_eq!(world.live_systems(), 2, "the slave starts with its master");
    world.warm_up(5.0 / 30.0, &NoGround);
    assert_eq!(world.live_particles(), 10);

    let mut alone = ParticleWorld::new(3);
    let alone_slave = alone.register(&slave, None);
    alone.spawn(alone_slave, Vec3::ZERO).unwrap();
    alone.warm_up(5.0 / 30.0, &NoGround);
    assert_eq!(
        alone.live_particles(),
        5,
        "a slave template spawned on its own is an ordinary system"
    );
}

#[test]
fn removing_or_stopping_a_master_takes_its_slave() {
    let (master, slave) = two(
        &compound_source("master", 10, "    SlaveSystem = slave\n"),
        &compound_source("slave", 10, ""),
    );
    let mut world = ParticleWorld::new(3);
    let slave_handle = world.register(&slave, None);
    let master_handle = world.register(&master, None);
    world.link(master_handle, Some(slave_handle), None, None);

    let id = world.spawn(master_handle, Vec3::ZERO).unwrap();
    world.warm_up(3.0 / 30.0, &NoGround);
    assert_eq!(world.live_systems(), 2);
    world.stop(id);
    world.warm_up(20.0 / 30.0, &NoGround);
    assert_eq!(world.live_systems(), 0, "both died out");

    let id = world.spawn(master_handle, Vec3::ZERO).unwrap();
    world.warm_up(3.0 / 30.0, &NoGround);
    assert_eq!(world.live_systems(), 2);
    assert!(world.remove(id));
    assert_eq!(world.live_systems(), 0, "remove takes the slave too");
}

#[test]
fn every_particle_starts_an_attached_system_that_follows_and_then_stops() {
    let master = compound_source(
        "master",
        4,
        "    PerParticleAttachedSystem = trail\n    SystemLifetime = 3\n",
    )
    .replace(
        "  EmissionVelocity = OrthoEmissionVelocity\n  End",
        "  EmissionVelocity = OrthoEmissionVelocity\n    X = 1 1\n  End",
    );
    let (master, trail) = two(&master, &compound_source("trail", 2, ""));
    let mut world = ParticleWorld::new(7);
    let trail_handle = world.register(&trail, None);
    let master_handle = world.register(&master, None);
    world.link(master_handle, None, Some(trail_handle), None);

    world.spawn(master_handle, Vec3::ZERO).unwrap();
    world.warm_up(3.0 / 30.0, &NoGround);
    assert_eq!(world.live_systems(), 4);

    world.warm_up(1.0 / 30.0, &NoGround);
    let trail_slot = world
        .templates
        .iter()
        .position(|held| held.definition.name.eq_ignore_ascii_case("trail"))
        .unwrap();
    let followed: Vec<f32> = world
        .systems
        .iter()
        .filter(|system| system.template == trail_slot)
        .map(|system| system.origin.x)
        .collect();
    assert!(
        followed.iter().copied().fold(f32::MIN, f32::max) >= 3.0,
        "an attached system moved with its particle: {followed:?}"
    );
    assert!(
        followed.iter().any(|x| *x > 0.0),
        "and did not stay at the master's origin"
    );

    world.warm_up(60.0 / 30.0, &NoGround);
    assert_eq!(world.live_systems(), 0);
}

#[test]
fn an_attached_system_emits_in_its_carriers_axes_only_in_carrier_mode() {
    let master = compound_source(
        "master",
        4,
        "    PerParticleAttachedSystem = trail
",
    );
    let trail = compound_source("trail", 2, "").replace(
        "  EmissionVolume = PointEmissionVolume
  End",
        "  EmissionVolume = LineEmissionVolume
    StartPoint = X:0 Y:3 Z:0
             EndPoint = X:0 Y:3 Z:0
  End",
    );
    let (master, trail) = two(&master, &trail);
    let quarter_turn = Mat4::from_cols([
        [0.0, 1.0, 0.0, 0.0],
        [-1.0, 0.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ]);
    let born = |frame: AttachedFrame| {
        let mut world = ParticleWorld::new(7);
        let trail_handle = world.register(&trail, None);
        let master_handle = world.register(&master, None);
        world.link(master_handle, None, Some(trail_handle), None);
        world.set_attached_frame(frame);
        world.fire(
            &[FxSpawn {
                template: master_handle,
                offset: Vec3::ZERO,
                delay: RandomRange::default(),
                at_ground_height: false,
                orient_to_object: true,
                attach_to_object: false,
                set_target_matrix: false,
            }],
            FxFrame {
                at: Vec3::ZERO,
                matrix: Some(quarter_turn),
                secondary: None,
            },
            &NoGround,
        );
        world.warm_up(2.0 / 30.0, &NoGround);
        let trail_slot = world
            .templates
            .iter()
            .position(|held| held.definition.name.eq_ignore_ascii_case("trail"))
            .unwrap();
        world
            .systems
            .iter()
            .find(|system| system.template == trail_slot)
            .unwrap()
            .particles
            .first()
            .unwrap()
            .previous
    };
    assert_eq!(born(AttachedFrame::World), Vec3::new(0.0, 3.0, 0.0));
    assert_eq!(born(AttachedFrame::Carrier), Vec3::new(-3.0, 0.0, 0.0));
}

#[test]
fn the_attached_frame_default_is_world_axes() {
    assert_eq!(AttachedFrame::default(), AttachedFrame::World);
}

#[test]
fn a_slave_cycle_stops_at_the_compound_depth_cap() {
    let definition = definition(&compound_source("loop", 10, "    SlaveSystem = loop\n"));
    let mut world = ParticleWorld::new(5);
    let handle = world.register(&definition, None);
    world.link(handle, Some(handle), None, None);
    world.spawn(handle, Vec3::ZERO).unwrap();
    assert_eq!(
        world.live_systems(),
        usize::try_from(MAX_COMPOUND_DEPTH)
            .unwrap()
            .saturating_add(1)
    );
    assert_eq!(world.refusals().depth_refused, 1);
}

#[test]
fn the_compound_rung_off_starts_none_of_step_fourteen() {
    let (master, slave) = two(
        &compound_source(
            "master",
            10,
            "    SlaveSystem = slave\n    PerParticleAttachedSystem = slave\n",
        ),
        &compound_source("slave", 10, ""),
    );
    let mut world = ParticleWorld::new(3);
    let slave_handle = world.register(&slave, None);
    let master_handle = world.register(&master, None);
    world.link(
        master_handle,
        Some(slave_handle),
        Some(slave_handle),
        Some(CollisionEvent {
            fires: vec![plain_nugget(slave_handle)],
            height_offset: 0.0,
            kill_after: true,
            follow_up: None,
            orient: false,
        }),
    );
    world.set_compound(false);
    world.spawn(master_handle, Vec3::ZERO).unwrap();
    world.warm_up(3.0 / 30.0, &FlatGround(0.0));
    assert_eq!(world.live_systems(), 1, "no slave and no attachment");
    assert_eq!(world.live_particles(), 3, "and nothing was killed");
    assert_eq!(world.refusals().collisions, 0);
}

#[test]
fn a_particle_that_reaches_the_ground_fires_its_event_once() {
    let falling = compound_source("master", 60, "").replace(
        "  EmissionVelocity = OrthoEmissionVelocity\n  End",
        "  EmissionVelocity = OrthoEmissionVelocity\n    Z = -1 -1\n  End",
    );
    let (master, effect) = two(&falling, &compound_source("slave", 30, ""));
    let mut world = ParticleWorld::new(9);
    let fired = world.register(&effect, None);
    let handle = world.register(&master, None);
    world.link(
        handle,
        None,
        None,
        Some(CollisionEvent {
            fires: vec![plain_nugget(fired)],
            height_offset: 0.0,
            kill_after: true,
            follow_up: None,
            orient: false,
        }),
    );
    world.spawn(handle, Vec3::new(0.0, 0.0, 5.0)).unwrap();
    world.warm_up(4.0 / 30.0, &NoGround);
    assert_eq!(world.refusals().collisions, 0, "NoGround never collides");
    world.warm_up(4.0 / 30.0, &FlatGround(0.0));
    assert!(world.refusals().collisions > 0, "the ground was reached");
    assert!(
        world.live_systems() > 1,
        "the event started what its FXList named"
    );

    let mut high = ParticleWorld::new(9);
    let fired = high.register(&effect, None);
    let handle = high.register(&master, None);
    high.link(
        handle,
        None,
        None,
        Some(CollisionEvent {
            fires: vec![plain_nugget(fired)],
            height_offset: 4.0,
            kill_after: true,
            follow_up: None,
            orient: false,
        }),
    );
    high.spawn(handle, Vec3::new(0.0, 0.0, 5.0)).unwrap();
    high.warm_up(2.0 / 30.0, &FlatGround(0.0));
    assert!(
        high.refusals().collisions > 0,
        "the offset raised the plane"
    );
}

#[test]
fn a_collision_reports_its_follow_up_to_the_caller() {
    let falling = compound_source("master", 60, "").replace(
        "  EmissionVelocity = OrthoEmissionVelocity\n  End",
        "  EmissionVelocity = OrthoEmissionVelocity\n    X = 2 2\n    Z = -1 -1\n  End",
    );
    let (master, _) = two(&falling, &compound_source("slave", 30, ""));
    let mut world = ParticleWorld::new(9);
    let handle = world.register(&master, None);
    world.link(
        handle,
        None,
        None,
        Some(CollisionEvent {
            fires: Vec::new(),
            height_offset: 0.0,
            kill_after: false,
            follow_up: Some(7),
            orient: true,
        }),
    );
    world.spawn(handle, Vec3::new(0.0, 0.0, 5.0)).unwrap();
    world.warm_up(8.0 / 30.0, &FlatGround(0.0));
    let mut taken = Vec::new();
    world.take_collisions(&mut taken);
    let first = taken.first().expect("the ground was reached");
    assert_eq!(first.follow_up, 7);
    assert!(first.at.z <= 0.0);
    let [x, y] = first.heading.expect("oriented to the terrain");
    assert!(x > 0.0 && y.abs() < 1e-6, "heading along +x, got {x},{y}");
    let count = taken.len();
    world.take_collisions(&mut taken);
    assert_eq!(taken.len(), count, "taken once");
}

#[test]
fn a_particle_that_survives_its_event_does_not_fire_it_again() {
    let resting = compound_source("master", 60, "");
    let (master, effect) = two(&resting, &compound_source("slave", 5, ""));
    let mut world = ParticleWorld::new(9);
    let fired = world.register(&effect, None);
    let handle = world.register(&master, None);
    world.link(
        handle,
        None,
        None,
        Some(CollisionEvent {
            fires: vec![plain_nugget(fired)],
            height_offset: 0.0,
            kill_after: false,
            follow_up: None,
            orient: false,
        }),
    );
    world.spawn(handle, Vec3::ZERO).unwrap();
    world.warm_up(1.0 / 30.0, &FlatGround(0.0));
    assert_eq!(world.refusals().collisions, 1);
    world.warm_up(1.0 / 30.0, &FlatGround(0.0));
    assert_eq!(
        world.refusals().collisions,
        2,
        "one per particle, not one per particle per frame"
    );
    world.warm_up(1.0 / 30.0, &FlatGround(0.0));
    assert_eq!(world.refusals().collisions, 3);
}

#[test]
fn the_slot_map_survives_a_system_ending_under_another_one() {
    let one_shot = compound_source("master", 1, "    IsOneShot = Yes\n    SystemLifetime = 1\n");
    let (short, long) = two(&one_shot, &compound_source("slave", 400, ""));
    let mut world = ParticleWorld::new(13);
    let short_handle = world.register(&short, None);
    let long_handle = world.register(&long, None);
    let doomed = world.spawn(short_handle, Vec3::ZERO).unwrap();
    let survivor = world.spawn(long_handle, Vec3::ZERO).unwrap();
    world.warm_up(4.0 / 30.0, &NoGround);
    assert_eq!(world.live_systems(), 1, "the one-shot has gone");
    assert!(
        !world.remove(doomed),
        "a system that has ended is not still in the map"
    );
    assert!(
        world.set_origin(survivor, Vec3::new(1.0, 2.0, 3.0)),
        "and the survivor is still reachable at its new slot"
    );
    assert_eq!(
        world
            .systems
            .first()
            .map(|system| system.origin)
            .unwrap_or(Vec3::ZERO),
        Vec3::new(1.0, 2.0, 3.0)
    );
}

#[test]
fn a_slaves_particles_are_counted_against_the_same_ceiling() {
    let (master, slave) = two(
        &compound_source("master", 400, "    SlaveSystem = slave\n"),
        &compound_source("slave", 400, ""),
    );
    let mut world = ParticleWorld::new(3);
    let slave_handle = world.register(&slave, None);
    let master_handle = world.register(&master, None);
    world.link(master_handle, Some(slave_handle), None, None);
    world.set_budget(Budget {
        max_particles: Some(20),
        ..Budget::default()
    });
    world.spawn(master_handle, Vec3::ZERO).unwrap();
    world.warm_up(60.0 / 30.0, &NoGround);
    assert_eq!(
        world.refusals().counted,
        20,
        "the pair together are held at the ceiling"
    );
}

fn burst_source(name: &str, life: u32, burst_count: u32, burst_delay: u32, extra: &str) -> String {
    format!(
        "FXParticleSystem {name}\n  System\n    ParticleName = EX.tga\n    \
             Lifetime = {life} {life}\n    Size = 1 1\n    BurstCount = {burst_count} {burst_count}\n    \
             BurstDelay = {burst_delay} {burst_delay}\n{extra}  End\n\
             \x20 EmissionVolume = PointEmissionVolume\n  End\n\
             \x20 EmissionVelocity = OrthoEmissionVelocity\n  End\nEnd\n"
    )
}

#[test]
fn a_slaves_batch_is_sized_from_its_masters_arrival_rate() {
    let (master, slave) = two(
        &burst_source("master", 30, 3, 0, "    SlaveSystem = slave\n"),
        &burst_source("slave", 30, 1, 4, ""),
    );
    let mut world = ParticleWorld::new(3);
    let slave_handle = world.register(&slave, None);
    let master_handle = world.register(&master, None);
    world.link(master_handle, Some(slave_handle), None, None);

    world.spawn(master_handle, Vec3::ZERO).unwrap();
    world.warm_up(20.0 / 30.0, &NoGround);
    let slave_slot = world
        .templates
        .iter()
        .position(|held| held.definition.name.eq_ignore_ascii_case("slave"))
        .unwrap();
    let slave_live: usize = world
        .systems
        .iter()
        .filter(|system| system.template == slave_slot)
        .map(|system| system.particles.len())
        .sum();
    assert_eq!(
        slave_live, 60,
        "3 a frame for 20 frames, a 30-frame lifetime so none are dead yet"
    );
    assert_eq!(world.dropped(), 0, "sized to hold every arrival");
}

#[test]
fn a_chained_slaves_batch_follows_the_top_master_and_a_second_master_cannot_shrink_it() {
    let (top, slow) = two(
        &burst_source(
            "master",
            30,
            3,
            0,
            "    SlaveSystem = middle
",
        ),
        &burst_source(
            "slow",
            30,
            1,
            10,
            "    SlaveSystem = middle
",
        ),
    );
    let (bottom, middle) = two(
        &burst_source("bottom", 30, 1, 10, ""),
        &burst_source(
            "middle",
            30,
            1,
            10,
            "    SlaveSystem = bottom
",
        ),
    );
    let mut world = ParticleWorld::new(3);
    let bottom_handle = world.register(&bottom, None);
    let middle_handle = world.register(&middle, None);
    let top_handle = world.register(&top, None);
    let slow_handle = world.register(&slow, None);
    world.link(middle_handle, Some(bottom_handle), None, None);
    world.link(top_handle, Some(middle_handle), None, None);
    world.link(slow_handle, Some(middle_handle), None, None);

    assert_eq!(world.capacity(middle_handle), Some(93));
    assert_eq!(world.capacity(bottom_handle), Some(93));
}

fn wind_source(name: &str, wind: &str) -> String {
    format!(
        "FXParticleSystem {name}
  System
    Lifetime = 100 100
    BurstCount = 1 1
    Size = 1 1
  End
  Wind = DefaultWind
{wind}  End
End
"
    )
}

#[test]
fn wind_displaces_the_position_and_leaves_the_velocity_where_it_was() {
    let source = wind_source(
        "Blown",
        "    WindMotion = PingPong\n    WindStrength = 1\n    WindAngleChangeMin = 0.1\n    \
             WindAngleChangeMax = 0.1\n    WindPingPongStartAngleMin = 0\n    \
             WindPingPongStartAngleMax = 0\n    WindPingPongEndAngleMin = 0.4\n    \
             WindPingPongEndAngleMax = 0.4\n",
    );
    let (mut world, handle) = world(&source);
    world.spawn(handle, Vec3::ZERO).unwrap();
    world.warm_up(1.0, &NoGround);

    let system = world.systems.first().unwrap();
    let particle = system.particles.first().unwrap();
    assert!(
        particle.position.x > 10.0,
        "30 frames of a 1.0 * U(0.7,1.3) push should be tens of units: {:?}",
        particle.position
    );
    assert!(
        particle.position.z.abs() < f32::EPSILON,
        "no gravity and no velocity, so Z must not have moved"
    );
    assert_eq!(
        particle.velocity,
        Vec3::ZERO,
        "the wind writes the position, not the velocity"
    );
}

#[test]
fn the_wind_falls_off_linearly_between_the_two_distances() {
    assert!((wind_falloff(0.0, 75.0, 200.0) - 1.0).abs() < f32::EPSILON);
    assert!((wind_falloff(75.0, 75.0, 200.0) - 1.0).abs() < f32::EPSILON);
    assert!(
        (wind_falloff(137.5, 75.0, 200.0) - 0.5).abs() < 1e-6,
        "halfway"
    );
    assert!(wind_falloff(200.0, 75.0, 200.0).abs() < f32::EPSILON);
    assert!(wind_falloff(1e9, 75.0, 200.0).abs() < f32::EPSILON);

    assert!((wind_falloff(10.0, 100.0, 100.0) - 1.0).abs() < f32::EPSILON);
    let outside = wind_falloff(150.0, 100.0, 100.0);
    assert!(outside.abs() < f32::EPSILON, "{outside} must be a clean 0");
    assert!(wind_falloff(150.0, 200.0, 100.0).is_finite());
}

#[test]
fn the_ping_pong_sweep_stays_in_its_span_reverses_and_never_stalls() {
    let definition = definition(&wind_source(
        "Sweep",
        "    WindMotion = PingPong\n    WindAngleChangeMin = 0.1\n    WindAngleChangeMax = \
             0.1\n    WindPingPongStartAngleMin = 0\n    WindPingPongStartAngleMax = 0\n    \
             WindPingPongEndAngleMin = 1\n    WindPingPongEndAngleMax = 1\n",
    ));
    let spec = WindSpec::compile(&definition).unwrap();
    let mut rng = ClientRng::new(7);
    let mut state = WindState {
        angle: 0.5,
        change: 1.0,
        start: 0.0,
        end: 1.0,
        angle_change: WIND_ANGLE_CHANGE_DEFAULT,
    };
    let overshoot = spec
        .angle_change
        .low
        .abs()
        .max(spec.angle_change.high.abs());
    let mut reversals = 0;
    let mut last = state.change;
    let mut stalled = 0;
    for _ in 0..2_000 {
        let before = state.angle;
        advance_wind(&mut state, &spec, &mut rng);
        assert!(
            (-overshoot..=1.0 + overshoot).contains(&state.angle),
            "left the span by more than one step: {}",
            state.angle
        );
        if (state.angle - before).abs() < 1e-4 {
            stalled += 1;
        }
        if state.change != last {
            reversals += 1;
            last = state.change;
        }
    }
    assert!(
        reversals > 4,
        "only {reversals} reversal(s) in 2,000 frames"
    );
    assert!(
        reversals < 200,
        "{reversals} reversals is the angle changing its mind at an edge, not a sweep"
    );
    assert_eq!(
        stalled, 0,
        "the 0.005 floor must keep it moving every frame"
    );
}

#[test]
fn the_shipped_span_traverses_the_circle_rather_than_sticking_at_an_edge() {
    let definition = definition(&wind_source(
        "Shipped",
        "    WindMotion = PingPong
    WindStrength = 0.1
    WindAngleChangeMin =              0.149924
    WindAngleChangeMax = 0.449946
    WindPingPongStartAngleMax =              0.785398
    WindPingPongEndAngleMin = 5.49779
    WindPingPongEndAngleMax =              6.28319
",
    ));
    let spec = WindSpec::compile(&definition).unwrap();
    let mut rng = ClientRng::new(3);
    let mut state = WindState {
        angle: 0.0,
        change: 1.0,
        start: 0.0,
        end: 6.0,
        angle_change: WIND_ANGLE_CHANGE_DEFAULT,
    };
    let (mut lowest, mut highest) = (f32::MAX, f32::MIN);
    let mut reversals = 0;
    let mut last = state.change;
    for _ in 0..3_000 {
        advance_wind(&mut state, &spec, &mut rng);
        lowest = lowest.min(state.angle);
        highest = highest.max(state.angle);
        if state.change != last {
            reversals += 1;
            last = state.change;
        }
    }
    assert!(lowest < 1.5, "never came back down: lowest {lowest}");
    assert!(
        highest > 5.0,
        "never reached the far end: highest {highest}"
    );
    assert!(
        reversals >= 2,
        "{reversals} reversal(s) in 3,000 frames is not a sweep"
    );
}

#[test]
fn drawn_color_interpolates_across_the_blend_fractions_within_a_step() {
    let bare = emission("FXParticleSystem Bare\n  System\n    Lifetime = 100 100\n  End\nEnd\n");
    let mut particle = birth(&bare, &Placement::at(Vec3::ZERO), &mut ClientRng::new(1));
    particle.previous_color = [0.0, 0.0, 0.0];
    particle.color = [1.0, 0.5, 0.0];
    particle.previous_alpha = 0.0;
    particle.alpha = 1.0;
    let at = |blend: f32| particle.drawn_color(blend);
    let (start_color, start_alpha) = at(0.0);
    let (mid_color, mid_alpha) = at(0.5);
    let (end_color, end_alpha) = at(1.0);
    assert_eq!(start_color, [0.0, 0.0, 0.0]);
    assert_eq!(start_alpha, 0.0);
    assert_eq!(mid_color, [0.5, 0.25, 0.0]);
    assert_eq!(mid_alpha, 0.5);
    assert_eq!(end_color, [1.0, 0.5, 0.0]);
    assert_eq!(end_alpha, 1.0);
    assert_ne!(
        start_color, mid_color,
        "the pixels must change between blends"
    );
    assert_ne!(
        mid_color, end_color,
        "the pixels must change between blends"
    );
}

#[test]
fn the_circular_sweep_wraps_and_never_reverses() {
    let definition = definition(&wind_source(
        "Swirl",
        "    WindMotion = Circular\n    WindAngleChangeMin = 0.2\n    WindAngleChangeMax = \
             0.2\n",
    ));
    let spec = WindSpec::compile(&definition).unwrap();
    assert_eq!(spec.motion, WindMotion::Circular);
    let mut rng = ClientRng::new(7);
    let mut state = WindState::default();
    let mut wrapped = 0;
    for _ in 0..200 {
        let before = state.angle;
        advance_wind(&mut state, &spec, &mut rng);
        assert!((0.0..TWO_PI).contains(&state.angle));
        if state.angle < before {
            wrapped += 1;
        }
    }
    assert_eq!(wrapped, 6, "wrapped {wrapped} time(s)");
    assert!(state.change >= 0.0, "circular never reverses");
}

#[test]
fn the_ping_pong_speed_holds_across_a_leg_and_only_rerolls_at_a_reversal() {
    let definition = definition(&wind_source(
        "Steady",
        "    WindMotion = PingPong\n    WindAngleChangeMin = 0.01\n    WindAngleChangeMax = \
             2.0\n    WindPingPongStartAngleMin = 0\n    WindPingPongStartAngleMax = 0\n    \
             WindPingPongEndAngleMin = 100\n    WindPingPongEndAngleMax = 100\n",
    ));
    let spec = WindSpec::compile(&definition).unwrap();
    let mut rng = ClientRng::new(11);
    let mut state = WindState {
        angle: 50.0,
        change: 1.0,
        start: 0.0,
        end: 100.0,
        angle_change: rng.range(spec.angle_change).abs(),
    };
    let first = state.angle_change;
    for _ in 0..50 {
        advance_wind(&mut state, &spec, &mut rng);
        assert_eq!(
            state.angle_change, first,
            "the speed changed mid-leg, with no reversal"
        );
    }
}

#[test]
fn a_spawned_systems_wind_angle_is_not_pinned_to_the_span_start() {
    let source = wind_source(
        "Fanned",
        "    WindMotion = PingPong\n    WindAngleChangeMin = 0.1\n    WindAngleChangeMax = \
             0.1\n    WindPingPongStartAngleMin = 0\n    WindPingPongStartAngleMax = 0\n    \
             WindPingPongEndAngleMin = 6\n    WindPingPongEndAngleMax = 6\n",
    );
    let (mut world, handle) = world(&source);
    for _ in 0..8 {
        world.spawn(handle, Vec3::ZERO).unwrap();
    }
    let angles: Vec<f32> = world.systems.iter().map(|s| s.wind.angle).collect();
    assert!(
        angles.iter().any(|a| (*a - 0.0).abs() > 1e-3),
        "every system started at the span's own edge: {angles:?}"
    );
}

#[test]
fn a_wind_block_with_no_motion_changes_neither_the_picture_nor_the_stream() {
    let blocked = wind_source("Blocked", "    WindStrength = 0.5\n");
    let bare = "FXParticleSystem Bare\n  System\n    Lifetime = 100 100\n    BurstCount = 1 \
                    1\n    Size = 1 1\n  End\n  EmissionVelocity = SphericalEmissionVelocity\n    \
                    Speed = 1 3\n  End\nEnd\n";
    let with_velocity = blocked.replace(
        "  Wind = DefaultWind\n",
        "  EmissionVelocity = SphericalEmissionVelocity\n    Speed = 1 3\n  End\n  Wind = \
             DefaultWind\n",
    );

    let positions = |source: &str| {
        let (mut world, handle) = world(source);
        world.spawn(handle, Vec3::ZERO).unwrap();
        world.warm_up(2.0, &NoGround);
        world
            .systems
            .first()
            .unwrap()
            .particles
            .iter()
            .map(|p| (p.position.x, p.position.y, p.position.z, p.size))
            .collect::<Vec<_>>()
    };
    let with_block = positions(&with_velocity);
    assert!(with_block.len() > 10, "{} particles", with_block.len());
    assert_eq!(with_block, positions(bare));
}

#[test]
fn the_wind_rung_moves_the_particles_and_not_the_random_stream() {
    let source = wind_source(
        "Blown",
        "    WindMotion = PingPong\n    WindStrength = 1\n    WindAngleChangeMin = 0.1\n    \
             WindAngleChangeMax = 0.1\n    WindPingPongEndAngleMin = 0.4\n    \
             WindPingPongEndAngleMax = 0.4\n",
    )
    .replace("Size = 1 1", "Size = 1 4");
    let run = |mode: WindMode| {
        let (mut world, handle) = world(&source);
        world.set_wind(mode);
        world.spawn(handle, Vec3::ZERO).unwrap();
        world.warm_up(2.0, &NoGround);
        let system = world.systems.first().unwrap();
        let rolled: Vec<(f32, u32)> = system
            .particles
            .iter()
            .map(|p| (p.size, p.lifetime))
            .collect();
        let moved: Vec<(f32, f32)> = system
            .particles
            .iter()
            .map(|p| (p.position.x, p.position.y))
            .collect();
        (rolled, moved)
    };
    let (off_rolled, off_moved) = run(WindMode::Off);
    let (retail_rolled, retail_moved) = run(WindMode::Retail);
    let (doubled_rolled, doubled_moved) = run(WindMode::Doubled);
    assert!(off_rolled.len() > 10);
    assert_eq!(
        off_rolled, retail_rolled,
        "the stream must not depend on the rung"
    );
    assert_eq!(off_rolled, doubled_rolled);
    assert_ne!(
        off_moved, retail_moved,
        "the rung has to change the picture"
    );
    assert_ne!(retail_moved, doubled_moved, "and the two rungs differ too");

    let (Some((retail_x, _)), Some((doubled_x, _)), Some((off_x, _))) =
        (retail_moved.last(), doubled_moved.last(), off_moved.last())
    else {
        panic!("no particles");
    };
    let retail = retail_x - off_x;
    let doubled = doubled_x - off_x;
    assert!(retail.abs() > 1.0, "{retail} is not a drift");
    assert!(
        (doubled - retail * 2.0).abs() < 1e-3,
        "doubled {doubled} should be exactly twice retail {retail}"
    );
}

#[test]
fn wind_strength_replaces_the_engine_constant() {
    let drift = |strength: &str| {
        let source = wind_source(
            "Blown",
            &format!(
                "    WindMotion = PingPong\n{strength}    WindPingPongStartAngleMin = 0\n    \
                 WindPingPongStartAngleMax = 0\n    WindPingPongEndAngleMin = 0\n    \
                 WindPingPongEndAngleMax = 0\n"
            ),
        );
        let (mut world, handle) = world(&source);
        world.spawn(handle, Vec3::ZERO).unwrap();
        world.warm_up(1.0, &NoGround);
        world
            .systems
            .first()
            .unwrap()
            .particles
            .first()
            .unwrap()
            .position
            .x
    };
    let tenth = drift("    WindStrength = 0.1\n");
    assert!((2.0..4.0).contains(&tenth), "{tenth}");
    assert_eq!(drift(""), drift("    WindStrength = 2\n"));
    assert_ne!(drift(""), drift("    WindStrength = 1\n"));
}

#[test]
fn a_fired_nugget_is_offset_snapped_to_the_ground_and_delayed() {
    let (mut world, handle) = world(
        "FXParticleSystem Burst\n  System\n    Lifetime = 100 100\n    BurstCount = 1 1\n    \
             Size = 1 1\n  End\nEnd\n",
    );
    let ground = FlatGround(12.0);
    let nuggets = [
        FxSpawn {
            template: handle,
            offset: Vec3::new(5.0, -3.0, 40.0),
            delay: RandomRange::default(),
            at_ground_height: true,
            orient_to_object: true,
            attach_to_object: true,
            set_target_matrix: false,
        },
        FxSpawn {
            template: handle,
            offset: Vec3::new(0.0, 0.0, 7.0),
            delay: RandomRange::constant(30.0),
            at_ground_height: false,
            orient_to_object: false,
            attach_to_object: false,
            set_target_matrix: false,
        },
    ];
    assert_eq!(
        world
            .fire(&nuggets, FxFrame::at(Vec3::new(100.0, 100.0, 0.0)), &ground)
            .0,
        2
    );

    let snapped = world.systems.first().unwrap();
    assert_eq!(
        snapped.origin,
        Vec3::new(105.0, 97.0, 12.0),
        "offset then ground"
    );
    let delayed = world.systems.get(1).unwrap();
    assert_eq!(delayed.origin, Vec3::new(100.0, 100.0, 7.0), "offset only");

    world.warm_up(29.0 / 30.0, &ground);
    assert_eq!(world.systems.first().unwrap().particles.len(), 29);
    assert_eq!(world.systems.get(1).unwrap().particles.len(), 0);
    world.warm_up(2.0 / 30.0, &ground);
    assert!(
        !world.systems.get(1).unwrap().particles.is_empty(),
        "the delay must expire at 30 frames, not 1,000"
    );
}

#[test]
fn firing_in_a_frame_rotates_the_offset_but_fire_does_not() {
    let (mut world, handle) = world(
        "FXParticleSystem Burst\n  System\n    Lifetime = 100 100\n    BurstCount = 1 1\n    \
             Size = 1 1\n  End\nEnd\n",
    );
    let nuggets = [FxSpawn {
        template: handle,
        offset: Vec3::new(1.0, 0.0, 0.0),
        delay: RandomRange::default(),
        at_ground_height: false,
        orient_to_object: true,
        attach_to_object: false,
        set_target_matrix: false,
    }];
    let quarter_turn = Mat4::from_cols([
        [0.0, 1.0, 0.0, 0.0],
        [-1.0, 0.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [5.0, 6.0, 7.0, 1.0],
    ]);
    assert_eq!(
        world
            .fire(
                &nuggets,
                FxFrame {
                    at: Vec3::new(5.0, 6.0, 7.0),
                    matrix: Some(quarter_turn),
                    secondary: None,
                },
                &NoGround,
            )
            .0,
        1
    );
    let rotated = world.systems.first().unwrap();
    assert_eq!(
        rotated.origin,
        Vec3::new(5.0, 7.0, 7.0),
        "the +X offset became +Y under the quarter turn"
    );

    assert_eq!(
        world
            .fire(&nuggets, FxFrame::at(Vec3::new(5.0, 6.0, 7.0)), &NoGround)
            .0,
        1
    );
    let unrotated = world.systems.get(1).unwrap();
    assert_eq!(
        unrotated.origin,
        Vec3::new(6.0, 6.0, 7.0),
        "fire (no frame) adds the offset in world space, unrotated"
    );
}

#[test]
fn set_target_matrix_pitches_the_caller_matrix_towards_the_secondary() {
    let (mut world, handle) = world(
        "FXParticleSystem Beam
  System
    Lifetime = 10 10
    BurstCount = 1 1
                 Size = 1 1
  End
End
",
    );
    let nugget = FxSpawn {
        template: handle,
        offset: Vec3::ZERO,
        delay: RandomRange::default(),
        at_ground_height: false,
        orient_to_object: true,
        attach_to_object: false,
        set_target_matrix: true,
    };
    let facing_y = Mat4::from_cols([
        [0.0, 1.0, 0.0, 0.0],
        [-1.0, 0.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ]);
    let secondary = Some(Vec3::new(0.0, 50.0, -10.0));
    let fire = |world: &mut ParticleWorld, nugget: FxSpawn, matrix| {
        world.fire(
            &[nugget],
            FxFrame {
                at: Vec3::ZERO,
                matrix,
                secondary,
            },
            &NoGround,
        );
        world.systems.last().unwrap().orientation
    };
    let aimed = fire(&mut world, nugget, Some(facing_y)).unwrap();
    let x = aimed.transform_vector(Vec3::X);
    assert!(
        (x.y + 1.0).abs() < 1e-5 && x.z.abs() < 1e-5,
        "level aim, X turned back: {x:?}"
    );

    world.step(&NoGround);
    let mut particle = world
        .systems
        .last()
        .unwrap()
        .particles
        .first()
        .cloned()
        .unwrap();
    particle.angle = 1.45;
    let shaft = crate::draw::model_transform(&particle, 1.0, Some(aimed), ParticleRotation::Z)
        .transform_vector(Vec3::new(0.0, -1.0, 0.0));
    assert!(
        shaft.y > 0.95,
        "the mesh's long axis reaches the target: {shaft:?}"
    );

    let unoriented = FxSpawn {
        orient_to_object: false,
        ..nugget
    };
    assert_eq!(
        fire(&mut world, unoriented, Some(facing_y)),
        None,
        "no caller matrix taken"
    );
}

#[test]
fn fire_reports_the_attach_to_object_spawns_and_their_local_offset() {
    let (mut world, handle) = world(
        "FXParticleSystem Puff\n  System\n    Lifetime = 10 10\n    BurstCount = 1 1\n    \
             Size = 1 1\n  End\nEnd\n",
    );
    let nuggets = [
        FxSpawn {
            template: handle,
            offset: Vec3::new(1.0, 2.0, 3.0),
            delay: RandomRange::default(),
            at_ground_height: false,
            orient_to_object: false,
            attach_to_object: true,
            set_target_matrix: false,
        },
        FxSpawn {
            template: handle,
            offset: Vec3::ZERO,
            delay: RandomRange::default(),
            at_ground_height: false,
            orient_to_object: false,
            attach_to_object: false,
            set_target_matrix: false,
        },
    ];
    let (started, attached) = world.fire(&nuggets, FxFrame::at(Vec3::ZERO), &NoGround);
    assert_eq!(started, 2);
    assert_eq!(attached.len(), 1, "only the AttachToObject nugget");
    assert_eq!(attached[0].1, Vec3::new(1.0, 2.0, 3.0));
}

#[test]
fn an_oriented_system_emits_along_the_frame_and_an_unoriented_one_along_the_world() {
    let (mut world, handle) = world(
        "FXParticleSystem Gust
  System
    Lifetime = 100 100
    BurstCount = 1 1
                 Size = 1 1
  End
  Physics = DefaultPhysics
    VelocityDamping = 1 1
  End
               EmissionVelocity = OrthoEmissionVelocity
    X = 7 7
  End
               EmissionVolume = LineEmissionVolume
    StartPoint = X:3 Y:0 Z:0
                 EndPoint = X:3 Y:0 Z:0
  End
End
",
    );
    let quarter_turn = Mat4::from_cols([
        [0.0, 1.0, 0.0, 0.0],
        [-1.0, 0.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ]);
    for orient in [true, false] {
        let nuggets = [FxSpawn {
            template: handle,
            offset: Vec3::ZERO,
            delay: RandomRange::default(),
            at_ground_height: false,
            orient_to_object: orient,
            attach_to_object: false,
            set_target_matrix: false,
        }];
        world.clear();
        world.fire(
            &nuggets,
            FxFrame {
                at: Vec3::ZERO,
                matrix: Some(quarter_turn),
                secondary: None,
            },
            &NoGround,
        );
        world.warm_up(1.0 / 30.0, &NoGround);
        let particle = &world.systems[0].particles[0];
        let (born, velocity) = if orient {
            (Vec3::new(0.0, 3.0, 0.0), Vec3::new(0.0, 7.0, 0.0))
        } else {
            (Vec3::new(3.0, 0.0, 0.0), Vec3::new(7.0, 0.0, 0.0))
        };
        assert_eq!(particle.velocity, velocity, "orient {orient}");
        assert_eq!(particle.previous, born, "orient {orient}");
    }
}

#[test]
fn a_particle_up_towards_its_emitter_turns_away_from_it() {
    let (mut world, handle) = world(
        "FXParticleSystem Wave
  System
    Lifetime = 100 100
    BurstCount = 1 1
                 Size = 1 1
    IsParticleUpTowardsEmitter = Yes
  End
               Update = DefaultUpdate
    AngleZ = 1 1
  End
  Physics = DefaultPhysics
    VelocityDamping = 1 1
  End
               EmissionVelocity = OrthoEmissionVelocity
    Y = -1 -1
  End
End
",
    );
    world.spawn(handle, Vec3::new(10.0, 10.0, 0.0)).unwrap();
    world.warm_up(1.0 / 30.0, &NoGround);
    let particle = &world.systems[0].particles[0];
    assert!(particle.angle.abs() < 1e-5, "{}", particle.angle);
    assert_eq!(
        up_away_from(Vec3::new(1.0, 0.0, 0.0), Vec3::ZERO),
        std::f32::consts::TAU
    );
    assert_eq!(
        up_away_from(Vec3::new(-1.0, 0.0, 0.0), Vec3::ZERO),
        std::f32::consts::PI
    );
    assert_eq!(up_away_from(Vec3::ZERO, Vec3::ZERO), std::f32::consts::PI);
}

#[test]
fn firing_an_fxlist_over_and_over_does_not_defeat_the_ceiling() {
    let (mut world, handle) = world(
        "FXParticleSystem Spam\n  System\n    Lifetime = 500 500\n    BurstCount = 5 5\n    \
             Priority = HIGH_OR_ABOVE\n    Size = 1 1\n  End\nEnd\n",
    );
    let ceiling = 200;
    let mut budget = world.budget();
    budget.max_particles = Some(ceiling);
    world.set_budget(budget);
    let nuggets = [
        FxSpawn {
            template: handle,
            offset: Vec3::ZERO,
            delay: RandomRange::default(),
            at_ground_height: false,
            orient_to_object: false,
            attach_to_object: false,
            set_target_matrix: false,
        },
        FxSpawn {
            template: handle,
            offset: Vec3::new(10.0, 0.0, 0.0),
            delay: RandomRange::default(),
            at_ground_height: false,
            orient_to_object: false,
            attach_to_object: false,
            set_target_matrix: false,
        },
    ];
    for _ in 0..50 {
        assert_eq!(
            world.fire(&nuggets, FxFrame::at(Vec3::ZERO), &NoGround).0,
            2
        );
        world.warm_up(1.0 / 30.0, &NoGround);
    }
    assert_eq!(world.live_systems(), 100, "every firing started its two");
    let (counted, exempt) = world.live_split();
    assert_eq!(exempt, 0, "nothing here is ALWAYS_RENDER");
    assert!(
        counted <= ceiling,
        "{counted} live particles past a ceiling of {ceiling}"
    );
    assert!(
        world.refusals().refused > 0 || world.refusals().evicted > 0,
        "the ceiling has to have refused or evicted something"
    );
}

#[test]
fn an_aimed_lightning_bolt_spans_from_its_point_to_its_target() {
    let (mut world, handle) = world(
        "FXParticleSystem Bolt\n  System\n    Type = STREAK\n    Lifetime = 20 20\n    \
             BurstCount = 10 10\n    Size = 1 1\n  End\n  EmissionVolume = LightningEmission\n    \
             EndPoint = X:-450 Y:0 Z:0\n    Amplitude1 = 100 100\n    Frequency1 = 4 6\n    \
             Phase1 = 4 5\n  End\nEnd\n",
    );
    let nuggets = [FxSpawn {
        template: handle,
        offset: Vec3::ZERO,
        delay: RandomRange::default(),
        at_ground_height: false,
        orient_to_object: true,
        attach_to_object: false,
        set_target_matrix: true,
    }];
    let (at, target) = (Vec3::new(10.0, 20.0, 30.0), Vec3::new(110.0, 20.0, 0.0));
    world.fire(
        &nuggets,
        FxFrame {
            at,
            matrix: Some(Mat4::IDENTITY),
            secondary: Some(target),
        },
        &NoGround,
    );
    world.step(&NoGround);
    let particles = &world.systems.first().unwrap().particles;
    assert_eq!(particles.len(), 10);
    let near = |a: Vec3, b: Vec3| sub(a, b).length() < 1e-3;
    assert!(near(particles.first().unwrap().position, at));
    assert!(near(particles.last().unwrap().position, target));
}

fn fire_unaimed_bolt(source: &str) -> Vec<Vec3> {
    let (mut world, handle) = world(source);
    let nuggets = [FxSpawn {
        template: handle,
        offset: Vec3::ZERO,
        delay: RandomRange::default(),
        at_ground_height: false,
        orient_to_object: false,
        attach_to_object: false,
        set_target_matrix: false,
    }];
    world.fire(&nuggets, FxFrame::at(Vec3::ZERO), &NoGround);
    world.step(&NoGround);
    world
        .systems
        .first()
        .unwrap()
        .particles
        .iter()
        .map(|particle| particle.position)
        .collect()
}

#[test]
fn a_lightning_bolt_bends_by_the_triangle_taper() {
    let positions = fire_unaimed_bolt(
        "FXParticleSystem Bolt\n  System\n    Type = STREAK\n    Lifetime = 20 20\n    \
             BurstCount = 3 3\n    Size = 1 1\n  End\n  EmissionVolume = LightningEmission\n    \
             EndPoint = X:-450 Y:0 Z:0\n    Amplitude1 = 100 100\n    Frequency1 = 0 0\n    \
             Phase1 = 1.5708 1.5708\n  End\nEnd\n",
    );
    assert_eq!(positions.len(), 3);
    let off_chord = positions[1].y.abs();
    assert!(
        (off_chord - 50.0).abs() < 1e-2,
        "expected 50 off chord at mid-bolt, got {off_chord}"
    );
}

#[test]
fn a_vertical_lightning_bolt_does_not_bend() {
    let positions = fire_unaimed_bolt(
        "FXParticleSystem Bolt\n  System\n    Type = STREAK\n    Lifetime = 20 20\n    \
             BurstCount = 3 3\n    Size = 1 1\n  End\n  EmissionVolume = LightningEmission\n    \
             StartPoint = X:0 Y:0 Z:300\n    EndPoint = X:0 Y:0 Z:0\n    Amplitude1 = 100 100\n    \
             Frequency1 = 0 0\n    Phase1 = 1.5708 1.5708\n  End\nEnd\n",
    );
    assert_eq!(positions.len(), 3);
    for position in positions {
        assert!(
            position.x.abs() < 1e-3 && position.y.abs() < 1e-3,
            "a vertical bolt should not leave its chord, got {position:?}"
        );
    }
}

#[test]
fn a_45_degree_lightning_bolt_bends_by_cosine_of_elevation() {
    let positions = fire_unaimed_bolt(
        "FXParticleSystem Bolt\n  System\n    Type = STREAK\n    Lifetime = 20 20\n    \
             BurstCount = 3 3\n    Size = 1 1\n  End\n  EmissionVolume = LightningEmission\n    \
             EndPoint = X:-450 Y:0 Z:-450\n    Amplitude1 = 100 100\n    Frequency1 = 0 0\n    \
             Phase1 = 1.5708 1.5708\n  End\nEnd\n",
    );
    assert_eq!(positions.len(), 3);
    let off_chord = positions[1].y.abs();
    let expected = 50.0 / std::f32::consts::SQRT_2;
    assert!(
        (off_chord - expected).abs() < 1e-1,
        "expected {expected} off chord at mid-bolt, got {off_chord}"
    );
}

#[test]
fn a_swirly_particle_circles_the_nearest_bone() {
    let text = |swirly: &str| {
        format!(
            "FXParticleSystem SwirlTest
  System
    Lifetime = 100 100
    BurstCount = 1 1
    BurstDelay = 0 0
  End
  Physics = DefaultPhysics
    VelocityDamping = 1 1
    {swirly}
  End
  EmissionVelocity = OrthoEmissionVelocity
    Z = 1 1
  End
End
"
        )
    };
    let run = |swirly: &str| {
        let definition = definition(&text(swirly));
        let mut world = ParticleWorld::new(17);
        let handle = world.register(&definition, None);
        let id = world.spawn(handle, Vec3::ZERO).unwrap();
        assert!(world.set_swirl_bones(
            id,
            &[Vec3::new(5000.0, 0.0, 0.0), Vec3::new(10.0, 0.0, 0.0)]
        ));
        world.warm_up(1.0 / 30.0, &NoGround);
        world.systems[0].particles[0].position
    };
    let swirled = run("Swirly = Yes");
    assert_eq!((swirled.x, swirled.y, swirled.z), (0.0, -4.0, 1.0));
    assert_eq!(run("Swirly = No"), Vec3::new(0.0, 0.0, 1.0));
}

fn ring_system(radius: f32, rate: f32, extra: &str) -> (ParticleWorld, TemplateHandle) {
    world(&format!(
        "FXParticleSystem Ring\n\
         \x20 System\n    Lifetime = 1000 1000\n    BurstCount = 1 1\n    BurstDelay = 0 0\n{extra}  End\n\
         \x20 EmissionVolume = CylinderEmissionVolume\n\
         \x20   Radius = {radius}\n    RadiusRate = {rate}\n    IsHollow = Yes\n  End\n\
         End\n"
    ))
}

fn reach(world: &ParticleWorld, particle: usize) -> f32 {
    let at = world.systems[0].particles[particle].position;
    (at.x * at.x + at.y * at.y).sqrt()
}

#[test]
fn a_cylinder_burst_uses_the_radius_as_it_stood_before_that_steps_update() {
    let (mut world, handle) = ring_system(50.0, 10.0, "");
    world.spawn(handle, Vec3::ZERO).unwrap();
    world.warm_up(1.0 / 30.0, &NoGround);
    assert!(
        (reach(&world, 0) - 50.0).abs() < 1e-3,
        "{}",
        reach(&world, 0)
    );
    world.warm_up(1.0 / 30.0, &NoGround);
    assert!(
        (reach(&world, 1) - 60.0).abs() < 1e-3,
        "{}",
        reach(&world, 1)
    );
}

#[test]
fn a_shrinking_cylinder_stops_at_the_floor() {
    let (mut world, handle) = ring_system(1.0, -5.0, "");
    world.spawn(handle, Vec3::ZERO).unwrap();
    for _ in 0..3 {
        world.warm_up(1.0 / 30.0, &NoGround);
    }
    assert!(
        (reach(&world, 2) - 0.1).abs() < 1e-5,
        "{}",
        reach(&world, 2)
    );
    let (mut world, handle) = ring_system(-1.0, 0.0, "");
    world.spawn(handle, Vec3::ZERO).unwrap();
    for _ in 0..2 {
        world.warm_up(1.0 / 30.0, &NoGround);
    }
    assert!(
        (reach(&world, 1) - 0.1).abs() < 1e-5,
        "{}",
        reach(&world, 1)
    );
}

#[test]
fn a_delayed_cylinder_does_not_grow_until_its_delay_ends() {
    let (mut world, handle) = ring_system(50.0, 10.0, "    InitialDelay = 3 3\n");
    world.spawn(handle, Vec3::ZERO).unwrap();
    for _ in 0..3 {
        world.warm_up(1.0 / 30.0, &NoGround);
    }
    assert_eq!(world.systems[0].radius, 50.0);
    world.warm_up(1.0 / 30.0, &NoGround);
    assert!((reach(&world, 0) - 50.0).abs() < 1e-3);
    assert_eq!(world.systems[0].radius, 60.0);
}

#[test]
fn a_radiusless_cylinder_with_a_rate_gives_every_particle_its_own_angle() {
    let (mut world, handle) = world(
        "FXParticleSystem FireFlies\n\
         \x20 System\n    Lifetime = 1000 1000\n    BurstCount = 1 1\n    BurstDelay = 0 0\n    IsParticleUpTowardsEmitter = Yes\n  End\n\
         \x20 EmissionVolume = CylinderEmissionVolume\n    Length = 75\n    RadiusRate = 0.001\n  End\n\
         End\n",
    );
    world.spawn(handle, Vec3::ZERO).unwrap();
    for _ in 0..39 {
        world.warm_up(1.0 / 30.0, &NoGround);
    }
    let mut angles: Vec<u32> = world.systems[0]
        .particles
        .iter()
        .map(|particle| particle.angle.to_bits())
        .collect();
    let live = angles.len();
    angles.sort_unstable();
    angles.dedup();
    println!(
        "birth angles: {} distinct over {live} particles",
        angles.len()
    );
    assert_eq!(angles.len(), live);
}
