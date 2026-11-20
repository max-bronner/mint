use super::*;
use crate::table::BlockTable;

fn parse(text: &str) -> Document {
    let table = BlockTable::particle_path();
    let document = Document::parse("test.ini", text, &table).unwrap();
    assert!(
        document.is_balanced(),
        "unbalanced: {:?}",
        document.diagnostics()
    );
    document
}

fn index(text: &str) -> ParticleIndex {
    let document = parse(text);
    let mut index = ParticleIndex::new();
    index.insert_document(&document);
    index
}

#[test]
fn a_gpu_draw_reads_its_three_flipbook_fields_together_or_not_at_all() {
    let index = index(
        "FXParticleSystem Fire\n\
             \x20 Draw = GpuDraw\n\
             \x20   TotalFrames = 16\n\
             \x20   FramesPerRow = 4\n\
             \x20   SpeedMultiplier = 0.5\n\
             \x20   DetailTexture = EXDetail.tga\n\
             \x20 End\n\
             End\n\
             FXParticleSystem Partial\n\
             \x20 Draw = GpuDraw\n\
             \x20   TotalFrames = 16\n\
             \x20   FramesPerRow = 4\n\
             \x20 End\n\
             End\n\
             FXParticleSystem Plain\n\
             \x20 Draw = DefaultDraw\n\
             \x20 End\n\
             End\n",
    );
    let fire = index.get("fire").unwrap().flipbook.unwrap();
    assert_eq!(fire.total_frames, 16);
    assert_eq!(fire.frames_per_row, 4);
    assert_eq!(fire.speed_multiplier, 0.5);
    assert_eq!(fire.rows(), 4);
    assert!(fire.animates());
    assert_eq!(
        index.get("fire").unwrap().detail_texture.as_deref(),
        Some("EXDetail.tga")
    );
    assert_eq!(index.get("partial").unwrap().flipbook, None);
    assert_eq!(index.get("plain").unwrap().flipbook, None);
}

#[test]
fn a_single_cell_sheet_does_not_animate_and_a_ragged_one_rounds_up() {
    let one = Flipbook {
        total_frames: 1,
        frames_per_row: 1,
        speed_multiplier: 1.0,
    };
    assert!(!one.animates());
    assert_eq!(one.rows(), 1);
    let ragged = Flipbook {
        total_frames: 7,
        frames_per_row: 4,
        speed_multiplier: 1.0,
    };
    assert_eq!(ragged.rows(), 2);
    let broken = Flipbook {
        total_frames: 0,
        frames_per_row: 0,
        speed_multiplier: 1.0,
    };
    assert_eq!(broken.rows(), 1);
}

#[test]
fn a_flipbook_layout_that_is_not_a_whole_count_is_refused() {
    let index = index(
        "FXParticleSystem Bad\n\
             \x20 Draw = GpuDraw\n\
             \x20   TotalFrames = 4.5\n\
             \x20   FramesPerRow = 4\n\
             \x20   SpeedMultiplier = 1\n\
             \x20 End\n\
             End\n\
             FXParticleSystem Zero\n\
             \x20 Draw = GpuDraw\n\
             \x20   TotalFrames = 16\n\
             \x20   FramesPerRow = 0\n\
             \x20   SpeedMultiplier = 1\n\
             \x20 End\n\
             End\n",
    );
    assert_eq!(index.get("bad").unwrap().flipbook, None);
    assert_eq!(index.get("zero").unwrap().flipbook, None);
}

#[test]
fn the_slave_and_per_particle_systems_are_read_from_the_system_block() {
    let index = index(
        "FXParticleSystem FireBuildingLarge\n\
             \x20 System\n\
             \x20   SlaveSystem = FireBuildingLargeFlare\n\
             \x20   SlavePosOffset = X:1 Y:-2 Z:3\n\
             \x20   PerParticleAttachedSystem = BuildingChunkDebris\n\
             \x20 End\n\
             End\n",
    );
    let held = index.get("firebuildinglarge").unwrap();
    assert_eq!(held.slave_system.as_deref(), Some("FireBuildingLargeFlare"));
    assert_eq!(held.slave_pos_offset, [1.0, -2.0, 3.0]);
    assert_eq!(
        held.per_particle_system.as_deref(),
        Some("BuildingChunkDebris")
    );
}

#[test]
fn a_terrain_collision_event_reads_all_five_fields_and_another_module_none() {
    let index = index(
        "FXParticleSystem Chunks\n\
             \x20 Event = TerrainCollision\n\
             \x20   EventFX = FX_SnowChunkGroundHit\n\
             \x20   HeightOffset = 0 10\n\
             \x20   KillAfterEvent = Yes\n\
             \x20   OrientFXToTerrain = No\n\
             \x20   PerParticle = Yes\n\
             \x20 End\n\
             End\n\
             FXParticleSystem Later\n\
             \x20 Event = LifeEvent\n\
             \x20   EventFX = FX_Something\n\
             \x20 End\n\
             End\n",
    );
    let event = index.get("chunks").unwrap().event.clone().unwrap();
    assert_eq!(event.fx, "FX_SnowChunkGroundHit");
    assert_eq!(
        event.height_offset,
        RandomRange {
            low: 0.0,
            high: 10.0
        }
    );
    assert!(event.kill_after_event);
    assert!(!event.orient_to_terrain);
    assert!(event.per_particle);
    assert_eq!(index.get("later").unwrap().event, None);
}

#[test]
fn a_terrain_collision_without_an_event_fx_is_no_event() {
    let index = index(
        "FXParticleSystem Chunks\n\
             \x20 Event = TerrainCollision\n\
             \x20   KillAfterEvent = Yes\n\
             \x20 End\n\
             End\n",
    );
    assert_eq!(index.get("chunks").unwrap().event, None);
}

#[test]
fn a_block_form_definition_reads_its_module_slots() {
    let index = index(
        "FXParticleSystem Puff\n\
             \x20 System\n\
             \x20   Shader = ALPHA\n\
             \x20   ParticleName = EXsnowcloud02.tga\n\
             \x20   Lifetime = 40 60\n\
             \x20   Size = 10 12\n\
             \x20   BurstCount = 2 3\n\
             \x20   BurstDelay = 4 4\n\
             \x20 End\n\
             \x20 EmissionVolume = LineEmissionVolume\n\
             \x20   StartPoint = X:0 Y:0 Z:5\n\
             \x20   EndPoint = X:0 Y:0 Z:15\n\
             \x20 End\n\
             \x20 EmissionVelocity = OrthoEmissionVelocity\n\
             \x20   X = 0 0.5\n\
             \x20   Z = 1 2\n\
             \x20 End\n\
             \x20 Draw = DefaultDraw\n\
             \x20 End\n\
             End\n",
    );
    assert_eq!(index.len(), 1);
    let puff = index.get("puff").unwrap();
    assert_eq!(puff.shader(), ParticleShader::Alpha);
    assert_eq!(puff.particle_name.as_deref(), Some("EXsnowcloud02.tga"));
    assert_eq!(
        puff.lifetime,
        RandomRange {
            low: 40.0,
            high: 60.0
        }
    );
    assert_eq!(
        puff.size,
        RandomRange {
            low: 10.0,
            high: 12.0
        }
    );
    assert_eq!(
        puff.burst_count,
        RandomRange {
            low: 2.0,
            high: 3.0
        }
    );
    assert_eq!(
        puff.volume,
        EmissionVolume::Line {
            start: [0.0, 0.0, 5.0],
            end: [0.0, 0.0, 15.0],
        }
    );
    assert_eq!(puff.velocity, EmissionVelocity::Ortho);
    assert_eq!(
        puff.ortho_velocity[0],
        RandomRange {
            low: 0.0,
            high: 0.5
        }
    );
    assert_eq!(puff.ortho_velocity[1], RandomRange::default());
    assert_eq!(
        puff.ortho_velocity[2],
        RandomRange {
            low: 1.0,
            high: 2.0
        }
    );
    assert_eq!(puff.draw_module.as_deref(), Some("DefaultDraw"));
    assert!(puff.is_sprite_path());
}

#[test]
fn an_unauthored_key_slot_is_the_templates_default_not_a_gap() {
    let index = index(
        "FXParticleSystem Dust\n\
             \x20 System\n\
             \x20   Shader = ALPHA\n\
             \x20 End\n\
             \x20 Color = DefaultColor\n\
             \x20   Color2 = R:255 G:128 B:0 25\n\
             \x20 End\n\
             \x20 Alpha = DefaultAlpha\n\
             \x20   Alpha2 = 0.15 0.35 15\n\
             \x20   Alpha3 = 0 0 75\n\
             \x20 End\n\
             End\n",
    );
    let dust = index.get("dust").unwrap();
    assert_eq!(dust.colors[0], ColorKey::default(), "black at frame 0");
    assert_eq!(dust.colors[1].frame, 25);
    assert!((dust.colors[1].rgb[0] - 1.0).abs() < 1e-6);
    assert_eq!(dust.colors_authored, 1);
    assert_eq!(dust.alphas[0], AlphaKey::default(), "0.0 at frame 0");
    assert_eq!(
        dust.alphas[1],
        AlphaKey {
            value: RandomRange {
                low: 0.15,
                high: 0.35
            },
            frame: 15,
        }
    );
    assert!(dust.has_alpha_block);
}

#[test]
fn an_absent_shader_is_additive_and_additive_ignores_alpha() {
    let index = index("FXParticleSystem Glow\n  System\n  End\nEnd\n");
    let glow = index.get("glow").unwrap();
    assert_eq!(glow.declared_shader, None);
    assert_eq!(glow.shader(), ParticleShader::Additive);
    assert!(!glow.shader().reads_alpha());
    assert!(!glow.has_alpha_block);
    assert!(ParticleShader::Alpha.reads_alpha());
}

#[test]
fn a_key_frame_past_two_to_the_thirty_first_survives() {
    let index = index(
        "FXParticleSystem Sentinel\n\
             \x20 Alpha = DefaultAlpha\n\
             \x20   Alpha1 = 1 1 0\n\
             \x20   Alpha2 = 0 0 3703221197\n\
             \x20 End\n\
             End\n",
    );
    let key = index.get("sentinel").unwrap().alphas[1];
    assert_eq!(key.frame, 3_703_221_197);
    assert!(key.frame > u32::try_from(i32::MAX).unwrap());
}

#[test]
fn the_flat_form_reads_the_same_emission_as_the_block_form() {
    let index = index(
        "ParticleSystem Splash\n\
             \x20 Shader = ADDITIVE\n\
             \x20 Type = PARTICLE\n\
             \x20 ParticleName = EXWater04.tga\n\
             \x20 Lifetime = 15.00 15.00\n\
             \x20 VelocityType = ORTHO\n\
             \x20 VelOrthoX = 0.000 16.000\n\
             \x20 VolumeType = LINE\n\
             \x20 VolLineStart = X:0.00 Y:-10.00 Z:0.00\n\
             \x20 VolLineEnd = X:0.00 Y:10.00 Z:1.00\n\
             \x20 Color1 = R:255 G:255 B:255 0\n\
             End\n",
    );
    let splash = index.get("splash").unwrap();
    assert_eq!(splash.form, DefinitionForm::Flat);
    assert_eq!(
        splash.volume,
        EmissionVolume::Line {
            start: [0.0, -10.0, 0.0],
            end: [0.0, 10.0, 1.0],
        }
    );
    assert_eq!(splash.velocity, EmissionVelocity::Ortho);
    assert_eq!(
        splash.ortho_velocity[0],
        RandomRange {
            low: 0.0,
            high: 16.0
        }
    );
    assert_eq!(splash.kind, ParticleKind::Particle);
}

#[test]
fn the_block_form_wins_a_duplicated_name_in_either_order() {
    let flat = "ParticleSystem Shared\n  Size = 1 1\nEnd\n\
                    ParticleSystem FlatOnly\n  Size = 3 3\nEnd\n";
    let block = "FXParticleSystem Shared\n  System\n    Size = 2 2\n  End\nEnd\n";

    for (first, second) in [(flat, block), (block, flat)] {
        let mut index = ParticleIndex::new();
        index.insert_document(&parse(first));
        index.insert_document(&parse(second));
        assert_eq!(index.len(), 2);
        assert_eq!(index.get("shared").unwrap().size.low, 2.0, "block wins");
        assert_eq!(index.get("shared").unwrap().form, DefinitionForm::Block);
        assert_eq!(index.get("flatonly").unwrap().size.low, 3.0);
        assert_eq!(index.shadowed_flat(), 1);
        assert_eq!(index.redefinitions(), 0);
        assert_eq!(index.block_definitions(), 1);
        assert_eq!(index.flat_definitions(), 2);
    }
}

#[test]
fn the_box_and_sphere_volumes_read_their_own_fields() {
    let index = index(
        "FXParticleSystem Crate\n\
             \x20 EmissionVolume = BoxEmissionVolume\n\
             \x20   HalfSize = X:10 Y:20 Z:5\n\
             \x20   IsHollow = Yes\n\
             \x20 End\n\
             End\n\
             FXParticleSystem Ball\n\
             \x20 EmissionVolume = SphereEmissionVolume\n\
             \x20   Radius = 12.5\n\
             \x20 End\n\
             End\n",
    );
    assert_eq!(
        index.get("crate").unwrap().volume,
        EmissionVolume::Box {
            half_size: [10.0, 20.0, 5.0],
            hollow: true,
        }
    );
    assert_eq!(
        index.get("ball").unwrap().volume,
        EmissionVolume::Sphere {
            radius: 12.5,
            hollow: false,
        }
    );
}

#[test]
fn rotation_reads_defaults_off_and_refuses_an_unknown_axis() {
    let index = index(
        "FXParticleSystem Rays
               Update = RenderObjectUpdate
                 Rotation = ROTATE_X
               End
             End
             FXParticleSystem Motes
               Update = DefaultUpdate
                 Rotation = ROTATE_V
               End
             End
             FXParticleSystem Plain
               Update = DefaultUpdate
               End
             End
             FXParticleSystem Askew
               Update = DefaultUpdate
                 Rotation = ROTATE_W
               End
             End
",
    );
    assert_eq!(index.get("rays").unwrap().rotation, ParticleRotation::X);
    assert_eq!(
        index.get("motes").unwrap().rotation,
        ParticleRotation::Velocity
    );
    assert_eq!(index.get("plain").unwrap().rotation, ParticleRotation::Off);
    let refused = index.value_errors().take();
    assert_eq!(refused.len(), 1);
    assert_eq!(refused[0].context, "Askew");
    assert_eq!(refused[0].field, "Rotation");
}

#[test]
fn a_speed_that_does_not_read_is_refused_and_an_absent_one_is_not() {
    let index = index(
        "FXParticleSystem Quiet\n\
             \x20 EmissionVelocity = SphericalEmissionVelocity\n\
             \x20 End\n\
             End\n\
             FXParticleSystem Loud\n\
             \x20 EmissionVelocity = SphericalEmissionVelocity\n\
             \x20   Speed = quickly\n\
             \x20 End\n\
             End\n",
    );
    let refused = index.value_errors().take();
    assert_eq!(refused.len(), 1);
    assert_eq!(refused[0].context, "Loud");
    assert_eq!(refused[0].field, "Speed");
}

#[test]
fn a_volume_offset_that_does_not_read_is_refused() {
    let index = index(
        "FXParticleSystem Skewed
               EmissionVolume = CylinderEmissionVolume
                 Radius = 4
                 Offset = X:1 Y:sideways Z:3
               End
             End
",
    );
    let refused = index.value_errors().take();
    assert_eq!(refused.len(), 1);
    assert_eq!(refused[0].context, "Skewed");
    assert_eq!(refused[0].field, "Offset");
}

#[test]
fn a_coordinate_with_a_space_after_its_tag_reads_the_next_token_as_its_value() {
    let index = index(
        "FXParticleSystem Spaced\n\
             \x20 EmissionVolume = CylinderEmissionVolume\n\
             \x20   Radius = 4\n\
             \x20   Offset = X:1.0 Y:-2.0 Z: 10.0\n\
             \x20 End\n\
             End\n",
    );
    let system = index.get("Spaced").unwrap();
    let EmissionVolume::Cylinder { offset, .. } = &system.volume else {
        panic!("a cylinder volume");
    };
    assert_eq!(*offset, [1.0, -2.0, 10.0]);
    assert!(index.value_errors().take().is_empty());
}

#[test]
fn every_emission_velocity_module_reads_its_own_speeds() {
    let index = index(
        "FXParticleSystem Ball\n\
             \x20 EmissionVelocity = SphericalEmissionVelocity\n\
             \x20   Speed = 2 6\n\
             \x20 End\n\
             End\n\
             FXParticleSystem Dome\n\
             \x20 EmissionVelocity = HemisphericalEmissionVelocity\n\
             \x20   Speed = 3 3\n\
             \x20 End\n\
             End\n\
             FXParticleSystem Tube\n\
             \x20 EmissionVelocity = CylindricalEmissionVelocity\n\
             \x20   Radial = 1 2\n\
             \x20   Normal = 4 5\n\
             \x20 End\n\
             End\n",
    );
    assert_eq!(
        index.get("ball").unwrap().velocity,
        EmissionVelocity::Spherical {
            speed: RandomRange {
                low: 2.0,
                high: 6.0
            }
        }
    );
    assert_eq!(
        index.get("dome").unwrap().velocity,
        EmissionVelocity::Hemispherical {
            speed: RandomRange::constant(3.0)
        }
    );
    assert_eq!(
        index.get("tube").unwrap().velocity,
        EmissionVelocity::Cylindrical {
            radial: RandomRange {
                low: 1.0,
                high: 2.0
            },
            normal: RandomRange {
                low: 4.0,
                high: 5.0
            },
        }
    );
}

#[test]
fn the_physics_and_update_blocks_read_step_sixs_motion() {
    let index = index(
        "FXParticleSystem Puff\n\
             \x20 Physics = DefaultPhysics\n\
             \x20   Gravity = -0.02\n\
             \x20   DriftVelocity = X:0.1 Y:-0.2 Z:1.0\n\
             \x20   VelocityDamping = 0.8 0.9\n\
             \x20 End\n\
             \x20 Update = DefaultUpdate\n\
             \x20   SizeRate = 0.5 1.5\n\
             \x20   SizeRateDamping = 0.95 0.95\n\
             \x20   AngleZ = -3 3\n\
             \x20   AngularRateZ = -0.1 0.1\n\
             \x20   AngularDamping = 0.99 0.99\n\
             \x20 End\n\
             End\n",
    );
    let puff = index.get("puff").unwrap();
    assert!((puff.gravity - -0.02).abs() < 1e-6);
    assert_eq!(puff.drift_velocity, [0.1, -0.2, 1.0]);
    assert_eq!(
        puff.velocity_damping,
        RandomRange {
            low: 0.8,
            high: 0.9
        }
    );
    assert_eq!(
        puff.size_rate,
        RandomRange {
            low: 0.5,
            high: 1.5
        }
    );
    assert_eq!(puff.size_rate_damping, RandomRange::constant(0.95));
    assert_eq!(
        puff.angle_z,
        RandomRange {
            low: -3.0,
            high: 3.0
        }
    );
    assert_eq!(puff.angular_damping, RandomRange::constant(0.99));
}

#[test]
fn a_per_axis_rate_without_a_start_size_grows_from_zero() {
    let index = index(
        "FXParticleSystem Ring\n\
             \x20 Update = RenderObjectUpdate\n\
             \x20   SizeRateZ = 0.5 1\n\
             \x20   SizeDampingZ = 0.85 1\n\
             \x20 End\n\
             End\n\
         FXParticleSystem Plain\n\
             \x20 Update = RenderObjectUpdate\n\
             \x20   AngleZ = -7 7\n\
             \x20 End\n\
             End\n",
    );
    let ring = index.get("ring").unwrap();
    assert_eq!(ring.render_size, Some([RandomRange::default(); 3]));
    assert_eq!(
        ring.render_size_rate[2],
        RandomRange {
            low: 0.5,
            high: 1.0
        }
    );
    assert_eq!(ring.render_size_damping[0], RandomRange::UNIT);
    assert_eq!(index.get("plain").unwrap().render_size, None);
}

#[test]
fn priority_sort_level_and_ground_alignment_are_read_and_a_strange_token_is_permissive() {
    let index = index(
        "FXParticleSystem Glow\n\
             \x20 System\n\
             \x20   Priority = HIGH_OR_ABOVE\n\
             \x20   SortLevel = 4\n\
             \x20   IsGroundAligned = Yes\n\
             \x20 End\n\
             End\n\
             FXParticleSystem Bare\n\
             \x20 System\n\
             \x20   Size = 1 1\n\
             \x20 End\n\
             End\n\
             FXParticleSystem Odd\n\
             \x20 System\n\
             \x20   Priority = WEAPON_EXPLSION\n\
             \x20   SortLevel = nonsense\n\
             \x20 End\n\
             End\n",
    );

    let glow = index.get("glow").unwrap();
    assert_eq!(glow.declared_priority, Some(ParticlePriority::HighOrAbove));
    assert_eq!(glow.priority(), ParticlePriority::HighOrAbove);
    assert_eq!(glow.sort_level, Some(4));
    assert!(glow.ground_aligned);

    let bare = index.get("bare").unwrap();
    assert_eq!(bare.declared_priority, None, "absence is reportable");
    assert_eq!(
        bare.priority(),
        ParticlePriority::Unrated,
        "a system that says nothing about the ladder is not restricted by it"
    );
    assert_eq!(bare.sort_level, None);
    assert!(!bare.ground_aligned);

    let odd = index.get("odd").unwrap();
    assert_eq!(
        odd.priority(),
        ParticlePriority::Unrated,
        "a token in neither enumeration must not lose the definition"
    );
    assert_eq!(
        odd.sort_level, None,
        "an unreadable level is the distance sort"
    );
}

#[test]
fn start_size_rate_is_read_from_the_system_block() {
    let index = index(
        "FXParticleSystem Growing\n\
             \x20 System\n\
             \x20   Size = 4 4\n\
             \x20   StartSizeRate = 1.5 2.5\n\
             \x20 End\n\
             End\n\
             FXParticleSystem Plain\n\
             \x20 System\n\
             \x20   Size = 4 4\n\
             \x20 End\n\
             End\n",
    );
    let growing = index.get("growing").unwrap();
    assert_eq!(
        growing.start_size_rate,
        RandomRange {
            low: 1.5,
            high: 2.5
        }
    );
    let plain = index.get("plain").unwrap();
    assert_eq!(plain.start_size_rate, RandomRange::default());
}

#[test]
fn the_priority_ladder_orders_by_how_hard_a_system_is_to_get_rid_of() {
    assert!(ParticlePriority::AlwaysRender > ParticlePriority::VeryLowOrAbove);
    assert!(ParticlePriority::VeryLowOrAbove > ParticlePriority::LowOrAbove);
    assert!(ParticlePriority::LowOrAbove > ParticlePriority::MediumOrAbove);
    assert!(ParticlePriority::MediumOrAbove > ParticlePriority::HighOrAbove);
    assert!(ParticlePriority::HighOrAbove > ParticlePriority::UltraHighOnly);
    assert!(ParticlePriority::AlwaysRender.is_exempt());
    assert!(!ParticlePriority::VeryLowOrAbove.is_exempt());
    for token in [
        "ALWAYS_RENDER",
        "VERY_LOW_OR_ABOVE",
        "LOW_OR_ABOVE",
        "MEDIUM_OR_ABOVE",
        "HIGH_OR_ABOVE",
        "ULTRA_HIGH_ONLY",
    ] {
        assert_eq!(ParticlePriority::parse(token).as_str(), token);
    }
}

#[test]
fn an_unwritten_velocity_damping_is_zero_and_the_other_dampings_the_identity() {
    let index = index(
        "FXParticleSystem Bare\n\
             \x20 Physics = DefaultPhysics\n\
             \x20   Gravity = -0.01\n\
             \x20 End\n\
             End\n",
    );
    let bare = index.get("bare").unwrap();
    assert_eq!(bare.velocity_damping, RandomRange::constant(0.0));
    assert_eq!(bare.size_rate_damping, RandomRange::UNIT);
    assert_eq!(bare.angular_damping, RandomRange::UNIT);
    assert_eq!(
        bare.size_rate,
        RandomRange::default(),
        "a rate, not a factor"
    );
}

#[test]
fn the_flat_form_reads_the_volumes_the_block_form_gained() {
    let index = index(
        "ParticleSystem Ring\n\
             \x20 VolumeType = CYLINDER\n\
             \x20 VolCylinderRadius = 30\n\
             \x20 VolCylinderLength = 8\n\
             \x20 IsHollow = Yes\n\
             \x20 VelocityType = OUTWARD\n\
             \x20 VelOutward = 2 4\n\
             \x20 VelOutwardOther = 1 1\n\
             End\n",
    );
    let ring = index.get("ring").unwrap();
    assert_eq!(
        ring.volume,
        EmissionVolume::Cylinder {
            radius: 30.0,
            length: 8.0,
            offset: [0.0; 3],
            radius_rate: 0.0,
            hollow: true,
        }
    );
    assert_eq!(
        ring.velocity,
        EmissionVelocity::Outward {
            speed: RandomRange {
                low: 2.0,
                high: 4.0
            },
            other_speed: RandomRange::constant(1.0),
        }
    );
}

#[test]
fn a_cylinder_and_an_outward_velocity_are_decoded() {
    let index = index(
        "FXParticleSystem Ring\n\
             \x20 EmissionVolume = CylinderEmissionVolume\n\
             \x20   Radius = 30\n\
             \x20   Length = 12\n\
             \x20   Offset = X:0.0 Y:0.0 Z:5.0\n\
             \x20   RadiusRate = 0.5\n\
             \x20   IsHollow = Yes\n\
             \x20 End\n\
             \x20 EmissionVelocity = OutwardEmissionVelocity\n\
             \x20   Speed = 2 4\n\
             \x20   OtherSpeed = 1 1\n\
             \x20 End\n\
             End\n",
    );
    let ring = index.get("ring").unwrap();
    assert_eq!(
        ring.volume,
        EmissionVolume::Cylinder {
            radius: 30.0,
            length: 12.0,
            offset: [0.0, 0.0, 5.0],
            radius_rate: 0.5,
            hollow: true,
        }
    );
    assert_eq!(
        ring.velocity,
        EmissionVelocity::Outward {
            speed: RandomRange {
                low: 2.0,
                high: 4.0
            },
            other_speed: RandomRange::constant(1.0),
        }
    );
    assert!(ring.is_sprite_path());
}

#[test]
fn an_undecoded_emission_module_keeps_its_name() {
    let index = index(
        "FXParticleSystem Bolt\n\
             \x20 EmissionVolume = TerrainFireEmission\n\
             \x20   Radius = 30\n\
             \x20 End\n\
             End\n",
    );
    let bolt = index.get("bolt").unwrap();
    assert_eq!(
        bolt.volume,
        EmissionVolume::Unread {
            module: "TerrainFireEmission".to_owned()
        }
    );
    assert!(!bolt.is_sprite_path());
}

#[test]
fn an_fxlist_reads_its_nuggets_in_order() {
    let document = parse(
        "FXList FX_Test\n\
             \x20 PlayEvenIfShrouded = Yes\n\
             \x20 ParticleSystem\n\
             \x20   Name = DragonStrikeGround\n\
             \x20 End\n\
             \x20 ParticleSystem\n\
             \x20   Name = DragonStrikeGround02\n\
             \x20 End\n\
             \x20 Sound\n\
             \x20   Name = FirePlumeSweetenerMS\n\
             \x20 End\n\
             End\n",
    );
    let mut index = FxListIndex::new();
    index.insert_document(&document);
    assert_eq!(index.len(), 1);
    let list = index.get("fx_test").unwrap();
    assert_eq!(list.nuggets.len(), 3, "the leaf is not a nugget");
    assert_eq!(list.nuggets[0].keyword, "ParticleSystem");
    assert_eq!(list.nuggets[2].keyword, "Sound");
    assert_eq!(list.play_even_if_shrouded, Some(true));
    let named: Vec<&str> = index
        .particle_references()
        .map(|(_, system)| system)
        .collect();
    assert_eq!(named, ["DragonStrikeGround", "DragonStrikeGround02"]);
}

#[test]
fn an_fxlist_reads_its_bone_pos_and_camera_shaker_nuggets() {
    let document = parse(
        "FXList FX_TelekinesisAtBone\n\
             \x20 FXListAtBonePos\n\
             \x20   FX = FX_Telekinesis\n\
             \x20   BoneName = STAFF\n\
             \x20 End\n\
             \x20 FXListAtBonePos\n\
             \x20   FX = FX_Other\n\
             \x20   BoneName = HAND\n\
             \x20   OrientToBone = No\n\
             \x20 End\n\
             \x20 CameraShakerVolume\n\
             \x20   Radius = 500\n\
             \x20   Duration_Seconds = 1.5\n\
             \x20   Amplitude_Degrees = 0.5\n\
             \x20 End\n\
             End\n",
    );
    let mut index = FxListIndex::new();
    index.insert_document(&document);
    let list = index.get("fx_telekinesisatbone").unwrap();
    assert_eq!(list.nuggets.len(), 3);
    let first = list.nuggets[0].at_bone_pos.as_ref().unwrap();
    assert_eq!(first.fx.as_deref(), Some("FX_Telekinesis"));
    assert_eq!(first.bone_name.as_deref(), Some("STAFF"));
    assert!(first.orient_to_bone, "unwritten defaults to Yes");
    let second = list.nuggets[1].at_bone_pos.as_ref().unwrap();
    assert!(!second.orient_to_bone);
    let shaker = list.nuggets[2].camera_shaker.unwrap();
    assert_eq!(shaker.radius, 500.0);
    assert_eq!(shaker.duration_seconds, 1.5);
    assert_eq!(shaker.amplitude_degrees, 0.5);
}

#[test]
fn an_fxlist_reads_its_eva_event_nugget() {
    let document = parse(
        "FXList FX_Death\n\
             \x20 EvaEvent\n\
             \x20   EvaEventOwner = EVA_UnitLostOwner\n\
             \x20   EvaEventAlly = EVA_UnitLostAlly\n\
             \x20 End\n\
             End\n",
    );
    let mut index = FxListIndex::new();
    index.insert_document(&document);
    let list = index.get("fx_death").unwrap();
    let eva = list.nuggets[0].eva_event.as_ref().unwrap();
    assert_eq!(eva.owner.as_deref(), Some("EVA_UnitLostOwner"));
    assert_eq!(eva.ally.as_deref(), Some("EVA_UnitLostAlly"));
    assert_eq!(eva.enemy, None, "EvaEventEnemy was not written");
}

#[test]
fn an_fxlist_camera_shaker_scalar_that_does_not_read_is_refused() {
    let document = parse(
        "FXList FX_Shake
               CameraShakerVolume
                 Radius = loudly
               End
             End
",
    );
    let mut index = FxListIndex::new();
    index.insert_document(&document);
    let refused = index.value_errors().take();
    assert_eq!(refused.len(), 1);
    assert_eq!(refused[0].field, "Radius");
}

#[test]
fn an_fxlist_reads_its_direct_particle_sys_bone_and_culling_leaves() {
    let document = parse(
        "FXList FX_Bones\n\
             \x20 ParticleSysBone = BONE_Head SmokeWisp\n\
             \x20 CullingInfo = SOME_TOKEN\n\
             End\n",
    );
    let mut index = FxListIndex::new();
    index.insert_document(&document);
    let list = index.get("fx_bones").unwrap();
    assert_eq!(
        list.particle_sys_bones,
        vec![vec!["BONE_Head".to_owned(), "SmokeWisp".to_owned()]]
    );
    assert_eq!(list.culling_info.as_deref(), Some("SOME_TOKEN"));
}

#[test]
fn color_scale_is_read_raw() {
    let index = index(
        "FXParticleSystem Fade
  Color = DefaultColor
    ColorScale = -5 0
  End
End
",
    );
    let fade = index.get("fade").unwrap();
    assert_eq!(
        fade.color_scale,
        RandomRange {
            low: -5.0,
            high: 0.0
        }
    );
}

#[test]
fn a_wind_block_with_no_motion_is_wind_data_with_the_wind_switched_off() {
    let index = index(
        "FXParticleSystem Embers\n  Wind = DefaultWind\n    WindStrength = 0.5\n    \
             WindAngleChangeMin = 0.017453\n    WindPingPongEndAngleMax = 6.28319\n  End\nEnd\n\
             FXParticleSystem Still\n  System\n    Lifetime = 10 10\n  End\nEnd\n",
    );
    let embers = index.get("embers").unwrap().wind.unwrap();
    assert_eq!(embers.motion, WindMotion::None, "no WindMotion line");
    assert!(!embers.motion.moves());
    assert_eq!(embers.declared_strength, Some(0.5), "still read");
    assert!((embers.angle_change.low - 0.017_453).abs() < 1e-6);

    assert_eq!(index.get("still").unwrap().wind, None);

    assert_eq!(WindMotion::parse("Unused"), WindMotion::None);
    assert_eq!(WindMotion::parse("NONE"), WindMotion::None);
    assert_eq!(WindMotion::parse("pingpong"), WindMotion::PingPong);
    assert_eq!(WindMotion::parse("Circular"), WindMotion::Circular);
}

#[test]
fn the_absent_wind_distances_are_eas_numbers_rather_than_zero() {
    let index = index(
        "FXParticleSystem Bare\n  Wind = DefaultWind\n    WindMotion = PingPong\n  End\nEnd\n\
             FXParticleSystem Full\n  Wind = DefaultWind\n    WindMotion = PingPong\n    \
             WindStrength = 0.1\n    WindFullStrengthDist = 15\n    WindZeroStrengthDist = 100\n  \
             End\nEnd\n",
    );
    let bare = index.get("bare").unwrap().wind.unwrap();
    assert_eq!(bare.declared_strength, None);
    assert_eq!(bare.declared_full_strength_dist, None);
    assert_eq!(bare.declared_zero_strength_dist, None);
    assert!((bare.strength() - 2.0).abs() < f32::EPSILON);
    assert!((bare.full_strength_dist() - 75.0).abs() < f32::EPSILON);
    assert!((bare.zero_strength_dist() - 200.0).abs() < f32::EPSILON);

    let full = index.get("full").unwrap().wind.unwrap();
    assert_eq!(full.declared_strength, Some(0.1));
    assert!((full.full_strength_dist() - 15.0).abs() < f32::EPSILON);
    assert!((full.zero_strength_dist() - 100.0).abs() < f32::EPSILON);
}

#[test]
fn a_wind_range_is_two_lines_and_neither_end_is_assumed() {
    let index = index(
        "FXParticleSystem Gust\n  Wind = DefaultWind\n    WindMotion = PingPong\n    \
             WindAngleChangeMax = 0.2\n    WindPingPongEndAngleMin = 5.49779\n    \
             WindPingPongEndAngleMax = 5.28319\n  End\nEnd\n",
    );
    let wind = index.get("gust").unwrap().wind.unwrap();
    assert_eq!(
        wind.angle_change.low, 0.0,
        "absent Min is zero, not the Max"
    );
    assert!((wind.angle_change.high - 0.2).abs() < 1e-6);
    assert!(
        wind.ping_pong_end.high < wind.ping_pong_end.low,
        "the inverted pair is kept as written: {:?}",
        wind.ping_pong_end
    );
    assert_eq!(wind.ping_pong_start, RandomRange::default());
}

#[test]
fn a_nugget_delay_is_milliseconds_rounded_up_and_ignores_its_distribution() {
    let document = parse(
        "FXList FX_Delayed\n  ParticleSystem\n    Name = A\n    InitialDelay = 1000 1000 \
             UNIFORM\n  End\n  ParticleSystem\n    Name = B\n    InitialDelay = 300 300 CONSTANT\n \
             End\n  ParticleSystem\n    Name = C\n    InitialDelay = 1 1 UNIFORM\n  End\n  \
             ParticleSystem\n    Name = D\n    InitialDelay = notanumber\n  End\n  \
             ParticleSystem\n    Name = E\n  End\nEnd\n",
    );
    let mut lists = FxListIndex::new();
    lists.insert_document(&document);
    let list = lists.get("fx_delayed").unwrap();
    let delay = |slot: usize| {
        list.nuggets
            .get(slot)
            .unwrap()
            .particle
            .as_ref()
            .unwrap()
            .initial_delay
    };
    assert_eq!(delay(0), RandomRange::constant(5.0), "1000 ms is 5 frames");
    assert_eq!(delay(1), RandomRange::constant(2.0), "300 ms, CONSTANT");
    assert_eq!(delay(2), RandomRange::constant(1.0), "1 ms rounds up to 1");
    assert_eq!(delay(3), RandomRange::default(), "an unexpanded macro");
    assert_eq!(delay(4), RandomRange::default(), "no field at all");
}

#[test]
fn only_a_particle_nugget_carries_the_five_fields_and_they_read_as_written() {
    let document = parse(
        "FXList FX_Hit\n  ParticleSystem\n    Name = Sparks\n    Offset = X:1.5 Y:-2 Z:5\n    \
             OrientToObject = Yes\n    AttachToObject = No\n    CreateAtGroundHeight = Yes\n  \
             End\n  Sound\n    Name = Bang\n  End\nEnd\n",
    );
    let mut lists = FxListIndex::new();
    lists.insert_document(&document);
    let list = lists.get("FX_HIT").unwrap();
    assert_eq!(list.nuggets.len(), 2);

    let particle = list.nuggets.first().unwrap();
    assert!(particle.is_particle_system());
    let nugget = particle.particle.as_ref().unwrap();
    assert_eq!(nugget.offset, [1.5, -2.0, 5.0]);
    assert!(nugget.orient_to_object);
    assert!(
        !nugget.attach_to_object,
        "No is written 7 times and means it"
    );
    assert!(nugget.create_at_ground_height);

    let sound = list.nuggets.get(1).unwrap();
    assert!(!sound.is_particle_system());
    assert_eq!(sound.particle, None, "a Sound has none of the five");
    assert_eq!(sound.name.as_deref(), Some("Bang"));
}

#[test]
fn a_nugget_reads_its_water_and_weather_gates() {
    let document = parse(
        "FXList FX_TrollFootStep\n\
             \x20 ParticleSystem\n\
             \x20   Name = entFootDust\n\
             \x20 End\n\
             \x20 ParticleSystem\n\
             \x20   Name = MumakilFootSplash\n\
             \x20   OnlyIfOnWater = Yes\n\
             \x20 End\n\
             End\n\
             FXList FX_TrollRightFootStep\n\
             \x20 FXListAtBonePos\n\
             \x20   FX = FX_TrollFootStep\n\
             \x20   BoneName = B_FOOTR\n\
             \x20   Weather = NORMAL\n\
             \x20 End\n\
             End\n",
    );
    let mut lists = FxListIndex::new();
    lists.insert_document(&document);
    let step = lists.get("fx_trollfootstep").unwrap();
    let dust = step.nuggets[0].particle.as_ref().unwrap();
    assert!(!dust.on_land && !dust.on_water, "no gate written");
    let splash = step.nuggets[1].particle.as_ref().unwrap();
    assert!(splash.on_water, "OnlyIfOnWater = Yes");
    assert!(!splash.on_land);
    let bone_pos_nugget = &lists.get("fx_trollrightfootstep").unwrap().nuggets[0];
    assert!(bone_pos_nugget.at_bone_pos.is_some());
    assert_eq!(
        bone_pos_nugget.conditions.weather,
        Some(NuggetWeather::Normal)
    );
}

#[test]
fn an_unreadable_weather_value_is_refused() {
    let document =
        parse("FXList FX_Bad\n  ParticleSystem\n    Name = A\n    Weather = MONSOON\n  End\nEnd\n");
    let mut lists = FxListIndex::new();
    lists.insert_document(&document);
    let nugget = &lists.get("fx_bad").unwrap().nuggets[0];
    assert!(nugget.particle.is_some());
    assert_eq!(nugget.conditions.weather, None);
    let refused = lists.value_errors().take();
    assert_eq!(refused.len(), 1);
    assert_eq!(refused[0].field, "Weather");
}

#[test]
fn every_nugget_kind_reads_the_base_condition_fields() {
    let document = parse(
        "FXList FX_Conditioned\n\
             \x20 Sound\n\
             \x20   Name = Bang\n\
             \x20   SourceObjectFilter = NONE +INFANTRY\n\
             \x20   ObjectFilter = ANY -HERO\n\
             \x20   RequiredSourceModelConditions = DISGUISED\n\
             \x20   ExcludedSourceModelConditions = DEAD\n\
             \x20   RequiredSecondaryModelConditions = AFLAME\n\
             \x20   ExcludedSecondaryModelConditions = STUNNED\n\
             \x20   StopIfNuggetPlayed = Yes\n\
             \x20   Weather = SNOWY\n\
             \x20 End\n\
             \x20 EvaEvent\n\
             \x20   StopIfNuggetPlayed = Yes\n\
             \x20 End\n\
             \x20 CameraShakerVolume\n\
             \x20   StopIfNuggetPlayed = Yes\n\
             \x20 End\n\
             End\n",
    );
    let mut lists = FxListIndex::new();
    lists.insert_document(&document);
    let list = lists.get("fx_conditioned").unwrap();
    for nugget in &list.nuggets {
        assert!(
            nugget.conditions.stop_if_played,
            "{} did not read StopIfNuggetPlayed",
            nugget.keyword
        );
    }
    let sound = &list.nuggets[0];
    assert_eq!(
        sound.conditions.source_filter,
        Some(vec!["NONE".to_owned(), "+INFANTRY".to_owned()])
    );
    assert_eq!(
        sound.conditions.object_filter,
        Some(vec!["ANY".to_owned(), "-HERO".to_owned()])
    );
    assert_eq!(sound.conditions.required_source, ["DISGUISED"]);
    assert_eq!(sound.conditions.excluded_source, ["DEAD"]);
    assert_eq!(sound.conditions.required_secondary, ["AFLAME"]);
    assert_eq!(sound.conditions.excluded_secondary, ["STUNNED"]);
    assert_eq!(sound.conditions.weather, Some(NuggetWeather::Snowy));
    assert!(list.nuggets[1].conditions.source_filter.is_none());
}

#[test]
fn an_unreadable_stop_if_nugget_played_is_refused() {
    let document =
        parse("FXList FX_Bad\n  Sound\n    Name = A\n    StopIfNuggetPlayed = MAYBE\n  End\nEnd\n");
    let mut lists = FxListIndex::new();
    lists.insert_document(&document);
    let sound = &lists.get("fx_bad").unwrap().nuggets[0];
    assert!(!sound.conditions.stop_if_played);
    let refused = lists.value_errors().take();
    assert_eq!(refused.len(), 1);
    assert_eq!(refused[0].field, "StopIfNuggetPlayed");
}

#[test]
fn the_water_gate_admits_by_the_fields_it_writes() {
    assert!(water_gate_admits(false, false, false));
    assert!(water_gate_admits(false, false, true));
    assert!(water_gate_admits(true, false, false), "on land, on land");
    assert!(
        !water_gate_admits(true, false, true),
        "on land, but on water"
    );
    assert!(water_gate_admits(false, true, true), "on water, on water");
    assert!(!water_gate_admits(false, true, false), "on water, but dry");
    assert!(!water_gate_admits(true, true, true), "asks for both");
    assert!(!water_gate_admits(true, true, false), "asks for both");
}

#[test]
fn the_weather_gate_admits_a_match_or_no_gate_at_all() {
    assert!(weather_gate_admits(None, NuggetWeather::Normal));
    assert!(weather_gate_admits(None, NuggetWeather::Snowy));
    assert!(weather_gate_admits(
        Some(NuggetWeather::Snowy),
        NuggetWeather::Snowy
    ));
    assert!(!weather_gate_admits(
        Some(NuggetWeather::Snowy),
        NuggetWeather::Normal
    ));
}

#[test]
fn a_particle_flag_reads_one_boolean_grammar_and_no_other_spelling() {
    for (written, expected) in [("Yes", true), ("no", false), ("true", false), ("1", true)] {
        let index = index(&format!(
            "FXParticleSystem Crate\n\
             \x20 System = Default\n\
             \x20   IsOneShot = {written}\n\
             \x20 End\n\
             \x20 EmissionVolume = BoxEmissionVolume\n\
             \x20   HalfSize = X:1 Y:1 Z:1\n\
             \x20   IsHollow = {written}\n\
             \x20 End\n\
             End\n"
        ));
        let definition = index.get("crate").unwrap();
        assert_eq!(definition.one_shot, expected, "IsOneShot = {written}");
        assert_eq!(
            definition.volume,
            EmissionVolume::Box {
                half_size: [1.0, 1.0, 1.0],
                hollow: expected,
            },
            "IsHollow = {written}"
        );
    }
}
