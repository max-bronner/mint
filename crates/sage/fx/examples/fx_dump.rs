use mint_sage_fx::effect::Resource;
use mint_sage_fx::preshader::op_name;
use mint_sage_fx::shader::opcode_name;
use mint_sage_fx::{Effect, Shader};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let install = args.next().ok_or("install path")?;
    let name = args.next().ok_or("effect name")?.to_ascii_lowercase();
    let only = args.next();
    let archive = mint_sage_big::BigArchive::open(format!("{install}/Shaders.big"))?;
    let entry = archive
        .entries()
        .iter()
        .find(|e| {
            e.name
                .to_ascii_lowercase()
                .replace('\\', "/")
                .ends_with(&name)
        })
        .ok_or("no such effect")?;
    let bytes = archive.read_entry(entry)?;
    let effect = Effect::parse(&bytes)?;
    for (i, p) in effect.parameters.iter().enumerate() {
        let floats: Vec<f32> = p.words.iter().map(|w| f32::from_bits(*w)).collect();
        println!(
            "param {i} {} sem={:?} kind={} class={:?} {}x{} el={} members={} value={:?} samplers={}",
            p.ty.name,
            p.ty.semantic,
            p.ty.kind,
            p.ty.class,
            p.ty.rows,
            p.ty.columns,
            p.ty.elements,
            p.ty.members.len(),
            if p.ty.kind == 3 { format!("{floats:?}") } else { format!("{:?}", p.words) },
            p.samplers.len()
        );
        for m in &p.ty.members {
            println!(
                "    member {} {:?} {}x{} el={}",
                m.name, m.class, m.rows, m.columns, m.elements
            );
        }
        for s in p.samplers.iter().flatten() {
            println!(
                "    sampler state op={:#x} value={:?} res={:?}",
                s.operation, s.value.words, s.resource
            );
        }
    }
    for t in &effect.techniques {
        if only
            .as_deref()
            .is_some_and(|o| !o.eq_ignore_ascii_case(&t.name))
        {
            continue;
        }
        println!("technique {}", t.name);
        for p in &t.passes {
            println!("  pass {}", p.name);
            for s in &p.states {
                let shown = match &s.resource {
                    Some(Resource::Data(d)) => format!("data {} bytes", d.len()),
                    other => format!("{other:?}"),
                };
                println!(
                    "    state op={:#x} idx={} value={:?} {shown}",
                    s.operation, s.index, s.value.words
                );
                if s.operation == 0x92 || s.operation == 0x93 {
                    if let Some(blob) = effect.state_shader(s, 0) {
                        dump_shader(blob);
                    }
                }
            }
        }
    }
    Ok(())
}

fn dump_shader(blob: &[u8]) {
    let shader = match Shader::parse(blob) {
        Ok(s) => s,
        Err(e) => {
            println!("      PARSE FAILED: {e}");
            return;
        }
    };
    println!("      {:?} {}.{}", shader.stage, shader.major, shader.minor);
    for c in &shader.constants {
        println!(
            "      ctab {} set={} reg={}+{} class={} {}x{} el={} members={:?}",
            c.name,
            c.register_set,
            c.register_index,
            c.register_count,
            c.class,
            c.rows,
            c.columns,
            c.elements,
            c.members
        );
    }
    for i in &shader.instructions {
        let lit: Vec<f32> = i.literal.iter().map(|w| f32::from_bits(*w)).collect();
        let mut line = format!("      {}", opcode_name(i.opcode));
        if i.controls != 0 {
            line += &format!("<{}>", i.controls);
        }
        if let Some(d) = i.decl {
            line += &format!(
                " [usage {} idx {} tex {}]",
                d.usage, d.usage_index, d.texture_type
            );
        }
        if let Some(d) = i.dst {
            let mask: String = "xyzw"
                .chars()
                .enumerate()
                .filter(|(n, _)| d.mask & (1 << n) != 0)
                .map(|(_, c)| c)
                .collect();
            line += &format!(
                " {}{}.{}{}",
                reg_name(d.reg.kind),
                d.reg.index,
                mask,
                if d.modifiers & 1 != 0 { "_sat" } else { "" }
            );
        }
        for s in &i.src {
            let sw: String = s
                .swizzle
                .iter()
                .map(|c| *['x', 'y', 'z', 'w'].get(*c as usize).unwrap_or(&'?'))
                .collect();
            let rel = s
                .relative
                .map(|(r, c)| {
                    format!(
                        "[{}{}.{}]",
                        reg_name(r.kind),
                        r.index,
                        ['x', 'y', 'z', 'w'].get(c as usize).unwrap_or(&'?')
                    )
                })
                .unwrap_or_default();
            line += &format!(
                ", {}{}{}{}.{}",
                if s.modifier != 0 {
                    format!("m{}:", s.modifier)
                } else {
                    String::new()
                },
                reg_name(s.reg.kind),
                s.reg.index,
                rel,
                sw
            );
        }
        if !lit.is_empty() {
            line += &format!(" {lit:?}");
        }
        println!("{line}");
    }
    if let Some(pre) = &shader.preshader {
        println!("      preshader literals={:?}", pre.literals);
        for c in &pre.constants {
            println!(
                "      pre-ctab {} reg={}+{} class={} {}x{}",
                c.name, c.register_index, c.register_count, c.class, c.rows, c.columns
            );
        }
        for op in &pre.ops {
            println!(
                "      pre {} n={} scalar={} in={:?} out={:?}",
                op_name(op.opcode),
                op.components,
                op.scalar,
                op.inputs
                    .iter()
                    .map(|o| (o.table, o.offset, o.index))
                    .collect::<Vec<_>>(),
                (op.output.table, op.output.offset)
            );
        }
    }
}

fn reg_name(kind: u32) -> &'static str {
    match kind {
        0 => "r",
        1 => "v",
        2 => "c",
        3 => "a/t",
        4 => "oRast",
        5 => "oD",
        6 => "oT",
        7 => "i",
        8 => "oC",
        9 => "oDepth",
        10 => "s",
        14 => "b",
        _ => "?",
    }
}
