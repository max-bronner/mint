#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mint_render::{Camera, Error as RenderError, Instance, Renderer, Vec3};
use mint_sage_ini::{BlockTable, Document, ParticleIndex};
use mint_sage_particle::{NoGround, ParticleWorld};

#[test]
fn a_hidden_template_draws_nothing_and_still_simulates() {
    let mut renderer = match Renderer::offscreen(16, 16) {
        Ok(renderer) => renderer,
        Err(RenderError::NoAdapter) => {
            eprintln!("SKIPPED: no graphics adapter, nothing in tests/hidden.rs was verified");
            return;
        }
        Err(error) => panic!("creating an offscreen renderer failed: {error}"),
    };

    let table = BlockTable::particle_path();
    let document = Document::parse(
        "test.ini",
        "FXParticleSystem Puff\n  System\n    Lifetime = 100 100\n    BurstCount = 4 4\n    \
         Size = 1 1\n  End\n  EmissionVolume = PointEmissionVolume\n  End\n  \
         EmissionVelocity = OrthoEmissionVelocity\n  End\nEnd\n",
        &table,
    )
    .unwrap();
    assert!(document.is_balanced(), "{:?}", document.diagnostics());
    let mut index = ParticleIndex::new();
    index.insert_document(&document);
    let definition = index.iter().next().unwrap().1.clone();

    let mut world = ParticleWorld::new(7);
    let handle = world.register(&definition, None);
    world.spawn(handle, Vec3::ZERO).unwrap();
    world.warm_up(5.0 / 30.0, &NoGround);
    let live = world.live_particles();
    assert!(live > 0, "the system emitted");

    let camera = Camera::default();
    let mut shown: Vec<Instance> = Vec::new();
    world.draw(&mut renderer, &camera, &mut shown).unwrap();
    assert!(!shown.is_empty(), "a visible system draws");

    world.set_hidden([handle]);
    let mut hidden: Vec<Instance> = Vec::new();
    world.draw(&mut renderer, &camera, &mut hidden).unwrap();
    assert!(hidden.is_empty(), "a hidden template draws nothing");
    assert_eq!(
        world.live_particles(),
        live,
        "hiding is presentation only - the system kept running"
    );
}
