use std::str::FromStr;

use anyhow::anyhow;

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

pub fn report_unused_dependencies(
    modules: std::collections::HashMap<NormalizedModulePath, Module>,
    _format: OutputFormat,
) {
    let mut found_any = false;

    let mut sorted_modules = modules
        .into_iter()
        .filter(|(_, module)| !module.is_wildcard_imported())
        .collect::<Vec<_>>();

    sorted_modules.sort_by(|(a, _), (b, _)| a.cmp(&b));

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
                println!("  {}:", path.to_string_lossy());
            }

            let name = item.get_name();
            println!("    - {}", name);
        }
    }

    if !found_any {
        println!("No unused exports!");
    }
}
