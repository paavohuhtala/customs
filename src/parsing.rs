use std::{
    collections::{HashMap, HashSet},
    ffi::OsStr,
    marker::PhantomData,
    ops::Deref,
    path::{Path, PathBuf},
    rc::Rc,
    sync::Arc,
};

use anyhow::{anyhow, Context};
use lazy_static::lazy_static;
use rayon::prelude::*;
use regex::Regex;

use swc_atoms::JsWord;
use swc_common::{FileName, FilePathMapping, SourceFile, SourceMap};
use swc_ecma_parser::StringInput;
use swc_ecma_visit::Visit;

use crate::{
    config::Config,
    dependency_graph::{
        normalize_module_path, resolve_import_source, Export, ExportKind, ExportName, Module,
        ModuleKind, ModulePath, ModuleSourceAndLine, NormalizedImportSource, NormalizedModulePath,
        Usage, Visibility,
    },
    module_visitor::{BindingId, ExportId, ModuleImport, ModuleVisitor, ScopeId, SelfExport},
};

fn normalize_package_import(import_source: &str) -> Option<String> {
    lazy_static! {
        // Parses the package name from an import source as capture group #1
        static ref PACKAGE_NAME_RE: Regex = Regex::new("((:?@[^/]+/[^/]+)|(:?[^@^/]*)).*").unwrap();
    }

    let captures = PACKAGE_NAME_RE.captures(import_source)?;
    Some(captures.get(1)?.as_str().to_string())
}

fn parse_imports(
    module: &mut Module,
    normalized_source: NormalizedImportSource,
    imports: Vec<ModuleImport>,
) -> anyhow::Result<()> {
    let normalized_module_path = match normalized_source {
        NormalizedImportSource::Global(name) => {
            let module_name =
                normalize_package_import(&name).context("Failed to normalize package import")?;
            module.imported_packages.insert(module_name);
            return Ok(());
        }
        NormalizedImportSource::Local(path) => path,
    };

    // TODO: handle CSS & other non-code imports

    let import_names = imports.into_iter().map(|import| import.imported_name);

    module
        .imports_mut(normalized_module_path)
        .extend(import_names);

    Ok(())
}

pub fn module_from_file(
    file_path: &Path,
    module_kind: ModuleKind,
) -> anyhow::Result<(SourceMap, swc_ecma_ast::Module)> {
    let source_map = SourceMap::new(FilePathMapping::empty());
    let source_file = source_map.load_file(file_path)?;
    let module = module_from_source_file(&source_file, module_kind)?;

    Ok((source_map, module))
}

pub fn module_from_source(
    source: String,
    module_kind: ModuleKind,
) -> anyhow::Result<(SourceMap, swc_ecma_ast::Module)> {
    let source_map = SourceMap::new(FilePathMapping::empty());
    let source_file = source_map.new_source_file(FileName::Anon, source);
    let module = module_from_source_file(&source_file, module_kind)?;

    Ok((source_map, module))
}

pub fn module_from_source_file(
    source_file: &Rc<SourceFile>,
    module_kind: ModuleKind,
) -> anyhow::Result<swc_ecma_ast::Module> {
    use swc_ecma_parser::{lexer::Lexer, Parser, Syntax, TsConfig};

    let input = StringInput::from(source_file.deref());

    let tsconfig = TsConfig {
        decorators: false,
        dynamic_import: false,
        import_assertions: false,
        no_early_errors: true,
        dts: module_kind == ModuleKind::DTS,
        tsx: module_kind == ModuleKind::TSX,
    };

    let lexer = Lexer::new(
        Syntax::Typescript(tsconfig),
        swc_ecma_ast::EsVersion::Es2020,
        input,
        None,
    );

    let mut parser = Parser::new_from(lexer);

    let module = parser
        .parse_module()
        .map_err(|err| anyhow!("Failed to parse module: {:?}", err))?;

    Ok(module)
}

fn read_and_parse_module(
    root: Arc<PathBuf>,
    file_path: &Path,
    module_kind: ModuleKind,
) -> anyhow::Result<Module> {
    let (source_map, module_ast) = module_from_file(file_path, module_kind)?;

    let normalized_path = normalize_module_path(&root, &file_path)?;

    let file_path = Arc::new(file_path.to_path_buf());

    let mut visitor = ModuleVisitor::new(file_path.clone(), source_map);
    visitor.visit_module(&module_ast, &module_ast);

    let module = Module::new(
        ModulePath {
            root,
            root_relative: file_path,
            normalized: normalized_path,
        },
        module_kind,
    );

    analyze_module(module, visitor)
}

#[derive(Debug)]
enum ValueBindingKind {}
#[derive(Debug)]
enum TypeBindingKind {}

#[derive(Debug)]
struct Binding<Kind> {
    kind: PhantomData<Kind>,
    name: JsWord,
    scope: ScopeId,
    reference_count: usize,
    source: ModuleSourceAndLine,
    export: SelfExport,
    export_ids: HashSet<ExportId>,
}

type ValueBinding = Binding<ValueBindingKind>;
type TypeBinding = Binding<TypeBindingKind>;

struct Bindings {
    values: Vec<ValueBinding>,
    types: Vec<TypeBinding>,
}

fn resolve_references(visitor: &ModuleVisitor, module_kind: ModuleKind) -> Bindings {
    let mut bindings: HashMap<BindingId, ValueBinding> = HashMap::new();
    let mut type_bindings: HashMap<BindingId, TypeBinding> = HashMap::new();

    for scope in &visitor.scopes {
        for binding in scope.bindings.values() {
            bindings.insert(
                binding.id,
                Binding {
                    kind: PhantomData,
                    name: binding.name.clone(),
                    scope: scope.id,
                    reference_count: 0,
                    export_ids: HashSet::new(),
                    export: binding.export,
                    source: binding.source.clone(),
                },
            );
        }

        for binding in scope.type_bindings.values() {
            type_bindings.insert(
                binding.id,
                Binding {
                    kind: PhantomData,
                    name: binding.name.clone(),
                    scope: scope.id,
                    reference_count: 0,
                    export_ids: HashSet::new(),
                    export: binding.export,
                    source: binding.source.clone(),
                },
            );
        }

        // Failing to resolve references is not an error, because we don't know all of the bindings (e.g globals, ambient declarations)

        'match_references: for (reference, _source) in scope.references.iter() {
            for scope in visitor.scope_and_parents_iter(scope) {
                if let Some(binding) = scope.bindings.get(reference) {
                    let binding = bindings.get_mut(&binding.id).unwrap();
                    binding.reference_count += 1;
                    continue 'match_references;
                }
            }
        }

        'match_type_references: for (reference, _source) in scope.type_references.iter() {
            for scope in visitor.scope_and_parents_iter(scope) {
                if let Some(binding) = scope.type_bindings.get(reference) {
                    let binding = type_bindings.get_mut(&binding.id).unwrap();
                    binding.reference_count += 1;
                    continue 'match_type_references;
                }
            }
        }

        'match_ambiguous_references: for (reference, _source) in scope.ambiguous_references.iter() {
            for scope in visitor.scope_and_parents_iter(scope) {
                if let Some(binding) = scope.bindings.get(reference) {
                    let binding = bindings.get_mut(&binding.id).unwrap();
                    binding.reference_count += 1;
                    continue 'match_ambiguous_references;
                }

                if let Some(binding) = scope.type_bindings.get(reference) {
                    let binding = type_bindings.get_mut(&binding.id).unwrap();
                    binding.reference_count += 1;
                    continue 'match_ambiguous_references;
                }
            }
        }
    }

    fn mark_as_exported<BK>(
        bindings: &mut HashMap<BindingId, Binding<BK>>,
        local_name: Option<&JsWord>,
        export_id: ExportId,
    ) {
        let local_name = if let Some(local_name) = local_name {
            local_name
        } else {
            return;
        };

        let exported_binding = bindings
            .values_mut()
            .filter(|binding| binding.scope.is_root())
            .find(|binding| &binding.name == local_name);

        if let Some(exported_binding) = exported_binding {
            exported_binding.export_ids.insert(export_id);
        }
    }

    // TODO: Add diagnostics if any of these fail
    for export in visitor.exports.iter() {
        match export.kind {
            ExportKind::Type => {
                mark_as_exported(&mut type_bindings, export.local_name.as_ref(), export.id);
            }
            ExportKind::Value => {
                mark_as_exported(&mut bindings, export.local_name.as_ref(), export.id);
            }
            ExportKind::Class | ExportKind::Enum | ExportKind::Unknown => {
                mark_as_exported(&mut bindings, export.local_name.as_ref(), export.id);
                mark_as_exported(&mut type_bindings, export.local_name.as_ref(), export.id);
            }
        }
    }

    // Only include bindings in root scope, because only they can be exported
    // If this is a declaration (DTS) file, consider all bindings in root scope to be implicitly exported
    // Otherwise, only consider bindings that are explicitly exported

    // TODO: DRY
    bindings.retain(|_, binding| {
        if !binding.scope.is_root() {
            return false;
        }

        if module_kind == ModuleKind::DTS {
            return true;
        }

        !binding.export_ids.is_empty()
    });

    type_bindings.retain(|_, binding| {
        if !binding.scope.is_root() {
            return false;
        }

        if module_kind == ModuleKind::DTS {
            return true;
        }

        !binding.export_ids.is_empty()
    });

    let values = bindings.into_values().collect();
    let types = type_bindings.into_values().collect();

    Bindings { values, types }
}

pub fn analyze_module(mut module: Module, visitor: ModuleVisitor) -> anyhow::Result<Module> {
    let Bindings { values, types } = resolve_references(&visitor, module.kind);

    println!("{:#?}", values);
    println!("{:#?}", types);

    let ModuleVisitor {
        exports, imports, ..
    } = visitor;

    for export in exports {
        if export.local_name.is_none() {
            module.add_export(
                export.name,
                Export::new(export.kind, Visibility::Exported, export.source),
            );
        } else {
            let export = Export::new(export.kind, Visibility::Exported, export.source);
        }
    }

    /*
    let named_exports = visitor
        .exports
        .iter()
        .filter_map(|export| export.local_name.as_ref())
        .map(|name| name.to_owned());

    let (non_shadowed_exports, shadowed_exports): (Vec<_>, Vec<_>) =
        named_exports.partition(|id| *binding_counts.get(id).unwrap_or(&1) == 1);

    let locally_used_exports_iter = non_shadowed_exports
        .into_iter()
        .filter(|export| *reference_counts.get(export).unwrap_or(&0) > 0);

    let locally_used_shadowed_exports_iter = shadowed_exports
        .into_iter()
        .filter(|export| !is_shadowed_export_used(&visitor, &export));

    let locally_used_exports = locally_used_exports_iter
        .chain(locally_used_shadowed_exports_iter)
        .collect::<HashSet<_>>();

    let ModuleVisitor {
        exports,
        mut scopes,
        imports,
        ..
    } = visitor;

    for export in exports {
        let export_entry = Export::new(export.kind, Visibility::Exported, export.source);

        if let Some(local_name) = export.local_name {
            if locally_used_exports.contains(&local_name) {
                export_entry.usage.set(Usage {
                    used_locally: true,
                    used_externally: false,
                });
            }
        }

        module.add_export(export.name, export_entry)
    }

    // In declaration modules all types defined in the root scope are implicitly exported
    if module.kind.is_declaration() {
        let root_scope = scopes.remove(0);

        for (type_binding_name, type_binding) in root_scope.type_bindings {
            let export_name = ExportName::Named(type_binding_name);
            module.add_export(
                export_name,
                Export::new(
                    crate::dependency_graph::ExportKind::Type,
                    Visibility::ImplicitlyExported,
                    type_binding.source,
                ),
            );
        }
    }*/

    let current_folder = module
        .path
        .root_relative
        .parent()
        .expect("A file path should always have a parent")
        .to_owned();

    for (unnormalized_module, imports) in imports {
        let source =
            resolve_import_source(&module.path.root, &current_folder, &unnormalized_module)?;
        parse_imports(&mut module, source, imports)?;
    }

    Ok(module)
}

pub fn parse_all_modules(config: &Config) -> HashMap<NormalizedModulePath, Module> {
    // This is kind of nasty: filter_entry wants a static closure, and this is the easiest way to to do that.
    // We leak a bit of memory (up to a few hundred bytes), but as long as this function is only ran once per process it's not an issue.
    // If we _really_ wanted to clean this up we could use a bit of unsafe to "unleak" the vector, based on the assumption
    // that walker does not hold onto any references after iteration is finished.
    // Alternatively we could filter after directory walking, but doing it earlier should more efficient.
    let ignored_folders = config.ignored_folders.clone();
    let leaked_ignored_folders = &*ignored_folders.leak::<'static>();

    let root = config.root.as_ref();

    let walker = ignore::WalkBuilder::new(root)
        .standard_filters(true)
        .add_custom_ignore_filename(".customsignore")
        .filter_entry(move |entry| {
            !leaked_ignored_folders
                .iter()
                .any(|root| entry.path().starts_with(root))
        })
        .build();

    walker
        .into_iter()
        .par_bridge()
        // TODO: don't silently ignore read errors?
        .filter_map(|entry| {
            entry.ok().filter(|entry| {
                entry
                    .file_type()
                    .expect("This should never be stdin.")
                    .is_file()
            })
        })
        .filter_map(|entry| {
            let file_path = entry.path();
            let file_name = file_path
                .file_name()
                .expect("Surely every file must have a name?");

            let module_kind = get_module_kind(file_name)?;

            match read_and_parse_module(config.root.clone(), &file_path, module_kind) {
                Ok(module) => Some((module.path.normalized.clone(), module)),
                Err(err) => {
                    eprintln!("Error while parsing {}: {}", file_path.display(), err);
                    None
                }
            }
        })
        .collect()
}

fn get_module_kind(file_name: &OsStr) -> Option<ModuleKind> {
    // OsStr doesn't support ends_with and extension() doesn't work with .d.ts files, so we have to do a hack like this:
    let file_name = file_name.to_string_lossy();

    if file_name.ends_with(".d.ts") {
        Some(ModuleKind::DTS)
    } else if file_name.ends_with(".ts") {
        Some(ModuleKind::TS)
    } else if file_name.ends_with(".tsx") {
        Some(ModuleKind::TSX)
    } else {
        None
    }
}
