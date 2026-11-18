use super::*;
use crate::testutil::{chunk, concat};

#[test]
fn a_four_byte_id_chunk_is_shared_and_a_longer_one_is_per_element() {
    let one = 1u32.to_le_bytes();
    let shared = IndexTable::parse("TEXTURE_IDS", &one).expect("4 bytes");
    assert!(matches!(shared, IndexTable::Shared(1)));
    assert_eq!(shared.first(), Some(1));
    assert_eq!(shared.len(), 1);

    let mut per = Vec::new();
    per.extend_from_slice(&7u32.to_le_bytes());
    per.extend_from_slice(&8u32.to_le_bytes());
    let per = IndexTable::parse("TEXTURE_IDS", &per).expect("8 bytes");
    assert_eq!(per.len(), 2);
    assert_eq!(per.first(), Some(7));
}

#[test]
fn the_unused_sentinel_reads_as_none() {
    let sentinel = u32::MAX.to_le_bytes();
    let ids = IndexTable::parse("TEXTURE_IDS", &sentinel).expect("4 bytes");
    assert_eq!(ids.first(), None);
}

#[test]
fn a_ragged_id_chunk_is_rejected() {
    assert!(matches!(
        IndexTable::parse("TEXTURE_IDS", &[0u8; 7]),
        Err(Error::RaggedArray { .. })
    ));
}

#[test]
fn shader_material_reads_header_and_properties() {
    let mut header = vec![1u8];
    header.extend_from_slice(b"NormalMapped.fx");
    header.resize(33, 0);
    header.extend_from_slice(&2u32.to_le_bytes());
    assert_eq!(header.len(), ShaderMaterial::HEADER_SIZE);

    let mut texture = Vec::new();
    texture.extend_from_slice(&1u32.to_le_bytes());
    texture.extend_from_slice(&15u32.to_le_bytes());
    texture.extend_from_slice(b"DiffuseTexture\0");
    texture.extend_from_slice(&24u32.to_le_bytes());
    texture.extend_from_slice(b"boxes_barrels_color.tga\0");

    let mut opacity = Vec::new();
    opacity.extend_from_slice(&2u32.to_le_bytes());
    opacity.extend_from_slice(&8u32.to_le_bytes());
    opacity.extend_from_slice(b"Opacity\0");
    opacity.extend_from_slice(&0.5f32.to_le_bytes());

    let body = concat(&[
        chunk(ChunkType::SHADER_MATERIAL_HEADER, false, &header),
        chunk(ChunkType::SHADER_MATERIAL_PROPERTY, false, &texture),
        chunk(ChunkType::SHADER_MATERIAL_PROPERTY, false, &opacity),
    ]);
    let file = chunk(
        ChunkType::SHADER_MATERIALS,
        true,
        &chunk(ChunkType::SHADER_MATERIAL, true, &body),
    );

    let top = crate::Chunks::new(&file)
        .next()
        .and_then(Result::ok)
        .expect("one chunk");
    let mats = parse_shader_materials(&top).expect("valid");
    let mat = mats.first().expect("one material");
    assert_eq!(mat.shader_name, "NormalMapped.fx");
    assert_eq!(mat.version, 1);
    assert_eq!(mat.unknown, 2);
    assert_eq!(mat.diffuse_texture(), Some("boxes_barrels_color.tga"));
    assert_eq!(mat.normal_map(), None);
    assert_eq!(
        mat.property("opacity"),
        Some(&ShaderMaterialValue::Float(0.5))
    );
}

#[test]
fn every_property_tag_decodes_to_its_documented_width() {
    let cases: [(u32, Vec<u8>, ShaderMaterialValue); 6] = [
        (
            2,
            1.5f32.to_le_bytes().to_vec(),
            ShaderMaterialValue::Float(1.5),
        ),
        (
            3,
            concat(&[1f32.to_le_bytes().to_vec(), 2f32.to_le_bytes().to_vec()]),
            ShaderMaterialValue::Vec2([1.0, 2.0]),
        ),
        (
            4,
            concat(&[
                1f32.to_le_bytes().to_vec(),
                2f32.to_le_bytes().to_vec(),
                3f32.to_le_bytes().to_vec(),
            ]),
            ShaderMaterialValue::Vec3([1.0, 2.0, 3.0]),
        ),
        (
            5,
            concat(&[
                1f32.to_le_bytes().to_vec(),
                2f32.to_le_bytes().to_vec(),
                3f32.to_le_bytes().to_vec(),
                4f32.to_le_bytes().to_vec(),
            ]),
            ShaderMaterialValue::Vec4([1.0, 2.0, 3.0, 4.0]),
        ),
        (6, 9u32.to_le_bytes().to_vec(), ShaderMaterialValue::Int(9)),
        (7, vec![1u8], ShaderMaterialValue::Bool(true)),
    ];
    for (tag, bytes, expected) in cases {
        let mut payload = Vec::new();
        payload.extend_from_slice(&tag.to_le_bytes());
        payload.extend_from_slice(&2u32.to_le_bytes());
        payload.extend_from_slice(b"P\0");
        payload.extend_from_slice(&bytes);
        let prop = ShaderMaterial::parse_property(&payload).expect("valid property");
        assert_eq!(prop.name, "P");
        assert_eq!(prop.value, expected, "tag {tag}");
    }
}

#[test]
fn an_unknown_property_tag_is_preserved_verbatim() {
    let mut payload = Vec::new();
    payload.extend_from_slice(&99u32.to_le_bytes());
    payload.extend_from_slice(&2u32.to_le_bytes());
    payload.extend_from_slice(b"P\0");
    payload.extend_from_slice(&[0xDE, 0xAD]);
    let prop = ShaderMaterial::parse_property(&payload).expect("unknown tags are not fatal");
    assert_eq!(
        prop.value,
        ShaderMaterialValue::Unknown {
            tag: 99,
            bytes: vec![0xDE, 0xAD]
        }
    );
}

#[test]
fn a_property_name_length_past_the_payload_errors_rather_than_allocating() {
    let mut prop = Vec::new();
    prop.extend_from_slice(&1u32.to_le_bytes());
    prop.extend_from_slice(&u32::MAX.to_le_bytes());
    prop.extend_from_slice(b"short");
    assert!(matches!(
        ShaderMaterial::parse_property(&prop),
        Err(Error::UnexpectedEof { .. })
    ));
}

#[test]
fn a_texture_value_length_past_the_payload_errors_rather_than_allocating() {
    let mut prop = Vec::new();
    prop.extend_from_slice(&1u32.to_le_bytes());
    prop.extend_from_slice(&2u32.to_le_bytes());
    prop.extend_from_slice(b"P\0");
    prop.extend_from_slice(&0xFFFF_FF00u32.to_le_bytes());
    assert!(matches!(
        ShaderMaterial::parse_property(&prop),
        Err(Error::UnexpectedEof { .. })
    ));
}

#[test]
fn short_fixed_structs_are_rejected() {
    assert!(MaterialInfo::parse(&[0u8; 8]).is_err());
    assert_eq!(MaterialInfo::parse(&[0u8; 16]), Ok(MaterialInfo::default()));
    assert!(TextureInfo::parse(&[0u8; 11]).is_err());
    assert!(matches!(
        ShaderMaterial::default().parse_header(&[0u8; 36]),
        Err(Error::ShortStruct {
            expected: 37,
            actual: 36,
            ..
        })
    ));
}

#[test]
fn an_empty_payload_is_a_short_struct_not_a_panic() {
    assert!(MaterialInfo::parse(&[]).is_err());
    assert!(ShaderMaterial::parse_property(&[]).is_err());
}

#[test]
fn sfe_armydecal_is_linear_offset_on_stage0_and_rotate_on_stage1() {
    let info = VertexMaterialInfo {
        attributes: 0x0004_0800,
        ..VertexMaterialInfo::default()
    };
    assert_eq!(info.stage0_mapper(), UvMapperType::LinearOffset);
    assert_eq!(info.stage1_mapper(), UvMapperType::Rotate);
}

#[test]
fn sfe_menucredits_lightstream_is_sine_linear_offset_on_both_stages() {
    let info = VertexMaterialInfo {
        attributes: 0x0009_0900,
        ..VertexMaterialInfo::default()
    };
    assert_eq!(info.stage0_mapper(), UvMapperType::SineLinearOffset);
    assert_eq!(info.stage1_mapper(), UvMapperType::SineLinearOffset);
}

#[test]
fn sfe_menucredits_dwarvenelvish_is_linear_offset_on_both_stages() {
    let info = VertexMaterialInfo {
        attributes: 0x0004_0400,
        ..VertexMaterialInfo::default()
    };
    assert_eq!(info.stage0_mapper(), UvMapperType::LinearOffset);
    assert_eq!(info.stage1_mapper(), UvMapperType::LinearOffset);
}

#[test]
fn a_zero_attribute_word_is_uv_on_both_stages() {
    let info = VertexMaterialInfo::default();
    assert_eq!(info.stage0_mapper(), UvMapperType::Uv);
    assert_eq!(info.stage1_mapper(), UvMapperType::Uv);
}

#[test]
fn a_mapper_code_outside_the_documented_range_is_unknown_not_rejected() {
    let info = VertexMaterialInfo {
        attributes: 0x0067_0000,
        ..VertexMaterialInfo::default()
    };
    assert_eq!(info.stage0_mapper(), UvMapperType::Unknown(0x67));
    assert!(!UvMapperType::Unknown(0x67).is_implemented());
}

#[test]
fn is_implemented_tracks_what_the_renderer_actually_generates() {
    assert!(UvMapperType::Uv.is_implemented());
    assert!(UvMapperType::LinearOffset.is_implemented());
    assert!(UvMapperType::SineLinearOffset.is_implemented());
    assert!(UvMapperType::Rotate.is_implemented());
    assert!(UvMapperType::Grid.is_implemented());

    assert!(UvMapperType::Environment.is_implemented());
    assert!(UvMapperType::CheapEnvironment.is_implemented());
    assert!(UvMapperType::WsClassicEnv.is_implemented());
    assert!(UvMapperType::WsEnvironment.is_implemented());

    assert!(!UvMapperType::GridClassicEnv.is_implemented());
    assert!(!UvMapperType::GridEnvironment.is_implemented());
    assert!(!UvMapperType::GridWsClassicEnv.is_implemented());
    assert!(!UvMapperType::GridWsEnvironment.is_implemented());
    assert!(!UvMapperType::Unknown(0x67).is_implemented());
}

#[test]
fn sfe_armydecal_args0_and_args1_parse_crlf_and_the_semicolon_comment() {
    let args0 = MapperArgs::parse("UPerSec=0.3\r\nVPerSec=0.3\r\nUScale=.001\r\nVScale=.001");
    assert_eq!(args0.u_per_sec, 0.3);
    assert_eq!(args0.v_per_sec, 0.3);
    assert_eq!(args0.u_scale, 0.001);
    assert_eq!(args0.v_scale, 0.001);
    assert!(args0.unrecognized.is_empty());

    let args1 = MapperArgs::parse(
        "Speed=0.03;In Hertz. 1 = 1 rotate per second\r\nUCenter=0.5\r\nVCenter=0.5\r\n\
             UScale=1.0\r\nVScale=1.0",
    );
    assert_eq!(args1.speed, 0.03);
    assert_eq!(args1.u_center, 0.5);
    assert_eq!(args1.v_center, 0.5);
    assert_eq!(args1.u_scale, 1.0);
    assert_eq!(args1.v_scale, 1.0);
    assert!(
        args1.unrecognized.is_empty(),
        "the semicolon comment must not leak into an unrecognised key"
    );
}

#[test]
fn sine_linear_offset_args_parse_all_six_keys() {
    let args = MapperArgs::parse(
        "UAmp=-.1\r\nUFreq=.066\r\nUPhase=0.0\r\nVAmp=.1\r\nVFreq=.066\r\nVPhase=0.0",
    );
    assert_eq!(args.u_amp, -0.1);
    assert_eq!(args.u_freq, 0.066);
    assert_eq!(args.u_phase, 0.0);
    assert_eq!(args.v_amp, 0.1);
    assert_eq!(args.v_freq, 0.066);
    assert_eq!(args.v_phase, 0.0);
}

#[test]
fn a_plain_lf_line_ending_parses_the_same_as_crlf() {
    let crlf = MapperArgs::parse("UPerSec=1.0\r\nVPerSec=2.0");
    let lf = MapperArgs::parse("UPerSec=1.0\nVPerSec=2.0");
    assert_eq!(crlf, lf);
}

#[test]
fn missing_keys_take_the_documented_defaults() {
    let args = MapperArgs::parse("UPerSec=5.0");
    assert_eq!(args.u_per_sec, 5.0);
    assert_eq!(args.u_scale, 1.0);
    assert_eq!(args.v_scale, 1.0);
    assert_eq!(args.speed, 0.1);
    assert_eq!(args.u_amp, 1.0);
}

#[test]
fn an_unrecognised_key_is_kept_rather_than_dropped() {
    let args = MapperArgs::parse("Offsett=3\r\nWobble=7");
    assert_eq!(
        args.unrecognized,
        vec![
            ("Offsett".to_owned(), "3".to_owned()),
            ("Wobble".to_owned(), "7".to_owned()),
        ]
    );
}

#[test]
fn grid_args_parse_all_four_keys() {
    let args = MapperArgs::parse("FPS=15\r\nLog2Width=2\r\nLast=16\r\nOffset=3");
    assert_eq!(args.fps, 15.0);
    assert_eq!(args.log2_width, 2);
    assert_eq!(args.last, 16);
    assert_eq!(args.offset, 3);
    assert!(args.unrecognized.is_empty());
}

#[test]
fn a_non_numeric_last_leaves_the_whole_sheet_selected() {
    for text in [
        "FPS=30.0\r\nLog2Width=3\r\nLast= ;GridWidth*GridWidth; The last frame to use",
        "FPS=30.0\r\nLog2Width=3\r\nLast=GridWidth*GridWidth",
    ] {
        let args = MapperArgs::parse(text);
        assert_eq!(args.fps, 30.0, "{text}");
        assert_eq!(args.log2_width, 3, "{text}");
        assert_eq!(args.last, 0, "whole sheet: {text}");
        assert_eq!(
            args.unrecognized.len(),
            1,
            "the unparseable Last is still reported: {text}"
        );
    }
}

#[test]
fn a_negative_fractional_or_absurd_count_becomes_zero_or_saturates() {
    let args = MapperArgs::parse("Log2Width=-4\r\nLast=2.7\r\nOffset=1e30");
    assert_eq!(args.log2_width, 0, "a negative count is 0, not a huge u32");
    assert_eq!(args.last, 2, "truncated towards zero, not rounded");
    assert_eq!(
        args.offset,
        u32::MAX,
        "a large finite count saturates rather than wrapping"
    );
    let odd = MapperArgs::parse("Log2Width=nan\r\nLast=inf\r\nOffset=-inf");
    assert_eq!(odd.log2_width, 0, "NaN is 0 by the cast's own definition");
    assert_eq!(odd.last, 0, "infinity is the default, not a huge sheet");
    assert_eq!(odd.offset, 0);
}

#[test]
fn a_blank_or_comment_only_line_is_skipped_not_treated_as_a_key() {
    let args = MapperArgs::parse("\r\n; just a comment, no equals sign\r\nUPerSec=1.0\r\n");
    assert_eq!(args.u_per_sec, 1.0);
    assert!(args.unrecognized.is_empty());
}

#[test]
fn a_trailing_float_suffix_and_a_doubled_decimal_point_still_parse() {
    let args = MapperArgs::parse("UPerSec=0.3f\r\nVPerSec=1..5");
    assert_eq!(args.u_per_sec, 0.3);
    assert_eq!(args.v_per_sec, 1.5);
}

#[test]
fn bump_keys_read_a_leading_number_or_keep_the_default_as_ea_get_float_does() {
    for (value, want) in [
        ("scale factor applied to the bumps\t(DEFAULT = 1.0)", 1.0),
        (".3 scale factor applied to the bumps", 0.3),
        ("0.1f", 0.1),
        ("50%", 0.5),
        ("half%", 0.01),
    ] {
        let args = MapperArgs::parse(&format!("BumpScale = {value}"));
        assert_eq!(args.bump_scale, want, "{value}");
    }
}

#[test]
fn empty_text_yields_every_default_and_no_unrecognized_entries() {
    assert_eq!(MapperArgs::parse(""), MapperArgs::default());
}

#[test]
fn a_vertex_material_with_no_mapper_args_chunk_parses_as_the_defaults() {
    let vm = VertexMaterial::default();
    assert_eq!(vm.mapper_args0_parsed(), MapperArgs::default());
    assert_eq!(vm.mapper_args1_parsed(), MapperArgs::default());
}
