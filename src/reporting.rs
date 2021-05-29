use crate::dependency_graph::{Module, NormalizedModulePath};

pub fn report_unused_dependencies(
    modules: std::collections::HashMap<NormalizedModulePath, Module>,
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
