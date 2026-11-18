use mint_render::{
    Camera, CustomMaterialHandle, CustomShader, CustomShaderHandle, Instance, MeshData, MeshHandle,
    PixelFormat, RenderTargetHandle, Renderer, Scene, TextureData, TextureHandle, Vec3, Vertex,
};

use crate::frame::{FramePlanes, PlaneRef};
use crate::yuv::SHADER;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Render(#[from] mint_render::Error),
    #[error("plane {plane} is {width}x{height} but holds {len} bytes")]
    BadPlane {
        plane: &'static str,
        width: u32,
        height: u32,
        len: usize,
    },
}

pub type Result<T> = std::result::Result<T, Error>;

struct Gpu {
    shape: [(u32, u32); 4],
    picture: (u32, u32),
    flip: bool,
    has_alpha: bool,
    textures: [TextureHandle; 4],
    material: CustomMaterialHandle,
    target: RenderTargetHandle,
    target_texture: TextureHandle,
}

pub struct VideoSurface {
    shader: CustomShaderHandle,
    mesh: MeshHandle,
    gpu: Option<Gpu>,
}

fn plane_texture(name: &'static str, plane: &PlaneRef<'_>) -> Result<TextureData> {
    let expected = usize::try_from(plane.width)
        .ok()
        .zip(usize::try_from(plane.height).ok())
        .and_then(|(w, h)| w.checked_mul(h));
    if plane.width == 0 || plane.height == 0 || expected != Some(plane.data.len()) {
        return Err(Error::BadPlane {
            plane: name,
            width: plane.width,
            height: plane.height,
            len: plane.data.len(),
        });
    }
    Ok(TextureData::single(
        PixelFormat::R8Unorm,
        plane.width,
        plane.height,
        plane.data.to_vec(),
    ))
}

fn quad() -> MeshData {
    let corner = |x: f32, y: f32, u: f32, v: f32| Vertex::new([x, y, 0.0], [0.0, 0.0, 1.0], [u, v]);
    MeshData {
        vertices: vec![
            corner(-1.0, 1.0, 0.0, 0.0),
            corner(1.0, 1.0, 1.0, 0.0),
            corner(1.0, -1.0, 1.0, 1.0),
            corner(-1.0, -1.0, 0.0, 1.0),
        ],
        indices: vec![0, 1, 2, 0, 2, 3],
    }
}

impl VideoSurface {
    pub fn new(renderer: &mut Renderer) -> Result<Self> {
        let shader = renderer.register_custom_shader(&CustomShader {
            label: "movie yuv",
            source: SHADER,
            texture_slots: 4,
            uniform_size: 16,
            ..CustomShader::default()
        })?;
        let mesh = renderer.upload_mesh(&quad())?;
        Ok(Self {
            shader,
            mesh,
            gpu: None,
        })
    }

    pub fn upload(&mut self, renderer: &mut Renderer, frame: &FramePlanes<'_>) -> Result<()> {
        let alpha_plane = frame.alpha.unwrap_or(PlaneRef {
            width: 1,
            height: 1,
            data: &[0],
        });
        let planes = [
            ("Y", frame.y),
            ("U", frame.u),
            ("V", frame.v),
            ("alpha", alpha_plane),
        ];
        let shape = planes.map(|(_, plane)| (plane.width, plane.height));
        let picture = (frame.width, frame.height);
        let has_alpha = frame.alpha.is_some();
        let reusable = self.gpu.as_ref().is_some_and(|gpu| {
            gpu.shape == shape
                && gpu.picture == picture
                && gpu.flip == frame.bottom_up
                && gpu.has_alpha == has_alpha
        });
        if !reusable {
            let mut handles = Vec::with_capacity(4);
            for (name, plane) in &planes {
                handles.push(renderer.upload_texture(&plane_texture(name, plane)?)?);
            }
            let Ok(textures): std::result::Result<[TextureHandle; 4], _> = handles.try_into()
            else {
                return Ok(());
            };
            let material = renderer.create_custom_material(
                self.shader,
                &textures.map(Some),
                &params_block(picture, frame.bottom_up, has_alpha),
            )?;
            let target = renderer.create_render_target(picture.0, picture.1)?;
            let target_texture = renderer
                .render_target_texture(target)
                .ok_or(mint_render::Error::UnknownHandle)?;
            self.gpu = Some(Gpu {
                shape,
                picture,
                flip: frame.bottom_up,
                has_alpha,
                textures,
                material,
                target,
                target_texture,
            });
            return Ok(());
        }
        if let Some(gpu) = &self.gpu {
            for ((name, plane), texture) in planes.iter().zip(gpu.textures) {
                if name == &"alpha" && !has_alpha {
                    continue;
                }
                renderer.update_texture(texture, &plane_texture(name, plane)?)?;
            }
        }
        Ok(())
    }

    #[must_use]
    pub fn instance(&self) -> Option<Instance> {
        self.gpu.as_ref().map(|gpu| Instance {
            custom_material: Some(gpu.material),
            ..Instance::new(self.mesh)
        })
    }

    #[must_use]
    pub fn size(&self) -> Option<(u32, u32)> {
        self.gpu.as_ref().map(|gpu| gpu.picture)
    }

    pub fn render(&mut self, renderer: &mut Renderer) -> Result<Option<TextureHandle>> {
        let (Some(gpu), Some(instance)) = (&self.gpu, self.instance()) else {
            return Ok(None);
        };
        let camera = Camera {
            eye: Vec3::new(0.0, 0.0, 10.0),
            target: Vec3::ZERO,
            up: Vec3::Y,
            fov_y: std::f32::consts::FRAC_PI_2,
            near: 0.05,
            far: 100.0,
        };
        let mut scene = Scene::new(camera);
        scene.clear_color = [0.0, 0.0, 0.0, 0.0];
        renderer.render_to_target(gpu.target, &scene.with_instances(&[instance]))?;
        Ok(Some(gpu.target_texture))
    }
}

fn params_block(picture: (u32, u32), flip: bool, has_alpha: bool) -> [u8; 16] {
    let words = [
        i32::try_from(picture.0).unwrap_or(i32::MAX),
        i32::try_from(picture.1).unwrap_or(i32::MAX),
        i32::from(flip),
        i32::from(has_alpha),
    ];
    let mut block = [0u8; 16];
    for (chunk, word) in block.chunks_exact_mut(4).zip(words) {
        chunk.copy_from_slice(&word.to_le_bytes());
    }
    block
}
