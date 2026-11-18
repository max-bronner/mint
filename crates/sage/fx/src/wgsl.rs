use crate::shader::{reg, Dst, Instruction, Shader, Src, Stage};
use crate::{Error, Result};
use core::fmt::Write as _;

pub struct Target<'a> {
    pub vertex_type: &'a str,
    pub vertex_input: &'a dyn Fn(u8, u8) -> Option<String>,
    pub group: u32,
    pub uniform_binding: u32,
    pub first_texture_binding: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Translation {
    pub source: String,
    pub vs_registers: u32,
    pub ps_registers: u32,
    pub vs_bools: u32,
    pub ps_bools: u32,
    pub texture_slots: u32,
    pub vs_defs: Vec<(u32, [f32; 4])>,
    pub ps_defs: Vec<(u32, [f32; 4])>,
}

impl Translation {
    #[must_use]
    pub fn uniform_size(&self) -> u32 {
        self.vs_registers
            .saturating_add(self.ps_registers)
            .saturating_add(self.vs_bools)
            .saturating_add(self.ps_bools)
            .saturating_add(1)
            .saturating_mul(16)
    }

    #[must_use]
    pub fn block(
        &self,
        vs: &[f32],
        ps: &[f32],
        vs_bools: &[f32],
        ps_bools: &[f32],
        width: u32,
        height: u32,
    ) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.uniform_size() as usize);
        for (registers, values, defs) in [
            (self.vs_registers, vs, &self.vs_defs),
            (self.ps_registers, ps, &self.ps_defs),
        ] {
            let count = (registers as usize).saturating_mul(4);
            let mut half: Vec<f32> = (0..count)
                .map(|i| values.get(i).copied().unwrap_or(0.0))
                .collect();
            for (register, value) in defs {
                for (k, v) in value.iter().enumerate() {
                    let at = (*register as usize).saturating_mul(4).saturating_add(k);
                    if let Some(slot) = half.get_mut(at) {
                        *slot = *v;
                    }
                }
            }
            for v in half {
                out.extend_from_slice(&v.to_le_bytes());
            }
        }
        for (count, values) in [(self.vs_bools, vs_bools), (self.ps_bools, ps_bools)] {
            for i in 0..count as usize {
                let v = values.get(i).copied().unwrap_or(0.0);
                for c in [v, 0.0, 0.0, 0.0] {
                    out.extend_from_slice(&c.to_le_bytes());
                }
            }
        }
        let raster = [
            -1.0 / width.max(1) as f32,
            1.0 / height.max(1) as f32,
            0.0,
            0.0,
        ];
        for v in raster {
            out.extend_from_slice(&v.to_le_bytes());
        }
        out
    }
}

fn extents(shader: &Shader) -> (u32, u32, u32, u32) {
    let (mut temps, mut consts, mut samplers, mut bools) = (0u32, 0u32, 0u32, 0u32);
    let mut see = |kind: u32, index: u32| {
        let next = index.saturating_add(1);
        match kind {
            reg::TEMP => temps = temps.max(next),
            reg::CONST => consts = consts.max(next),
            reg::CONSTBOOL => bools = bools.max(next),
            reg::SAMPLER => samplers = samplers.max(next),
            _ => {}
        }
    };
    for ins in &shader.instructions {
        if let Some(d) = ins.dst {
            see(d.reg.kind, d.reg.index);
        }
        for s in &ins.src {
            see(s.reg.kind, s.reg.index);
        }
    }
    for c in &shader.constants {
        if c.register_set == 2 {
            consts =
                consts.max(u32::from(c.register_index).saturating_add(u32::from(c.register_count)));
        }
    }
    if let Some(pre) = &shader.preshader {
        for op in &pre.ops {
            if op.output.table == crate::preshader::TABLE_OUTPUT {
                consts = consts.max(
                    (op.output
                        .offset
                        .saturating_add(op.components)
                        .saturating_add(3))
                        / 4,
                );
            }
        }
    }
    (temps, consts, samplers, bools)
}

fn defs(shader: &Shader) -> Vec<(u32, [f32; 4])> {
    shader
        .instructions
        .iter()
        .filter(|i| i.opcode == 81)
        .filter_map(|i| {
            let d = i.dst?;
            let mut value = [0.0f32; 4];
            for (slot, word) in value.iter_mut().zip(&i.literal) {
                *slot = f32::from_bits(*word);
            }
            Some((d.reg.index, value))
        })
        .collect()
}

fn float(v: f32) -> String {
    if v.is_finite() {
        format!("{v:?}")
    } else if v.is_nan() {
        "0.0".to_owned()
    } else if v > 0.0 {
        "3.4028235e38".to_owned()
    } else {
        "-3.4028235e38".to_owned()
    }
}

struct Emitter<'s> {
    shader: &'s Shader,
    sm3: bool,
    half: &'static str,
    registers: u32,
    defs: Vec<(u32, [f32; 4])>,
}

impl Emitter<'_> {
    fn unsupported(&self, what: &'static str, value: u32, ins: &Instruction) -> Error {
        Error::Unsupported {
            what,
            value,
            offset: ins.offset,
        }
    }

    fn register(&self, src: &Src, ins: &Instruction) -> Result<String> {
        let index = src.reg.index;
        Ok(match (self.shader.stage, src.reg.kind) {
            (_, reg::TEMP) => format!("r{index}"),
            (Stage::Vertex, reg::INPUT) => format!("v{index}"),
            (Stage::Pixel, reg::INPUT) if self.sm3 => format!("v{index}"),
            (Stage::Pixel, reg::INPUT) => format!("vin.d{index}"),
            (Stage::Pixel, reg::ADDR_OR_TEXTURE) => format!("vin.t{index}"),
            (Stage::Vertex, reg::ADDR_OR_TEXTURE) => "vec4<f32>(a0)".to_owned(),
            (_, reg::CONST) => match src.relative {
                Some((address, component)) => {
                    if address.kind != reg::ADDR_OR_TEXTURE || self.shader.stage != Stage::Vertex {
                        return Err(self.unsupported(
                            "relative address register",
                            address.kind,
                            ins,
                        ));
                    }
                    let c = ["x", "y", "z", "w"]
                        .get(usize::from(component))
                        .copied()
                        .unwrap_or("x");
                    format!(
                        "fx.{}[u32(clamp(a0.{c} + {index}, 0, {}))]",
                        self.half,
                        self.registers.saturating_sub(1)
                    )
                }
                None => match self.defs.iter().find(|(r, _)| *r == index) {
                    Some((_, v)) => format!(
                        "vec4<f32>({}, {}, {}, {})",
                        float(v[0]),
                        float(v[1]),
                        float(v[2]),
                        float(v[3])
                    ),
                    None => format!("fx.{}[{index}]", self.half),
                },
            },
            (_, kind) => return Err(self.unsupported("source register type", kind, ins)),
        })
    }

    fn source(&self, src: &Src, ins: &Instruction) -> Result<String> {
        let base = self.register(src, ins)?;
        let sw: String = src
            .swizzle
            .iter()
            .map(|c| {
                ['x', 'y', 'z', 'w']
                    .get(usize::from(*c))
                    .copied()
                    .unwrap_or('x')
            })
            .collect();
        let e = if sw == "xyzw" {
            base
        } else {
            format!("({base}).{sw}")
        };
        Ok(match src.modifier {
            0 => e,
            1 => format!("(-{e})"),
            2 => format!("({e} - vec4<f32>(0.5))"),
            3 => format!("(vec4<f32>(0.5) - {e})"),
            4 => format!("(2.0 * {e} - vec4<f32>(1.0))"),
            5 => format!("(vec4<f32>(1.0) - 2.0 * {e})"),
            6 => format!("(vec4<f32>(1.0) - {e})"),
            7 => format!("(2.0 * {e})"),
            8 => format!("(-2.0 * {e})"),
            11 => format!("abs({e})"),
            12 => format!("(-abs({e}))"),
            other => return Err(self.unsupported("source modifier", u32::from(other), ins)),
        })
    }

    fn destination(&self, d: &Dst, ins: &Instruction) -> Result<String> {
        let index = d.reg.index;
        Ok(match (self.shader.stage, d.reg.kind) {
            (_, reg::TEMP) => format!("r{index}"),
            (Stage::Vertex, reg::RASTOUT) => match index {
                0 => "o_pos".to_owned(),
                _ => "o_unused".to_owned(),
            },
            (Stage::Vertex, reg::ATTROUT) if index < 2 => format!("o_d{index}"),
            (Stage::Vertex, reg::TEXCRDOUT) if self.sm3 && index < 12 => format!("o{index}"),
            (Stage::Vertex, reg::TEXCRDOUT) if index < 8 => format!("o_t{index}"),
            (Stage::Pixel, reg::COLOROUT) if index == 0 => "o_c0".to_owned(),
            (_, kind) => return Err(self.unsupported("destination register type", kind, ins)),
        })
    }

    fn instruction(&self, ins: &Instruction, out: &mut String) -> Result<()> {
        match ins.opcode {
            31 | 81 | 47 | 48 | 0 => return Ok(()),
            40 => {
                let cond = ins
                    .src
                    .first()
                    .filter(|s| s.reg.kind == reg::CONSTBOOL)
                    .ok_or_else(|| self.unsupported("if on a non-boolean register", 0, ins))?;
                let _ = writeln!(
                    out,
                    "    if (fx.{}b[{}].x != 0.0) {{",
                    self.half, cond.reg.index
                );
                return Ok(());
            }
            41 => {
                let op = match ins.controls {
                    1 => ">",
                    2 => "==",
                    3 => ">=",
                    4 => "<",
                    5 => "!=",
                    6 => "<=",
                    other => return Err(self.unsupported("ifc comparison", u32::from(other), ins)),
                };
                let a = ins
                    .src
                    .first()
                    .ok_or_else(|| self.unsupported("operand count", 41, ins))?;
                let b = ins
                    .src
                    .get(1)
                    .ok_or_else(|| self.unsupported("operand count", 41, ins))?;
                let (a, b) = (self.source(a, ins)?, self.source(b, ins)?);
                let _ = writeln!(out, "    if (({a}).x {op} ({b}).x) {{");
                return Ok(());
            }
            42 => {
                out.push_str(
                    "    } else {
",
                );
                return Ok(());
            }
            43 => {
                out.push_str(
                    "    }
",
                );
                return Ok(());
            }
            _ => {}
        }
        let sources: Vec<String> = ins
            .src
            .iter()
            .filter(|s| s.reg.kind != reg::SAMPLER)
            .map(|s| self.source(s, ins))
            .collect::<Result<_>>()?;
        let arg = |n: usize| -> Result<&str> {
            sources
                .get(n)
                .map(String::as_str)
                .ok_or_else(|| self.unsupported("operand count", ins.opcode, ins))
        };
        let dst = ins
            .dst
            .ok_or_else(|| self.unsupported("instruction without destination", ins.opcode, ins))?;
        if self.shader.stage == Stage::Vertex && dst.reg.kind == reg::ADDR_OR_TEXTURE {
            let round = if ins.opcode == 46 {
                "floor({s} + vec4<f32>(0.5))"
            } else {
                "floor({s})"
            };
            let value = round.replace("{s}", arg(0)?);
            let _ = writeln!(out, "    {{ let t = vec4<i32>({value});");
            for (bit, c) in ["x", "y", "z", "w"].iter().enumerate() {
                if dst.mask & (1 << bit) != 0 {
                    let _ = writeln!(out, "      a0.{c} = t.{c};");
                }
            }
            out.push_str("    }\n");
            return Ok(());
        }
        let value = match ins.opcode {
            1 => arg(0)?.to_owned(),
            2 => format!("{} + {}", arg(0)?, arg(1)?),
            3 => format!("{} - {}", arg(0)?, arg(1)?),
            4 => format!("{} * {} + {}", arg(0)?, arg(1)?, arg(2)?),
            5 => format!("{} * {}", arg(0)?, arg(1)?),
            6 => format!("vec4<f32>(1.0 / ({}).x)", arg(0)?),
            7 => format!("vec4<f32>(inverseSqrt(abs(({}).x)))", arg(0)?),
            8 => format!("vec4<f32>(dot(({}).xyz, ({}).xyz))", arg(0)?, arg(1)?),
            9 => format!("vec4<f32>(dot({}, {}))", arg(0)?, arg(1)?),
            10 => format!("min({}, {})", arg(0)?, arg(1)?),
            11 => format!("max({}, {})", arg(0)?, arg(1)?),
            12 => format!(
                "select(vec4<f32>(0.0), vec4<f32>(1.0), {} < {})",
                arg(0)?,
                arg(1)?
            ),
            13 => format!(
                "select(vec4<f32>(0.0), vec4<f32>(1.0), {} >= {})",
                arg(0)?,
                arg(1)?
            ),
            14 => format!("vec4<f32>(exp(({}).x))", arg(0)?),
            15 => format!("vec4<f32>(log2(abs(({}).x)))", arg(0)?),
            18 => format!("{} * ({} - {}) + {}", arg(0)?, arg(1)?, arg(2)?, arg(2)?),
            19 => format!("fract({})", arg(0)?),
            32 => format!("vec4<f32>(pow(abs(({}).x), ({}).x))", arg(0)?, arg(1)?),
            33 => format!(
                "vec4<f32>(cross(({}).xyz, ({}).xyz), 0.0)",
                arg(0)?,
                arg(1)?
            ),
            35 => format!("abs({})", arg(0)?),
            36 => format!("({0}) * inverseSqrt(dot(({0}).xyz, ({0}).xyz))", arg(0)?),
            37 => format!("vec4<f32>(cos(({0}).x), sin(({0}).x), 0.0, 0.0)", arg(0)?),
            80 => format!(
                "select({}, {}, {} > vec4<f32>(0.5))",
                arg(2)?,
                arg(1)?,
                arg(0)?
            ),
            88 => format!(
                "select({}, {}, {} >= vec4<f32>(0.0))",
                arg(2)?,
                arg(1)?,
                arg(0)?
            ),
            90 => format!(
                "vec4<f32>(dot(({}).xy, ({}).xy) + ({}).x)",
                arg(0)?,
                arg(1)?,
                arg(2)?
            ),
            66 if self.shader.stage == Stage::Pixel && self.shader.major >= 2 => {
                let sampler = ins
                    .src
                    .iter()
                    .find(|s| s.reg.kind == reg::SAMPLER)
                    .ok_or_else(|| self.unsupported("texld without sampler", 0, ins))?;
                let n = sampler.reg.index;
                let coord = arg(0)?;
                match ins.controls {
                    0 => format!("textureSample(fx_tex{n}, fx_samp{n}, ({coord}).xy)"),
                    1 => {
                        format!("textureSample(fx_tex{n}, fx_samp{n}, ({coord}).xy / ({coord}).w)")
                    }
                    2 => format!(
                        "textureSampleBias(fx_tex{n}, fx_samp{n}, ({coord}).xy, ({coord}).w)"
                    ),
                    other => return Err(self.unsupported("texld variant", u32::from(other), ins)),
                }
            }
            65 => {
                let target = self.destination(&dst, ins)?;
                let _ = writeln!(
                    out,
                    "    if (any({target}.xyz < vec3<f32>(0.0))) {{ discard; }}"
                );
                return Ok(());
            }
            other => {
                return Err(self.unsupported("shader opcode", other, ins));
            }
        };
        let mut value = value;
        if dst.shift != 0 {
            let scale = if dst.shift > 0 {
                f32::from(1u8 << dst.shift.unsigned_abs().min(7))
            } else {
                1.0 / f32::from(1u8 << dst.shift.unsigned_abs().min(7))
            };
            value = format!("({value}) * {}", float(scale));
        }
        if dst.modifiers & 1 != 0 {
            value = format!("clamp({value}, vec4<f32>(0.0), vec4<f32>(1.0))");
        }
        let target = self.destination(&dst, ins)?;
        let _ = writeln!(out, "    {{ let t = {value};");
        for (bit, c) in ["x", "y", "z", "w"].iter().enumerate() {
            if dst.mask & (1 << bit) != 0 {
                let _ = writeln!(out, "      {target}.{c} = t.{c};");
            }
        }
        out.push_str("    }\n");
        Ok(())
    }
}

fn declared(shader: &Shader, kind: u32) -> Vec<(u32, u8, u8)> {
    shader
        .instructions
        .iter()
        .filter(|i| i.opcode == 31)
        .filter_map(|i| {
            let d = i.dst?;
            let decl = i.decl?;
            (d.reg.kind == kind).then_some((d.reg.index, decl.usage, decl.usage_index))
        })
        .collect()
}

pub fn translate(vs: &Shader, ps: &Shader, target: &Target<'_>) -> Result<Translation> {
    let sm3 = vs.major >= 3;
    if vs.stage != Stage::Vertex || vs.major > 3 {
        return Err(Error::Unsupported {
            what: "vertex shader model",
            value: u32::from(vs.major),
            offset: 0,
        });
    }
    if ps.stage != Stage::Pixel || ps.major != if sm3 { 3 } else { 2 } {
        return Err(Error::Unsupported {
            what: "pixel shader model",
            value: u32::from(ps.major),
            offset: 0,
        });
    }
    let (vs_temps, vs_consts, _, vs_bools) = extents(vs);
    let (ps_temps, ps_consts, samplers, ps_bools) = extents(ps);
    let vs_bools = vs_bools.max(1);
    let ps_bools = ps_bools.max(1);
    let vs_registers = vs_consts.max(1);
    let ps_registers = ps_consts.max(1);
    let mut src = String::new();
    let _ = writeln!(
        src,
        "// Translated from D3D9 bytecode (vs_{}_{}, ps_{}_{}) by mint-sage-fx.\n\
         struct FxBlock {{\n    vs: array<vec4<f32>, {vs_registers}>,\n    ps: array<vec4<f32>, {ps_registers}>,\n    vsb: array<vec4<f32>, {vs_bools}>,\n    psb: array<vec4<f32>, {ps_bools}>,\n    raster: vec4<f32>,\n}};\n\
         @group({}) @binding({}) var<uniform> fx: FxBlock;",
        vs.major, vs.minor, ps.major, ps.minor, target.group, target.uniform_binding
    );
    for n in 0..samplers {
        let binding = target
            .first_texture_binding
            .saturating_add(n.saturating_mul(2));
        let _ = writeln!(
            src,
            "@group({g}) @binding({binding}) var fx_tex{n}: texture_2d<f32>;\n@group({g}) @binding({}) var fx_samp{n}: sampler;",
            binding.saturating_add(1),
            g = target.group
        );
    }
    for i in &ps.instructions {
        if let (31, Some(d), Some(decl)) = (i.opcode, i.dst, i.decl) {
            if d.reg.kind == reg::SAMPLER && decl.texture_type != 2 {
                return Err(Error::Unsupported {
                    what: "sampler texture type",
                    value: u32::from(decl.texture_type),
                    offset: i.offset,
                });
            }
            if d.reg.kind == reg::MISCTYPE {
                return Err(Error::Unsupported {
                    what: "pixel input vPos/vFace",
                    value: d.reg.index,
                    offset: i.offset,
                });
            }
        }
    }
    let vs_outputs = if sm3 {
        declared(vs, reg::TEXCRDOUT)
    } else {
        Vec::new()
    };
    let ps_inputs = if sm3 {
        declared(ps, reg::INPUT)
    } else {
        Vec::new()
    };
    src.push_str("struct FxVaryings {\n    @builtin(position) position: vec4<f32>,\n");
    if sm3 {
        for n in 0..ps_inputs.len() {
            let _ = writeln!(src, "    @location({n}) i{n}: vec4<f32>,");
        }
    } else {
        src.push_str("    @location(0) d0: vec4<f32>,\n    @location(1) d1: vec4<f32>,\n");
        for n in 0u32..8 {
            let _ = writeln!(
                src,
                "    @location({}) t{n}: vec4<f32>,",
                n.saturating_add(2)
            );
        }
    }
    src.push_str("};\n\n");

    let _ = writeln!(
        src,
        "@vertex\nfn vs_main(vin: {}) -> FxVaryings {{",
        target.vertex_type
    );
    for n in 0..vs_temps {
        let _ = writeln!(src, "    var r{n} = vec4<f32>(0.0);");
    }
    src.push_str("    var a0 = vec4<i32>(0);\n");
    if sm3 {
        for (index, _, _) in &vs_outputs {
            let _ = writeln!(src, "    var o{index} = vec4<f32>(0.0, 0.0, 0.0, 1.0);");
        }
    } else {
        src.push_str(
            "    var o_pos = vec4<f32>(0.0, 0.0, 0.0, 1.0);\n    var o_unused = vec4<f32>(0.0);\n    var o_d0 = vec4<f32>(1.0);\n    var o_d1 = vec4<f32>(0.0);\n",
        );
        for n in 0..8 {
            let _ = writeln!(src, "    var o_t{n} = vec4<f32>(0.0, 0.0, 0.0, 1.0);");
        }
    }
    for i in &vs.instructions {
        if let (31, Some(d), Some(decl)) = (i.opcode, i.dst, i.decl) {
            if d.reg.kind == reg::INPUT {
                let expr = (target.vertex_input)(decl.usage, decl.usage_index).ok_or(
                    Error::Unsupported {
                        what: "vertex input usage",
                        value: u32::from(decl.usage),
                        offset: i.offset,
                    },
                )?;
                let _ = writeln!(src, "    let v{} = {expr};", d.reg.index);
            }
        }
    }
    let emitter = Emitter {
        shader: vs,
        sm3,
        half: "vs",
        registers: vs_registers,
        defs: defs(vs),
    };
    for i in &vs.instructions {
        emitter.instruction(i, &mut src)?;
    }
    src.push_str("    _ = a0;\n    var out: FxVaryings;\n");
    if sm3 {
        let position = vs_outputs
            .iter()
            .find(|(_, usage, index)| (*usage, *index) == (0, 0))
            .ok_or(Error::Unsupported {
                what: "vertex shader without a position output",
                value: 0,
                offset: 0,
            })?;
        let _ = writeln!(
            src,
            "    out.position = vec4<f32>(o{0}.xy + fx.raster.xy * o{0}.w, o{0}.zw);",
            position.0
        );
        for (n, (_, usage, index)) in ps_inputs.iter().enumerate() {
            match vs_outputs
                .iter()
                .find(|(_, u, i)| (*u, *i) == (*usage, *index))
            {
                Some((register, _, _)) => {
                    let _ = writeln!(src, "    out.i{n} = o{register};");
                }
                None => {
                    let _ = writeln!(src, "    out.i{n} = vec4<f32>(0.0);");
                }
            }
        }
    } else {
        src.push_str(
            "    _ = o_unused;\n    out.position = vec4<f32>(o_pos.xy + fx.raster.xy * o_pos.w, o_pos.zw);\n    out.d0 = clamp(o_d0, vec4<f32>(0.0), vec4<f32>(1.0));\n    out.d1 = clamp(o_d1, vec4<f32>(0.0), vec4<f32>(1.0));\n",
        );
        for n in 0..8 {
            let _ = writeln!(src, "    out.t{n} = o_t{n};");
        }
    }
    src.push_str("    return out;\n}\n\n");

    src.push_str("@fragment\nfn fs_main(vin: FxVaryings) -> @location(0) vec4<f32> {\n");
    for (n, (register, _, _)) in ps_inputs.iter().enumerate() {
        let _ = writeln!(src, "    let v{register} = vin.i{n};");
    }
    for n in 0..ps_temps {
        let _ = writeln!(src, "    var r{n} = vec4<f32>(0.0);");
    }
    src.push_str("    var o_c0 = vec4<f32>(0.0);\n");
    let emitter = Emitter {
        shader: ps,
        sm3,
        half: "ps",
        registers: ps_registers,
        defs: defs(ps),
    };
    for i in &ps.instructions {
        emitter.instruction(i, &mut src)?;
    }
    src.push_str("    return o_c0;\n}\n");

    Ok(Translation {
        source: src,
        vs_registers,
        ps_registers,
        vs_bools,
        ps_bools,
        texture_slots: samplers,
        vs_defs: defs(vs),
        ps_defs: defs(ps),
    })
}
