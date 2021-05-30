use std::collections::HashMap;
use std::str::FromStr;

use anyhow::anyhow;

use crate::dependency_graph::ExportName;
use crate::dependency_graph::ModuleSourceAndLine;
use crate::dependency_graph::{Module, NormalizedModulePath};

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum OutputFormat {
    Clean,
    Compact,
}

impl FromStr for OutputFormat {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "clean" => Ok(Self::Clean),
            "compact" => Ok(Self::Compact),
            _ => Err(anyhow!("Unknown output format: {}", s)),
        }
    }
}

impl OutputFormat {
    pub const ALL_FORMATS: &'static [&'static str] = &["clean", "compact"];
}

fn report_clean(modules: HashMap<NormalizedModulePath, Module>) {
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
}

fn report_compact(modules: HashMap<NormalizedModulePath, Module>) {
    let mut sorted_exports = modules
        .into_iter()
        .filter(|(_, module)| !module.is_wildcard_imported())
        .flat_map(|(_, module)| {
            module
                .exports
                .into_iter()
                .filter(|(_, export)| !export.usage.get().is_used())
        })
        .map(|(name, export)| (name, export.location))
        .collect::<Vec<(ExportName, ModuleSourceAndLine)>>();

    sorted_exports.sort_by(|(_, a_location), (_, b_location)| {
        a_location
            .path()
            .cmp(b_location.path())
            .then_with(|| a_location.line().cmp(&b_location.line()))
    });

    if sorted_exports.len() == 0 {
        println!("No unused exports!");
        return;
    }

    for (name, location) in sorted_exports {
        println!("{} - {}", location, name.get_name());
    }
}

pub fn report_unused_dependencies(
    modules: HashMap<NormalizedModulePath, Module>,
    format: OutputFormat,
) {
    match format {
        OutputFormat::Clean => report_clean(modules),
        OutputFormat::Compact => report_compact(modules),
    }
}
