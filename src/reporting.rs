use std::collections::HashMap;
use std::io::stdout;
use std::io::Write;

use itertools::Itertools;

use crate::config::Config;
use crate::config::OutputFormat;
use crate::dependency_graph::ExportKind;
use crate::dependency_graph::ExportName;
use crate::dependency_graph::ModuleSourceAndLine;
use crate::dependency_graph::{Module, NormalizedModulePath};

fn report_clean(
    modules: HashMap<NormalizedModulePath, Module>,
    _config: &Config,
) -> anyhow::Result<()> {
    let mut sorted_modules = modules
        .into_iter()
        .filter(|(_, module)| !module.is_wildcard_imported())
        .collect::<Vec<_>>();

    sorted_modules.sort_by(|(a, _), (b, _)| a.cmp(&b));

    let mut found_any = false;

    for (i, (path, module)) in sorted_modules.into_iter().enumerate() {
        if i == 0 {
            found_any = true;
            println!("Unused exports: ");
        }

        for (i, (item, _)) in module
            .exports
            .into_iter()
            .filter(|(_, export)| !export.usage.get().is_used())
            .enumerate()
        {
            if i == 0 {
                println!("  {}:", path.display());
            }

            let name = item.get_name();
            println!("    - {}", name);
        }
    }

    if !found_any {
        println!("No unused exports!");
    }

    Ok(())
}

fn report_compact(
    modules: HashMap<NormalizedModulePath, Module>,
    config: &Config,
) -> anyhow::Result<()> {
    let mut unknown_exports = 0;

    let sorted_exports = modules
        .into_iter()
        .filter(|(_, module)| !module.is_wildcard_imported())
        .flat_map(|(_, module)| {
            module
                .exports
                .into_iter()
                .filter(|(_, export)| !export.usage.get().is_used())
                .filter(|(_, export)| {
                    if export.kind.matches_analyze_target(config.analyze_target) {
                        return true;
                    }

                    if export.kind == ExportKind::Unknown {
                        unknown_exports += 1;
                    }

                    false
                })
                .sorted_by_key(|(_, export)| export.location.line())
        })
        .map(|(name, export)| (name, export.location))
        .sorted_by(|(_, a_location), (_, b_location)| a_location.path().cmp(b_location.path()))
        .collect::<Vec<(ExportName, ModuleSourceAndLine)>>();

    if sorted_exports.is_empty() {
        println!("No unused exports!");
        return Ok(());
    }

    let mut stdout = stdout();

    for (name, location) in sorted_exports {
        writeln!(&mut stdout, "{} - {}", location, name)?;
    }

    if unknown_exports > 0 {
        let exports_label = if unknown_exports == 1 {
            "export"
        } else {
            "exports"
        };

        println!(
            "WARNING: Filtered out {} {} which can't be determined to match the current analysis target ({})",
            unknown_exports,
            exports_label,
            config.analyze_target.as_str()
        );
    }

    stdout.flush()?;

    Ok(())
}

pub fn report_unused_dependencies(
    modules: HashMap<NormalizedModulePath, Module>,
    config: &Config,
) -> anyhow::Result<()> {
    match config.format {
        OutputFormat::Clean => report_clean(modules, config),
        OutputFormat::Compact => report_compact(modules, config),
    }
}
