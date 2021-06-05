use std::{borrow::Cow, collections::HashSet};

use itertools::Itertools;

use crate::{
    config::Config,
    dependency_graph::{
        ExportName, Import, Module, ModuleSourceAndLine, NormalizedModulePath, OwnedExportName,
        Usage,
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
                            Import::Named(name) => ExportName::Named(Cow::Borrowed(name)),
                            Import::Default => ExportName::Default,
                            Import::Wildcard => {
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
    pub sorted_exports: Vec<(OwnedExportName, ModuleSourceAndLine)>,
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
                .filter(|(_, export)| !export.usage.get().is_used())
                .filter(|(_, export)| export.kind.matches_analyze_target(config.analyze_target))
                .sorted_unstable_by_key(|(_, export)| export.location.line())
        })
        .map(|(name, export)| (name.to_owned(), export.location))
        .collect::<Vec<(OwnedExportName, ModuleSourceAndLine)>>();

    sorted_exports.sort_unstable_by(|(_, a_location), (_, b_location)| {
        a_location.path().cmp(b_location.path())
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
        .flat_map(|module| module.imported_packages.iter().map(|s| s.as_str()))
        .collect::<HashSet<&str>>();

    let installed_dependencies = package_json
        .dependencies
        .keys()
        .map(|s| s.as_str())
        .collect::<HashSet<&str>>();

    installed_dependencies
        .difference(&imported_packages)
        .map(|item| item.to_string())
        .collect()
}
