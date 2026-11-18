#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mint_render::{Error as RenderError, Renderer, Vec3};
use mint_sage_ini::{BlockTable, Document, FxListIndex, ParticleIndex};
use mint_sage_model::Resolver;
use mint_sage_particle::NoGround;
use mint_sage_scene::ParticleScene;
use mint_vfs::Vfs;

fn renderer() -> Option<Renderer> {
    match Renderer::offscreen(4, 4) {
        Ok(renderer) => Some(renderer),
        Err(RenderError::NoAdapter) => {
            eprintln!(
                "SKIPPED: no graphics adapter on this machine, so nothing in \
                 tests/particles.rs was actually verified"
            );
            None
        }
        Err(error) => panic!("creating an offscreen renderer failed: {error}"),
    }
}

const A_SYSTEM: &str = "FXParticleSystem Puff\n  System\n    Lifetime = 10 10\n    BurstCount = 1 \
                         1\n    Size = 1 1\n  End\nEnd\n";

fn scene_from(particles: &str, lists: &str) -> ParticleScene {
    let table = BlockTable::particle_path();
    let mut index = ParticleIndex::new();
    let document = Document::parse("particles.ini", particles, &table).unwrap();
    assert!(document.is_balanced(), "{:?}", document.diagnostics());
    index.insert_document(&document);
    let mut fx = FxListIndex::new();
    let document = Document::parse("fxlist.ini", lists, &table).unwrap();
    assert!(document.is_balanced(), "{:?}", document.diagnostics());
    fx.insert_document(&document);
    ParticleScene::new(index, fx)
}

#[test]
fn an_at_bone_pos_nugget_runs_its_nested_list_and_falls_back_with_no_caller() {
    let Some(mut renderer) = renderer() else {
        return;
    };
    let vfs = Vfs::new();
    let resolver = Resolver::new();
    let mut scene = scene_from(
        A_SYSTEM,
        "FXList Outer\n\
         \x20 ParticleSystem\n    Name = Puff\n  End\n\
         \x20 FXListAtBonePos\n    FX = Nested\n    BoneName = MADE_UP_BONE\n  End\n\
         End\n\
         FXList Nested\n  ParticleSystem\n    Name = Puff\n  End\nEnd\n",
    );
    let report = scene
        .fire_list(
            &mut renderer,
            &vfs,
            &resolver,
            "Outer",
            Vec3::new(1.0, 2.0, 3.0),
            None,
            &NoGround,
            None,
            None,
            &|_, _| false,
            mint_sage_ini::NuggetWeather::Normal,
        )
        .expect("Outer is declared");
    assert_eq!(report.systems, 2, "the outer system and the nested one");
    assert_eq!(report.at_bone_pos, 1);
    assert_eq!(report.nested_list_missing, 0, "Nested is declared");
    assert_eq!(
        report.bone_missing, 1,
        "no caller drawable to find a bone on"
    );
    assert_eq!(report.bone_found, 0);
}

#[test]
fn an_at_bone_pos_nugget_naming_no_list_is_counted() {
    let Some(mut renderer) = renderer() else {
        return;
    };
    let vfs = Vfs::new();
    let resolver = Resolver::new();
    let mut scene = scene_from(
        A_SYSTEM,
        "FXList Outer\n  FXListAtBonePos\n    FX = Missing\n    BoneName = X\n  End\nEnd\n",
    );
    let report = scene
        .fire_list(
            &mut renderer,
            &vfs,
            &resolver,
            "Outer",
            Vec3::ZERO,
            None,
            &NoGround,
            None,
            None,
            &|_, _| false,
            mint_sage_ini::NuggetWeather::Normal,
        )
        .expect("Outer is declared");
    assert_eq!(report.at_bone_pos, 1);
    assert_eq!(report.nested_list_missing, 1);
    assert_eq!(
        report.systems, 0,
        "nothing to fire - Outer has no ParticleSystem of its own"
    );
}

#[test]
fn an_at_bone_pos_cycle_terminates_at_the_depth_cap() {
    let Some(mut renderer) = renderer() else {
        return;
    };
    let vfs = Vfs::new();
    let resolver = Resolver::new();
    let mut scene = scene_from(
        A_SYSTEM,
        "FXList A\n  FXListAtBonePos\n    FX = B\n    BoneName = X\n  End\nEnd\n\
         FXList B\n  FXListAtBonePos\n    FX = A\n    BoneName = X\n  End\nEnd\n",
    );
    let report = scene
        .fire_list(
            &mut renderer,
            &vfs,
            &resolver,
            "A",
            Vec3::ZERO,
            None,
            &NoGround,
            None,
            None,
            &|_, _| false,
            mint_sage_ini::NuggetWeather::Normal,
        )
        .expect("A is declared");
    assert_eq!(report.at_bone_pos, 4);
    assert_eq!(report.nested_list_missing, 0, "A and B both resolve");
}

#[test]
fn a_placement_drawing_no_model_still_attaches_its_particle_systems() {
    let Some(mut renderer) = renderer() else {
        return;
    };
    let vfs = Vfs::new();
    let resolver = Resolver::new();
    let mut scene = scene_from(A_SYSTEM, "");
    let templates = Document::parse(
        "objects.ini",
        "Object EmitterProxyTest\n  Draw = W3DScriptedModelDraw Tag\n\
         \x20   DefaultModelConditionState\n      Model = NONE\n\
         \x20     ParticleSysBone = None Puff\n    End\n  End\nEnd\n\
         Object QuietProxyTest\n  Draw = W3DDefaultDraw Tag\n  End\nEnd\n",
        &BlockTable::object_model_path(),
    )
    .unwrap();
    let mut index = mint_sage_ini::ObjectIndex::new();
    index.insert_document(&templates);
    let objects: Vec<mint_sage_map::Object> = ["EmitterProxyTest", "QuietProxyTest"]
        .into_iter()
        .map(|name| mint_sage_map::Object {
            x: 10.0,
            y: 20.0,
            z: 0.0,
            angle: 0.0,
            flags: 0,
            template_name: name.to_owned(),
            properties: mint_sage_map::Dictionary::default(),
        })
        .collect();
    let proxies: Vec<mint_sage_terrain::ObjectProxy> = (0..objects.len())
        .map(|index| mint_sage_terrain::ObjectProxy {
            index,
            position: Vec3::new(10.0, 20.0, 5.0),
            ground: Some(5.0),
            angle: 0.0,
            align_to_terrain: false,
            waypoint: false,
            placement: mint_render::Mat4::from_translation(Vec3::new(10.0, 20.0, 5.0)),
        })
        .collect();
    let objects_scene =
        mint_sage_scene::build_object_scene(&mut renderer, &vfs, &resolver, &index, &objects);
    let report = mint_sage_scene::attach_particle_bones(
        &mut scene,
        &mut renderer,
        &vfs,
        &resolver,
        &index,
        &objects_scene,
        &objects,
        &proxies,
    );
    assert_eq!(report.placements, 1, "only the emitter proxy declares one");
    assert_eq!(report.spawned, 1);
    assert_eq!(report.dangling, 0);
}
