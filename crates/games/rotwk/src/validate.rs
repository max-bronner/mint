use std::collections::BTreeMap;
use std::time::Instant;

use mint_sage_ini::diagnostic::ValueErrors;
use mint_sim::module::{Capability, ModuleRegistry};
use mint_sim::{Frame, World};

use crate::game_data::GameData;

pub(crate) fn declared_modules(data: &GameData) -> Result<String, Vec<String>> {
    declared_modules_of(data, data.templates.iter())
}

pub(crate) fn declared_modules_of<'r>(
    data: &GameData,
    records: impl Iterator<Item = &'r mint_sage_modules::ObjectRecord>,
) -> Result<String, Vec<String>> {
    let started = Instant::now();
    let mut world = World::new();
    let mut registry = ModuleRegistry::new(&mut world);
    mint_sage_modules::register_live_families(&mut registry, &mut world);

    let values = ValueErrors::new();
    let mut refused: Vec<String> = Vec::new();
    let mut declarations: usize = 0;
    let mut bodies: BTreeMap<&str, usize> = BTreeMap::new();
    for record in records {
        for module in &record.declared_modules {
            declarations = declarations.saturating_add(1);
            let context = match &module.tag {
                Some(tag) => format!("{} / {} {tag}", record.name, module.name),
                None => format!("{} / {}", record.name, module.name),
            };
            let fields = module.recording_source(&data.upgrade_bits, &values, &context);
            if let Some(error) = registry.check(&module.name, &fields, Frame::ZERO) {
                refused.push(format!("{context}: {error}"));
            } else if registry
                .capabilities_of(&module.name)
                .is_some_and(|mask| mask.contains(Capability::Body))
            {
                let count = bodies.entry(module.name.as_str()).or_default();
                *count = count.saturating_add(1);
            }
        }
    }
    refused.extend(values.take().iter().map(ToString::to_string));

    if refused.is_empty() {
        let bodies: Vec<String> = bodies
            .iter()
            .map(|(name, count)| format!("{count} {name}"))
            .collect();
        Ok(format!(
            "modules: {declarations} declaration(s) validated in {:.0} ms (bodies: {})",
            started.elapsed().as_secs_f64() * 1000.0,
            bodies.join(", ")
        ))
    } else {
        Err(refused)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    #[ignore = "needs a retail installation; set MINT_CORPUS"]
    fn every_declared_module_builds() {
        let Some(root) = std::env::var_os("MINT_CORPUS").map(std::path::PathBuf::from) else {
            println!("SKIPPED: MINT_CORPUS is not set, so there is nothing to validate against");
            return;
        };
        let mut vfs = mint_vfs::Vfs::new();
        let mount = mint_vfs::DirMount::with_prefix(root.join("data"), "data")
            .unwrap_or_else(|error| panic!("mounting {}: {error}", root.display()));
        vfs.mount(std::sync::Arc::new(mount));
        match crate::game_data::load_game_data(&vfs) {
            Ok((_, line)) => println!("{line}"),
            Err(refusal) => {
                refusal.report();
                panic!("load refused - see the refusal(s) printed above");
            }
        }
    }
}
