use crate::read::{u32_at, Cursor};
use crate::{Error, Result};

pub const EFFECT_TAG: u32 = 0xFEFF_0901;

const MAX_TYPE_DEPTH: u32 = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Class {
    Scalar,
    Vector,
    MatrixRows,
    MatrixColumns,
    Object,
    Struct,
}

pub mod kind {
    pub const BOOL: u32 = 1;
    pub const INT: u32 = 2;
    pub const FLOAT: u32 = 3;
    pub const STRING: u32 = 4;
    pub const SAMPLERS: core::ops::RangeInclusive<u32> = 10..=14;
    pub const PIXELSHADER: u32 = 15;
    pub const VERTEXSHADER: u32 = 16;
}

#[derive(Debug, Clone, PartialEq)]
pub struct Type {
    pub kind: u32,
    pub class: Class,
    pub name: String,
    pub semantic: String,
    pub elements: u32,
    pub rows: u32,
    pub columns: u32,
    pub members: Vec<Type>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct State {
    pub operation: u32,
    pub index: u32,
    pub value: Parameter,
    pub resource: Option<Resource>,
}

pub mod op {
    pub const ZENABLE: u32 = 0x00;
    pub const ZWRITEENABLE: u32 = 0x03;
    pub const ALPHATESTENABLE: u32 = 0x04;
    pub const SRCBLEND: u32 = 0x06;
    pub const DESTBLEND: u32 = 0x07;
    pub const CULLMODE: u32 = 0x08;
    pub const ZFUNC: u32 = 0x09;
    pub const ALPHABLENDENABLE: u32 = 0x0D;
    pub const COLORWRITEENABLE: u32 = 0x49;
    pub const TSS_FIRST: u32 = 0x67;
    pub const TSS_LAST: u32 = 0x78;
    pub const VERTEXSHADER: u32 = 0x92;
    pub const PIXELSHADER: u32 = 0x93;
    pub const TEXTURE: u32 = 0xA4;
    pub const ADDRESSU: u32 = 0xA5;
    pub const ADDRESSV: u32 = 0xA6;
    pub const MAGFILTER: u32 = 0xA9;
    pub const MINFILTER: u32 = 0xAA;
    pub const MIPFILTER: u32 = 0xAB;
}

#[derive(Debug, Clone, PartialEq)]
pub enum Resource {
    Data(Vec<u8>),
    Parameter(String),
    ArraySelector { name: String, preshader: Vec<u8> },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Parameter {
    pub ty: Type,
    pub flags: u32,
    pub words: Vec<u32>,
    pub samplers: Vec<Vec<State>>,
    pub annotations: Vec<Parameter>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Pass {
    pub name: String,
    pub states: Vec<State>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Technique {
    pub name: String,
    pub passes: Vec<Pass>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Effect {
    pub parameters: Vec<Parameter>,
    pub techniques: Vec<Technique>,
    pub objects: Vec<Option<Vec<u8>>>,
}

struct Reader<'a> {
    data: &'a [u8],
}

impl<'a> Reader<'a> {
    fn string(&self, offset: u32) -> Result<String> {
        let at = offset as usize;
        let len = u32_at(self.data, at)? as usize;
        let mut cursor = Cursor::new(self.data, at.saturating_add(4));
        let bytes = cursor.padded(len)?;
        let end = bytes.iter().position(|b| *b == 0).unwrap_or(bytes.len());
        Ok(String::from_utf8_lossy(bytes.get(..end).unwrap_or(bytes)).into_owned())
    }

    fn typedef(&self, cursor: &mut Cursor<'a>, depth: u32) -> Result<Type> {
        if depth > MAX_TYPE_DEPTH {
            return Err(Error::TooDeep { offset: cursor.pos });
        }
        let kind = cursor.u32()?;
        let class_raw = cursor.u32()?;
        let name = self.string(cursor.u32()?)?;
        let semantic = self.string(cursor.u32()?)?;
        let elements = cursor.u32()?;
        let class = match class_raw {
            0 => Class::Scalar,
            1 => Class::Vector,
            2 => Class::MatrixRows,
            3 => Class::MatrixColumns,
            4 => Class::Object,
            5 => Class::Struct,
            other => {
                return Err(Error::Unsupported {
                    what: "parameter class",
                    value: other,
                    offset: cursor.pos,
                })
            }
        };
        let (mut rows, mut columns, mut members) = (0, 0, Vec::new());
        match class {
            Class::Vector => {
                columns = cursor.u32()?;
                rows = cursor.u32()?;
            }
            Class::Scalar | Class::MatrixRows | Class::MatrixColumns => {
                rows = cursor.u32()?;
                columns = cursor.u32()?;
            }
            Class::Struct => {
                let count = cursor.u32()?;
                let room = self.data.len().saturating_sub(cursor.pos) / 20;
                if count as usize > room {
                    return Err(Error::CountTooLarge {
                        count,
                        offset: cursor.pos,
                    });
                }
                for _ in 0..count {
                    members.push(self.typedef(cursor, depth.saturating_add(1))?);
                }
            }
            Class::Object => {}
        }
        Ok(Type {
            kind,
            class,
            name,
            semantic,
            elements,
            rows,
            columns,
            members,
        })
    }

    fn value(
        &self,
        ty: &Type,
        cursor: &mut Cursor<'a>,
        words: &mut Vec<u32>,
        samplers: &mut Vec<Vec<State>>,
        depth: u32,
    ) -> Result<()> {
        if depth > MAX_TYPE_DEPTH {
            return Err(Error::TooDeep { offset: cursor.pos });
        }
        let elements = ty.elements.max(1);
        let room = self.data.len().saturating_sub(cursor.pos);
        if elements as usize > room {
            return Err(Error::CountTooLarge {
                count: elements,
                offset: cursor.pos,
            });
        }
        for _ in 0..elements {
            match ty.class {
                Class::Struct => {
                    for member in &ty.members {
                        let single = Type {
                            elements: member.elements,
                            ..member.clone()
                        };
                        self.value(&single, cursor, words, samplers, depth.saturating_add(1))?;
                    }
                }
                Class::Object if kind::SAMPLERS.contains(&ty.kind) => {
                    let count = cursor.u32()?;
                    let room = self.data.len().saturating_sub(cursor.pos) / 16;
                    if count as usize > room {
                        return Err(Error::CountTooLarge {
                            count,
                            offset: cursor.pos,
                        });
                    }
                    let mut states = Vec::with_capacity(count as usize);
                    for _ in 0..count {
                        states.push(self.state(cursor, depth.saturating_add(1))?);
                    }
                    samplers.push(states);
                }
                Class::Object => words.push(cursor.u32()?),
                _ => {
                    let n = ty.rows.saturating_mul(ty.columns);
                    let room = self.data.len().saturating_sub(cursor.pos) / 4;
                    if n as usize > room {
                        return Err(Error::CountTooLarge {
                            count: n,
                            offset: cursor.pos,
                        });
                    }
                    for _ in 0..n {
                        words.push(cursor.u32()?);
                    }
                }
            }
        }
        Ok(())
    }

    fn parameter_at(&self, typedef: u32, value: u32, depth: u32) -> Result<Parameter> {
        let mut types = Cursor::new(self.data, typedef as usize);
        let ty = self.typedef(&mut types, depth)?;
        let mut values = Cursor::new(self.data, value as usize);
        let (mut words, mut samplers) = (Vec::new(), Vec::new());
        self.value(&ty, &mut values, &mut words, &mut samplers, depth)?;
        Ok(Parameter {
            ty,
            flags: 0,
            words,
            samplers,
            annotations: Vec::new(),
        })
    }

    fn state(&self, cursor: &mut Cursor<'a>, depth: u32) -> Result<State> {
        let operation = cursor.u32()?;
        let index = cursor.u32()?;
        let typedef = cursor.u32()?;
        let value = cursor.u32()?;
        Ok(State {
            operation,
            index,
            value: self.parameter_at(typedef, value, depth)?,
            resource: None,
        })
    }

    fn annotations(&self, cursor: &mut Cursor<'a>, count: u32) -> Result<Vec<Parameter>> {
        let room = self.data.len().saturating_sub(cursor.pos) / 8;
        if count as usize > room {
            return Err(Error::CountTooLarge {
                count,
                offset: cursor.pos,
            });
        }
        let mut out = Vec::with_capacity(count as usize);
        for _ in 0..count {
            let typedef = cursor.u32()?;
            let value = cursor.u32()?;
            out.push(self.parameter_at(typedef, value, 0)?);
        }
        Ok(out)
    }
}

impl Effect {
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        let tag = u32_at(bytes, 0)?;
        if tag != EFFECT_TAG {
            return Err(Error::NotAnEffect { tag });
        }
        let start = u32_at(bytes, 4)?;
        let data = bytes.get(8..).ok_or(Error::Truncated { offset: 8 })?;
        let reader = Reader { data };
        let mut cursor = Cursor::new(data, start as usize);
        let parameter_count = cursor.u32()?;
        let technique_count = cursor.u32()?;
        let _shader_count = cursor.u32()?;
        let object_count = cursor.u32()?;
        let room = data.len() / 8;
        for count in [parameter_count, technique_count, object_count] {
            if count as usize > room {
                return Err(Error::CountTooLarge {
                    count,
                    offset: cursor.pos,
                });
            }
        }
        let mut parameters = Vec::with_capacity(parameter_count as usize);
        for _ in 0..parameter_count {
            let typedef = cursor.u32()?;
            let value = cursor.u32()?;
            let flags = cursor.u32()?;
            let annotation_count = cursor.u32()?;
            let annotations = reader.annotations(&mut cursor, annotation_count)?;
            let mut parameter = reader.parameter_at(typedef, value, 0)?;
            parameter.flags = flags;
            parameter.annotations = annotations;
            parameters.push(parameter);
        }
        let mut techniques = Vec::with_capacity(technique_count as usize);
        for _ in 0..technique_count {
            let name = reader.string(cursor.u32()?)?;
            let annotation_count = cursor.u32()?;
            let pass_count = cursor.u32()?;
            reader.annotations(&mut cursor, annotation_count)?;
            if pass_count as usize > data.len() / 12 {
                return Err(Error::CountTooLarge {
                    count: pass_count,
                    offset: cursor.pos,
                });
            }
            let mut passes = Vec::with_capacity(pass_count as usize);
            for _ in 0..pass_count {
                let name = reader.string(cursor.u32()?)?;
                let annotation_count = cursor.u32()?;
                let state_count = cursor.u32()?;
                reader.annotations(&mut cursor, annotation_count)?;
                if state_count as usize > data.len() / 16 {
                    return Err(Error::CountTooLarge {
                        count: state_count,
                        offset: cursor.pos,
                    });
                }
                let mut states = Vec::with_capacity(state_count as usize);
                for _ in 0..state_count {
                    states.push(reader.state(&mut cursor, 0)?);
                }
                passes.push(Pass { name, states });
            }
            techniques.push(Technique { name, passes });
        }
        let string_count = cursor.u32()?;
        let resource_count = cursor.u32()?;
        let mut objects: Vec<Option<Vec<u8>>> = vec![None; object_count as usize];
        for _ in 0..string_count {
            let id = cursor.u32()?;
            let size = cursor.u32()? as usize;
            let blob = cursor.padded(size)?.to_vec();
            let slot = objects.get_mut(id as usize).ok_or(Error::Unsupported {
                what: "object id",
                value: id,
                offset: cursor.pos,
            })?;
            *slot = Some(blob);
        }
        let mut effect = Self {
            parameters,
            techniques,
            objects,
        };
        for _ in 0..resource_count {
            let technique = cursor.u32()?;
            let index = cursor.u32()?;
            let element = cursor.u32()?;
            let state_index = cursor.u32()?;
            let usage = cursor.u32()?;
            let size = cursor.u32()? as usize;
            let at = cursor.pos;
            let blob = cursor.padded(size)?;
            let resource = match usage {
                0 => Resource::Data(blob.to_vec()),
                1 => Resource::Parameter(c_string(blob)),
                2 => {
                    let len = u32_at(blob, 0)? as usize;
                    let mut inner = Cursor::new(blob, 4);
                    let name = c_string(inner.padded(len)?);
                    Resource::ArraySelector {
                        name,
                        preshader: blob.get(inner.pos..).unwrap_or_default().to_vec(),
                    }
                }
                other => {
                    return Err(Error::Unsupported {
                        what: "resource usage",
                        value: other,
                        offset: at,
                    })
                }
            };
            let state = if technique == u32::MAX {
                effect
                    .parameters
                    .get_mut(index as usize)
                    .and_then(|p| p.samplers.get_mut(element as usize))
                    .and_then(|s| s.get_mut(state_index as usize))
            } else {
                effect
                    .techniques
                    .get_mut(technique as usize)
                    .and_then(|t| t.passes.get_mut(index as usize))
                    .and_then(|p| p.states.get_mut(state_index as usize))
            };
            let state = state.ok_or(Error::Unsupported {
                what: "resource target",
                value: state_index,
                offset: at,
            })?;
            state.resource = Some(resource);
        }
        Ok(effect)
    }

    #[must_use]
    pub fn parameter(&self, name: &str) -> Option<&Parameter> {
        self.parameters
            .iter()
            .find(|p| p.ty.name.eq_ignore_ascii_case(name))
    }

    #[must_use]
    pub fn technique(&self, name: &str) -> Option<&Technique> {
        self.techniques
            .iter()
            .find(|t| t.name.eq_ignore_ascii_case(name))
    }

    #[must_use]
    pub fn string_value(&self, parameter: &Parameter) -> Option<String> {
        let id = *parameter.words.first()?;
        self.objects.get(id as usize)?.as_deref().map(c_string)
    }

    #[must_use]
    pub fn state_shader<'s>(&'s self, state: &'s State, element: usize) -> Option<&'s [u8]> {
        match state.resource.as_ref()? {
            Resource::Data(bytes) => Some(bytes),
            Resource::Parameter(name) | Resource::ArraySelector { name, .. } => {
                let parameter = self.parameter(name)?;
                let id = *parameter.words.get(element)?;
                self.objects.get(id as usize)?.as_deref()
            }
        }
    }
}

fn c_string(bytes: &[u8]) -> String {
    let end = bytes.iter().position(|b| *b == 0).unwrap_or(bytes.len());
    String::from_utf8_lossy(bytes.get(..end).unwrap_or(bytes)).into_owned()
}
