#![allow(clippy::expect_used, clippy::panic)]

use mint_render::{CustomMaterialSource, CustomShader, Error, Renderer};
use mint_sage_particle::{FLIPBOOK_SHADER_WGSL, SMUDGE_SHADER_WGSL};

#[test]
fn the_flipbook_and_smudge_shaders_compile() {
    let mut renderer = match Renderer::offscreen(16, 16) {
        Ok(renderer) => renderer,
        Err(Error::NoAdapter) => {
            eprintln!("SKIPPED: no graphics adapter, the particle shaders were not compiled");
            return;
        }
        Err(error) => panic!("creating an offscreen renderer failed: {error}"),
    };
    for (label, source, scene_color) in [
        ("flipbook", FLIPBOOK_SHADER_WGSL, false),
        ("smudge", SMUDGE_SHADER_WGSL, true),
    ] {
        renderer
            .register_custom_shader(&CustomShader {
                label,
                source,
                material: CustomMaterialSource::Builtin,
                scene_color,
                ..CustomShader::default()
            })
            .unwrap_or_else(|error| panic!("{label} does not compile: {error}"));
    }
}
