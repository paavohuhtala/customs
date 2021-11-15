use std::collections::{HashMap, HashSet};

use crate::{
    config::Config,
    dependency_graph::{
        ExportName, ImportName, Module, ModuleSourceAndLine, NormalizedModulePath, Usage,
    },
    package_json::PackageJson,
};

pub fn resolve_module_imports(modules: &HashMap<NormalizedModulePath, Module>) {
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
                                // println!("Marking {}##{} as used", import_path.display(), key);

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
    modules: HashMap<NormalizedModulePath, Module>,
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
    modules: &HashMap<NormalizedModulePath, Module>,
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

#[cfg(test)]
mod tests {
    use std::{path::PathBuf, sync::Arc};

    use crate::dependency_graph::{
        Export, ExportKind, ModuleKind, ModulePath, Visibility::Exported,
    };

    use super::*;

    #[test]
    fn imports_smoke() {
        let root_path: Arc<PathBuf> = Arc::new("".into());

        let mut modules = HashMap::new();

        let module_a_path = NormalizedModulePath::new("a");

        let mut module_a = Module::new(
            ModulePath {
                root: root_path.clone(),
                root_relative: Arc::new("a".into()),
                normalized: module_a_path.clone(),
            },
            ModuleKind::TS,
        );
        let export_foo = Export::new(ExportKind::Value, Exported, ModuleSourceAndLine::new_mock());
        module_a.add_export(ExportName::named("foo"), export_foo);
        let export_bar = Export::new(ExportKind::Value, Exported, ModuleSourceAndLine::new_mock());
        module_a.add_export(ExportName::named("bar"), export_bar);

        modules.insert(module_a_path.clone(), module_a);

        let module_b_path = NormalizedModulePath::new("b");
        let mut module_b = Module::new(
            ModulePath {
                root: root_path.clone(),
                root_relative: Arc::new("b".into()),
                normalized: module_b_path.clone(),
            },
            ModuleKind::TS,
        );
        module_b
            .imports_mut(module_a_path.clone())
            .push(ImportName::named("foo"));

        modules.insert(module_b_path.clone(), module_b);

        resolve_module_imports(&modules);

        let module_a_exports = &modules.get(&module_a_path).unwrap().exports;
        let export_foo = module_a_exports.get(&ExportName::named("foo")).unwrap();
        assert!(export_foo.is_used(), "foo should be marked as used");
        let export_foo = module_a_exports.get(&ExportName::named("bar")).unwrap();
        assert!(!export_foo.is_used(), "bar should not be marked as used");
    }
}
