#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mint_render::{Camera, Error as RenderError, Instance, MeshData, Renderer, Vec3, Vertex};
use mint_sage_ini::{BlockTable, Document, ParticleIndex};
use mint_sage_particle::{NoGround, ParticleWorld};

#[test]
fn a_drawable_systems_instances_are_reported_part_major_from_where_they_start() {
    let mut renderer = match Renderer::offscreen(16, 16) {
        Ok(renderer) => renderer,
        Err(RenderError::NoAdapter) => {
            eprintln!("SKIPPED: no graphics adapter, nothing in tests/model_draws.rs was verified");
            return;
        }
        Err(error) => panic!("creating an offscreen renderer failed: {error}"),
    };
    let document = Document::parse(
        "test.ini",
        "FXParticleSystem ModelTest\n  System\n    Type = DRAWABLE\n    ParticleName = ModelTest.w3d\n    \
         Lifetime = 100 100\n    BurstCount = 3 3\n    BurstDelay = 1000 1000\n  End\n  \
         EmissionVolume = PointEmissionVolume\n  End\n  \
         EmissionVelocity = OrthoEmissionVelocity\n  End\nEnd\n",
        &BlockTable::particle_path(),
    )
    .unwrap();
    assert!(document.is_balanced(), "{:?}", document.diagnostics());
    let mut index = ParticleIndex::new();
    index.insert_document(&document);
    let definition = index.iter().next().unwrap().1.clone();
    let mesh = renderer
        .upload_mesh(&MeshData {
            vertices: vec![Vertex::default(); 3],
            indices: vec![0, 1, 2],
        })
        .unwrap();

    let mut world = ParticleWorld::new(7);
    let handle = world.register_model(&definition, None, vec![Instance::new(mesh); 2]);
    world.spawn(handle, Vec3::ZERO).unwrap();
    world.warm_up(4.0 / 30.0, &NoGround);

    let mut out = vec![Instance::new(mesh); 5];
    world
        .draw(&mut renderer, &Camera::default(), &mut out)
        .unwrap();
    let draws = world.model_draws();
    assert_eq!(draws.len(), 1);
    let draw = &draws[0];
    assert_eq!(draw.start, 5);
    assert_eq!(draw.placements.len(), 3);
    assert_eq!(out.len(), 5 + 2 * draw.placements.len());
    assert_eq!(draw.ages.len(), 3);
    assert!(draw.ages.iter().all(|age| *age > 0.0 && *age < 0.5));
}
