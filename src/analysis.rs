use std::borrow::Cow;

use crate::dependency_graph::{ExportName, Import, Module, NormalizedModulePath, Usage};

pub fn analyze_imports(modules: &std::collections::HashMap<NormalizedModulePath, Module>) {
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
