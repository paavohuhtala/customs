use std::collections::HashSet;

use crate::{
    config::Config,
    dependency_graph::{
        ExportName, ImportName, Module, ModuleSourceAndLine, NormalizedModulePath, Usage,
    },
    package_json::PackageJson,
};

pub fn resolve_module_imports(modules: &std::collections::HashMap<NormalizedModulePath, Module>) {
    for (path, module) in modules.iter() {
        for (import_path, imports) in &module.imported_modules {
            match modules.get(import_path) {
                None => {
                    println!(
                        "WARNING: Failed to resolve module {} (in {})",
                        import_path.display(),
                        path.display()
                    );
                }
                Some(source_module) => {
                    if source_module.is_wildcard_imported() {
                        // Module is already fully imported, bail.
                        continue;
                    }

                    for import in imports {
                        let key = match import {
                            ImportName::Named(name) => ExportName::Named(name.clone()),
                            ImportName::Default => ExportName::Default,
                            ImportName::Wildcard => {
                                source_module.mark_wildcard_imported();
                                break;
                            }
                        };

                        match source_module.exports.get(&key) {
                            None => {
                                println!(
                                    "Failed to resolve export {} in module {} (imported from {})",
                                    key,
                                    import_path.display(),
                                    path.display(),
                                );
                            }
                            Some(export) => {
                                // TODO put behind debug logging

                                /* println!(
                                    "Marking {}##{} as used",
                                    import_path.0.to_string_lossy(),
                                    key.get_name()
                                );*/

                                export.usage.set(Usage {
                                    used_externally: true,
                                    ..export.usage.get()
                                })
                            }
                        }
                    }
                }
            }
        }
    }
}

pub struct UnusedExportsResults {
    pub sorted_exports: Vec<(ExportName, ModuleSourceAndLine, Usage)>,
}

pub fn find_unused_exports(
    modules: std::collections::HashMap<NormalizedModulePath, Module>,
    config: &Config,
) -> UnusedExportsResults {
    let mut sorted_exports = modules
        .into_iter()
        .filter(|(_, module)| !module.is_wildcard_imported())
        .flat_map(|(_, module)| {
            module
                .exports
                .into_iter()
                .filter(|(_, export)| !export.usage.get().used_externally)
                .filter(|(_, export)| export.kind.matches_analyze_target(config.analyze_target))
        })
        .map(|(name, export)| (name, export.location, export.usage.take()))
        .collect::<Vec<(ExportName, ModuleSourceAndLine, Usage)>>();

    sorted_exports.sort_unstable_by(|(_, a_location, _), (_, b_location, _)| {
        a_location
            .path()
            .cmp(b_location.path())
            .then_with(|| a_location.line().cmp(&b_location.line()))
    });

    UnusedExportsResults { sorted_exports }
}

pub fn find_unused_dependencies(
    modules: &std::collections::HashMap<NormalizedModulePath, Module>,
    package_json: &PackageJson,
    _config: &Config,
) -> Vec<String> {
    let imported_packages = modules
        .values()
        .flat_map(|module| module.imported_packages.iter().map(String::as_str))
        .collect::<HashSet<&str>>();

    let installed_dependencies = package_json
        .dependencies
        .keys()
        .map(String::as_str)
        .collect::<HashSet<&str>>();

    installed_dependencies
        .difference(&imported_packages)
        .map(|item| (*item).to_string())
        .collect()
}
