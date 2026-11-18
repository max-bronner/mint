use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AssetKind {
    Mesh,
    Animation,
    Hlod,
    Texture,
    Hierarchy,
    BoundingBox,
    Shader,
    Particles,
    Unrecognised(u32),
}

impl AssetKind {
    #[must_use]
    pub const fn tag(self) -> u32 {
        match self {
            Self::Mesh => 0x4D45_5348,
            Self::Animation => 0x414E_494D,
            Self::Hlod => 0x484C_4F44,
            Self::Texture => 0x0054_4558,
            Self::Hierarchy => 0x4849_4552,
            Self::BoundingBox => 0x0042_4F58,
            Self::Shader => 0x4658_5348,
            Self::Particles => 0x5041_5254,
            Self::Unrecognised(tag) => tag,
        }
    }

    #[must_use]
    pub const fn from_tag(tag: u32) -> Self {
        match tag {
            0x4D45_5348 => Self::Mesh,
            0x414E_494D => Self::Animation,
            0x484C_4F44 => Self::Hlod,
            0x0054_4558 => Self::Texture,
            0x4849_4552 => Self::Hierarchy,
            0x0042_4F58 => Self::BoundingBox,
            0x4658_5348 => Self::Shader,
            0x5041_5254 => Self::Particles,
            other => Self::Unrecognised(other),
        }
    }

    #[must_use]
    pub const fn is_whole_file(self) -> bool {
        matches!(self, Self::Texture | Self::Shader)
    }
}

impl fmt::Display for AssetKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Mesh => f.write_str("MESH"),
            Self::Animation => f.write_str("ANIM"),
            Self::Hlod => f.write_str("HLOD"),
            Self::Texture => f.write_str("TEX"),
            Self::Hierarchy => f.write_str("HIER"),
            Self::BoundingBox => f.write_str("BOX"),
            Self::Shader => f.write_str("FXSH"),
            Self::Particles => f.write_str("PART"),
            Self::Unrecognised(tag) => {
                let bytes = tag.to_be_bytes();
                if bytes.iter().all(|&b| b == 0 || b.is_ascii_graphic()) {
                    for byte in bytes {
                        if byte != 0 {
                            write!(f, "{}", byte as char)?;
                        }
                    }
                    Ok(())
                } else {
                    write!(f, "{tag:#010x}")
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tag_round_trips_through_the_kind_it_denotes() {
        for kind in [
            AssetKind::Mesh,
            AssetKind::Animation,
            AssetKind::Hlod,
            AssetKind::Texture,
            AssetKind::Hierarchy,
            AssetKind::BoundingBox,
            AssetKind::Shader,
            AssetKind::Particles,
        ] {
            assert_eq!(AssetKind::from_tag(kind.tag()), kind);
        }
    }

    #[test]
    fn the_tag_is_the_code_packed_big_endian_so_the_file_bytes_read_backwards() {
        assert_eq!(AssetKind::Texture.tag().to_le_bytes(), *b"XET\0");
        assert_eq!(AssetKind::Mesh.tag().to_le_bytes(), *b"HSEM");
    }

    #[test]
    fn an_unknown_tag_is_kept_rather_than_rejected() {
        assert_eq!(
            AssetKind::from_tag(0x1234_5678),
            AssetKind::Unrecognised(0x1234_5678)
        );
    }

    #[test]
    fn a_three_letter_code_displays_without_its_padding_byte() {
        assert_eq!(AssetKind::Texture.to_string(), "TEX");
    }

    #[test]
    fn an_unprintable_unknown_tag_displays_as_a_number() {
        assert_eq!(
            AssetKind::Unrecognised(0x0102_0304).to_string(),
            "0x01020304"
        );
    }

    #[test]
    fn only_whole_file_kinds_say_they_are_whole_file() {
        assert!(AssetKind::Texture.is_whole_file());
        assert!(AssetKind::Shader.is_whole_file());
        assert!(!AssetKind::Mesh.is_whole_file());
    }
}
