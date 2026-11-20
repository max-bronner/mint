use std::path::PathBuf;

pub mod anim_script;
pub mod animation_state;
pub mod camera_pose;
pub mod camera_transform;
mod client_clock;
pub mod dynamic_decal;
pub mod lighting;
mod objects_present;
mod once;
mod particles_models;
mod particles_present;
pub mod post_present;
mod report;
mod roads;
mod scene;
pub mod shadow_present;
pub mod shroud_present;
pub mod snow_present;
pub mod terrain_present;
pub mod units_present;
pub mod water_fx;
pub mod water_look;
pub mod water_present;
pub mod wave_tracks;

pub use animation_state::{
    phase_offset, pick_clip, rate_scale, reachable_condition_sets, speed_factor, variant,
    variant_fraction, CompiledState, Conditions, ExtraTokens, MachineReport, ModelStateMachine,
    StateMachine, WeaponPhase, EXTRA_CAPACITY,
};
pub use lighting::{
    default_time_of_day, distance_fog, load_cloud_darkening, model_light_sets, terrain_rig,
    terrain_scene, water_set_name, CloudDarkening, CloudDarkeningLook, ModelLights, SunConvention,
    TerrainRig, ENVIRONMENT_INI,
};
pub use mint_sage_particle::FlipbookMode;
pub use objects_present::{
    advance_object_animations, advance_object_sway, advance_object_uv_mappers,
    apply_object_conditions, build_object_scene, model_instances, placement_groups,
    replace_placement, rotation_anchors, template_notes, ModelGroup, ObjectScene, TemplateNote,
};
pub use particles_present::{
    attach_particle_bones, follow_particle_bones, BoneFrameMode, BoneLookup, BoneReport,
    ConditionParticles, FireReport, MapGround, ParticleScene,
};
pub use report::ClassNote;
pub use roads::{
    build_road_network, road_instances, road_state_line, road_texture_name, upload_roads, RoadScene,
};
pub use scene::{load_map_scene, tuning_ceiling, tuning_skip_mask, MapScene, SceneOptions};
pub use shadow_present::{ShadowFilter, ShadowPass};
pub use shroud_present::{
    bind_shroud_scene, push_shroud_scene, ShroudFade, ShroudView, CLEAR_ALPHA, FOG_ALPHA,
    FOG_LEVELS_PER_SECOND, MAX_SHROUD_EDGE, SHROUD_ALPHA,
};
pub use snow_present::SnowScene;
pub use terrain_present::{
    drawn_parts, load_sky_cloud, load_water_set_sky, object_instances, part_instance, part_texture,
    upload_terrain, upload_terrain_shading, TerrainShading, OBJECT_EMISSIVE,
};
pub use units_present::{
    drawable_id, names_part, natural_seconds, part_hidden, template_draws_a_model, Blending,
    BoneFxFire, GhostLook, LookArt, LookKey, LookSubObject, LookTexture, Ride, Skin, UnitDrawStats,
    UnitDrawable, UnitLook, UnitPose, UnitScene, UnitSceneReport, UnitTemplate, LOOK_STAMP_IDS,
};
pub use water_look::WaveRung;
pub(crate) use water_present::NO_SHROUD_EXTENT;
pub use water_present::{
    is_lava, over_standing_water, push_river_uniforms, push_water_uniforms, push_wave_uniforms,
    river_instances, upload_rivers, upload_water, upload_waves, water_instances, wave_instances,
    EnvSource, RiverScene, WaterMaterial, WaterScene, WaveScene,
};

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    #[error("{path}: {source}")]
    Vfs {
        path: PathBuf,
        #[source]
        source: mint_vfs::Error,
    },
}
