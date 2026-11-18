#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CustomShaderHandle(pub(crate) usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CustomMaterialHandle(pub(crate) usize);

#[derive(Debug, Clone, Copy)]
pub struct CustomShader<'a> {
    pub label: &'a str,
    pub source: &'a str,
    pub texture_slots: u32,
    pub uniform_size: u32,
    pub writes_alpha: bool,
    pub scene_depth: bool,
    pub scene_color: bool,
    pub material: CustomMaterialSource,
    pub scene_uniform_size: u32,
    pub scene_texture_slots: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CustomMaterialSource {
    #[default]
    Caller,
    Builtin,
}

impl Default for CustomShader<'_> {
    fn default() -> Self {
        Self {
            label: "custom",
            source: "",
            texture_slots: 0,
            uniform_size: 0,
            writes_alpha: true,
            scene_depth: false,
            scene_color: false,
            material: CustomMaterialSource::Caller,
            scene_uniform_size: 0,
            scene_texture_slots: 0,
        }
    }
}

pub const MAX_CUSTOM_TEXTURE_SLOTS: u32 = 8;

pub const MAX_CUSTOM_UNIFORM_SIZE: u32 = 4096;

#[must_use]
pub(crate) fn aligned_uniform_size(size: u32) -> Option<u32> {
    size.checked_add(15).map(|raised| raised & !15)
}

#[cfg(test)]
#[allow(clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn a_uniform_size_rounds_up_to_wgsls_sixteen_byte_struct_alignment() {
        assert_eq!(aligned_uniform_size(0), Some(0));
        assert_eq!(aligned_uniform_size(1), Some(16));
        assert_eq!(aligned_uniform_size(16), Some(16));
        assert_eq!(aligned_uniform_size(17), Some(32));
        assert_eq!(aligned_uniform_size(112), Some(112));
    }

    #[test]
    fn a_uniform_size_near_the_top_of_the_range_does_not_wrap() {
        assert_eq!(aligned_uniform_size(u32::MAX), None);
    }
}
