#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mint_render::{Camera, Error as RenderError, Instance, MeshData, Renderer, Vec3, Vertex};
use mint_sage_ini::{BlockTable, Document, ParticleIndex, ParticleSystemDef};
use mint_sage_particle::{NoGround, ParticleWorld, TemplateHandle};

const VERTICES_PER_QUAD: usize = 4;

fn definition(name: &str, burst_count: u32, extra: &str) -> ParticleSystemDef {
    let text = format!(
        "FXParticleSystem {name}\n  System\n    ParticleName = EX.tga\n    Lifetime = 17 17\n    \
         Size = 1 1\n    BurstCount = {burst_count} {burst_count}\n    BurstDelay = 0 0\n{extra}  \
         End\n  EmissionVolume = PointEmissionVolume\n  End\n  \
         EmissionVelocity = OrthoEmissionVelocity\n  End\nEnd\n"
    );
    let document = Document::parse("test.ini", &text, &BlockTable::particle_path()).unwrap();
    assert!(document.is_balanced(), "{:?}", document.diagnostics());
    let mut index = ParticleIndex::new();
    index.insert_document(&document);
    let first = index.iter().next().unwrap().1.clone();
    first
}

fn renderer() -> Option<Renderer> {
    match Renderer::offscreen(16, 16) {
        Ok(renderer) => Some(renderer),
        Err(RenderError::NoAdapter) => {
            eprintln!("SKIPPED: no graphics adapter, nothing in tests/buffers.rs was verified");
            None
        }
        Err(error) => panic!("creating an offscreen renderer failed: {error}"),
    }
}

fn grow(world: &mut ParticleWorld, slave: TemplateHandle) -> (usize, usize) {
    let before = world.capacity(slave).unwrap();
    let master = world.register(&definition("master", 23, "    SlaveSystem = slave\n"), None);
    world.link(master, Some(slave), None, None);
    let after = world.capacity(slave).unwrap();
    assert!(after > before, "the link grew the slave's batch");
    (before, after)
}

#[test]
fn a_buffer_uploaded_before_its_templates_batch_grew_is_never_written_past_its_end() {
    let Some(mut renderer) = renderer() else {
        return;
    };
    let camera = Camera::default();
    let mut drawn: Vec<Instance> = Vec::new();

    let mut world = ParticleWorld::new(7);
    let slave = world.register(&definition("slave", 2, ""), None);
    let early = world.spawn(slave, Vec3::ZERO).unwrap();
    world.warm_up(2.0 / 30.0, &NoGround);
    world.draw(&mut renderer, &camera, &mut drawn).unwrap();

    grow(&mut world, slave);
    assert!(world.remove(early));

    world.spawn(slave, Vec3::ZERO).unwrap();
    world.warm_up(2.0 / 30.0, &NoGround);
    drawn.clear();
    world.draw(&mut renderer, &camera, &mut drawn).unwrap();
    assert!(!drawn.is_empty(), "the later system draws");
}

#[test]
fn a_live_system_whose_template_grows_draws_from_a_buffer_the_new_batch_fits() {
    let Some(mut renderer) = renderer() else {
        return;
    };
    let camera = Camera::default();
    let mut drawn: Vec<Instance> = Vec::new();

    let mut world = ParticleWorld::new(7);
    let slave = world.register(&definition("slave", 2, ""), None);
    world.spawn(slave, Vec3::ZERO).unwrap();
    world.warm_up(2.0 / 30.0, &NoGround);
    world.draw(&mut renderer, &camera, &mut drawn).unwrap();

    let (_, grown) = grow(&mut world, slave);
    world.warm_up(1.0 / 30.0, &NoGround);
    drawn.clear();
    world.draw(&mut renderer, &camera, &mut drawn).unwrap();
    let held = drawn.last().expect("the live system still draws").mesh;

    let full = MeshData {
        vertices: vec![Vertex::default(); grown * VERTICES_PER_QUAD],
        indices: Vec::new(),
    };
    renderer
        .update_mesh_vertex_prefix(held, &full)
        .expect("the live system's buffer holds the grown batch");
}
