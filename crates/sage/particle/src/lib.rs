#![deny(clippy::arithmetic_side_effects)]

mod budget;
mod draw;
mod ground;
mod load;
mod rng;
mod world;

pub use budget::{
    static_lod_effects, static_lod_max_particles, static_lod_shadow_decals, Budget, BudgetReport,
    DetailLevel, DEFAULT_MAX_PARTICLES,
};
pub use draw::{
    fill_ribbon_points, ribbon_mesh, BatchShape, BillboardMode, DrawShape, FlatSize, FlipbookMode,
    RibbonPoint, ShapeMode, SmudgeJitter, SortMode, FLIPBOOK_SHADER_WGSL, SMUDGE_MAX_OFFSET,
    SMUDGE_SHADER_WGSL, VOLUME_PARTICLE_ATTENUATION, VOLUME_PARTICLE_COPIES, VOLUME_PARTICLE_DEPTH,
};
pub use ground::{FlatGround, Ground, NoGround};
pub use load::{
    load_particle_index, load_particle_index_over, particle_index_from_documents,
    ParticleLoadReport,
};
pub use rng::ClientRng;
pub use world::{
    AdditiveAlpha, AttachedFrame, BeamLengthMode, Collision, CollisionEvent, FxFrame, FxSpawn,
    ModelDraw, ParticleWorld, SystemId, TemplateHandle, WindMode, MAX_COMPOUND_DEPTH,
    MAX_PARTICLES_PER_SYSTEM, PARTICLE_HZ,
};
