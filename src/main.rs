use std::{
    borrow::Cow,
    cell::Cell,
    collections::HashMap,
    convert::TryFrom,
    ops::Deref,
    path::{Path, PathBuf},
    time::Instant,
};

use anyhow::{anyhow, Context};
use clap::{AppSettings, Clap};
use rayon::prelude::*;
use relative_path::RelativePath;
use swc_common::BytePos;
use swc_ecma_ast::{Decl, DefaultDecl, ImportSpecifier, ModuleDecl, ModuleItem, Stmt};
use swc_ecma_parser::{lexer::Lexer, Parser, StringInput, Syntax, TsConfig};
use walkdir;

#[derive(PartialEq, Eq, Hash, Debug, Clone)]
struct NormalizedModulePath(PathBuf);

#[derive(PartialEq, Eq, Hash, Debug, Clone)]
enum ExportName<'a> {
    Named(Cow<'a, str>),
    Default,
}

impl ExportName<'_> {
    pub fn get_name(&self) -> &Cow<'_, str> {
        match self {
            ExportName::Named(name) => name,
            ExportName::Default => &Cow::Borrowed("Default"),
        }
    }
}

#[derive(Debug, Clone)]
struct Export {
    usage: Cell<Usage>,
    kind: ExportKind,
    visibility: Visibility,
}

impl Export {
    pub fn new(kind: ExportKind, visibility: Visibility) -> Self {
        Export {
            usage: Default::default(),
            kind,
            visibility,
        }
    }
}

#[derive(PartialEq, Eq, Hash, Debug, Default, Copy, Clone)]
struct Usage {
    used_locally: bool,
    used_externally: bool,
}

impl Usage {
    pub fn is_used(&self) -> bool {
        self.used_locally || self.used_externally
    }
}

#[derive(PartialEq, Eq, Hash, Debug, Clone)]
enum Import {
    Named(String),
    Default,
    Wildcard,
}

#[derive(Debug, Clone)]
struct SourceModule<'a> {
    path: NormalizedModulePath,
    exports: HashMap<ExportName<'a>, Export>,
    imported_modules: HashMap<NormalizedModulePath, Vec<Import>>,
    is_wildcard_imported: Cell<bool>,
    is_dts: bool,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum ModuleKind {
    TS,
    TSX,
    DTS,
}

impl ModuleKind {
    pub fn is_declaration(self) -> bool {
        self == ModuleKind::DTS
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum ExportKind {
    Type,
    Value,
    Unknown,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum Visibility {
    Exported,
    ImplicitlyExported,
}

fn normalize_module_path(
    project_root: &Path,
    module_path: &Path,
) -> anyhow::Result<NormalizedModulePath> {
    let normalized_path = module_path.strip_prefix(project_root).with_context(|| {
        format!(
            "Failed to convert {:?} to be relative of {:?}",
            module_path, project_root
        )
    })?;

    let folder = normalized_path
        .parent()
        .expect("The file must be in a folder.");

    let file_name = normalized_path
        .file_name()
        .expect("The path must point to a file")
        .to_string_lossy();
    let file_name_without_extension = file_name.split_terminator('.').next().unwrap();

    let normalized_path = folder.join(file_name_without_extension);

    Ok(NormalizedModulePath(normalized_path))
}

fn resolve_import_path(
    project_root: &Path,
    current_folder: &Path,
    import_source: &str,
) -> anyhow::Result<NormalizedModulePath> {
    let mut absolute_path = RelativePath::new(import_source).to_logical_path(current_folder);

    for ext in ["d.ts", "ts", "tsx"] {
        let with_ext = absolute_path.clone().with_extension(ext);
        if with_ext.is_file() {
            return normalize_module_path(project_root, &with_ext);
        }
    }

    absolute_path.push("index.ts");
    normalize_module_path(project_root, &absolute_path)
}

fn get_decl_idents(
    decl: &Decl,
    results: &mut Vec<(String, Export)>,
    is_export_declaration: bool,
    module_kind: ModuleKind,
) {
    let should_export_value = is_export_declaration;
    let should_export_type = is_export_declaration || module_kind.is_declaration();
    let type_visibility = if module_kind.is_declaration() {
        Visibility::ImplicitlyExported
    } else {
        Visibility::Exported
    };

    match decl {
        // TODO is this correct with classes exported from d.ts files?
        Decl::Class(class) if should_export_value => {
            results.push((
                class.ident.sym.to_string(),
                Export::new(ExportKind::Value, Visibility::Exported),
            ));
        }
        Decl::Fn(f) if should_export_value => {
            results.push((
                f.ident.sym.to_string(),
                Export::new(ExportKind::Value, Visibility::Exported),
            ));
        }
        Decl::Var(var) if should_export_value => {
            for declarator in &var.decls {
                match &declarator.name {
                    swc_ecma_ast::Pat::Ident(id) => {
                        results.push((
                            id.id.sym.to_string(),
                            Export::new(ExportKind::Value, Visibility::Exported),
                        ));
                    }
                    swc_ecma_ast::Pat::Array(_) => {
                        todo!()
                    }
                    swc_ecma_ast::Pat::Rest(_) => {
                        todo!()
                    }
                    swc_ecma_ast::Pat::Object(pat) => {
                        results.extend(pat.props.iter().filter_map(|prop| match prop {
                            swc_ecma_ast::ObjectPatProp::KeyValue(_) => None,
                            swc_ecma_ast::ObjectPatProp::Assign(assign) => Some((
                                assign.key.sym.to_string(),
                                Export::new(ExportKind::Value, Visibility::Exported),
                            )),
                            swc_ecma_ast::ObjectPatProp::Rest(_) => None,
                        }));
                    }
                    swc_ecma_ast::Pat::Assign(_) => {
                        todo!()
                    }
                    swc_ecma_ast::Pat::Invalid(_) => {
                        todo!()
                    }
                    swc_ecma_ast::Pat::Expr(_) => {
                        todo!()
                    }
                }
            }
        }
        Decl::TsInterface(interface_decl) if should_export_type => {
            results.push((
                interface_decl.id.sym.to_string(),
                Export::new(ExportKind::Type, type_visibility),
            ));
        }
        Decl::TsTypeAlias(type_alias_decl) if should_export_type => {
            results.push((
                type_alias_decl.id.sym.to_string(),
                Export::new(ExportKind::Type, type_visibility),
            ));
        }
        Decl::TsEnum(enum_decl) if should_export_type => {
            results.push((
                enum_decl.id.sym.to_string(),
                Export::new(ExportKind::Type, type_visibility),
            ));
        }
        Decl::TsModule(_) => {
            // TODO: Do we need to handle typings files?
        }
        _ => {}
    }
}

#[derive(Clap)]
#[clap(version = "0.1", author = "Paavo Huhtala <paavo.huhtala@gmail.com>")]
#[clap(setting = AppSettings::ColoredHelp)]
struct Opts {
    target_dir: String,
}

fn main() -> anyhow::Result<()> {
    let opts = Opts::parse();
    let root = PathBuf::try_from(opts.target_dir)?;

    let start_time = Instant::now();

    let modules = walkdir::WalkDir::new(&root)
        .same_file_system(true)
        .into_iter()
        .par_bridge()
        // TODO: don't silently ignore read errors?
        .filter_map(|entry| entry.ok().filter(|entry| entry.file_type().is_file()))
        .filter_map(|entry| {
            let file_path = entry.path();
            let file_name = file_path
                .file_name()
                .expect("Surely every file must have a name?");

            // OsStr doesn't support ends_with and extension doesn't work with .d.ts files, so we have to do a hack like this:
            let file_name = file_name.to_string_lossy();

            let module_kind = if file_name.ends_with(".d.ts") {
                ModuleKind::DTS
            } else if file_name.ends_with(".ts") {
                ModuleKind::TS
            } else if file_name.ends_with(".tsx") {
                ModuleKind::TSX
            } else {
                return None;
            };

            match parse_module(&root, file_path, module_kind) {
                Ok(module) => Some((module.path.clone(), module)),
                Err(err) => {
                    eprintln!(
                        "Error while parsing {}: {}",
                        file_path.to_string_lossy(),
                        err
                    );
                    None
                }
            }
        })
        .collect::<HashMap<_, _>>();

    let finished_parse_time = Instant::now();
    let parse_duration = finished_parse_time - start_time;
    println!(
        "Parsed {} modules in {} ms",
        modules.len(),
        parse_duration.as_millis()
    );

    let mut total_imports = 0;

    for (path, module) in modules.iter() {
        for (import_path, imports) in &module.imported_modules {
            match modules.get(import_path) {
                None => {
                    println!(
                        "WARNING: Failed to resolve module {} (in {})",
                        import_path.0.to_string_lossy(),
                        path.0.to_string_lossy()
                    );
                }
                Some(source_module) => {
                    if source_module.is_wildcard_imported.get() {
                        // Module is already fully imported, bail.
                        continue;
                    }

                    for import in imports {
                        total_imports += 1;
                        let key = match import {
                            Import::Named(name) => ExportName::Named(Cow::Borrowed(name)),
                            Import::Default => ExportName::Default,
                            Import::Wildcard => {
                                source_module.is_wildcard_imported.set(true);
                                break;
                            }
                        };

                        match source_module.exports.get(&key) {
                            None => {
                                println!(
                                    "Failed to resolve import {:?} in module {} (imported from {})",
                                    key,
                                    import_path.0.to_string_lossy(),
                                    path.0.to_string_lossy(),
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

    let resolution_duration = finished_parse_time.elapsed();

    println!(
        "Resolved {} imports in {} ms",
        total_imports,
        resolution_duration.as_millis()
    );

    let mut found_any = false;

    let mut sorted_modules = modules
        .into_iter()
        .filter(|(_, module)| !module.is_wildcard_imported.get())
        .collect::<Vec<_>>();

    sorted_modules.sort_by(|(a, _), (b, _)| a.0.cmp(&b.0));

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
                println!("  {}:", path.0.to_string_lossy());
            }

            let name = item.get_name();
            println!("    - {}", name);
        }
    }

    if !found_any {
        println!("No unused exports!");
    }

    println!("Finished in {}ms", start_time.elapsed().as_millis());

    Ok(())
}

fn parse_module(
    root: &Path,
    file_path: &Path,
    module_kind: ModuleKind,
) -> anyhow::Result<SourceModule<'static>> {
    let tsconfig = TsConfig {
        decorators: false,
        dynamic_import: false,
        import_assertions: false,
        no_early_errors: true,
        dts: module_kind == ModuleKind::DTS,
        tsx: module_kind == ModuleKind::TSX,
    };

    let normalized_path = normalize_module_path(&root, &file_path)?;
    let current_folder = file_path
        .parent()
        .expect("A file path should always have a parent");

    let content = std::fs::read_to_string(&file_path)?;
    let input = StringInput::new(&content, BytePos(0), BytePos(content.len() as u32));
    let lexer = Lexer::new(
        Syntax::Typescript(tsconfig),
        swc_ecma_parser::JscTarget::Es2020,
        input,
        None,
    );
    let mut parser = Parser::new_from(lexer);

    // TODO put behind debug logging
    // println!("Parsing {}", file_path.to_string_lossy());

    let module = parser
        .parse_module()
        .map_err(|err| anyhow!("Failed to parse module: {:?}", err))?;

    let mut named_exports = Vec::new();
    let mut default_export: Option<ExportKind> = None;
    let mut imported_modules = HashMap::new();

    for item in module.body {
        match item {
            ModuleItem::ModuleDecl(ModuleDecl::ExportDecl(export_decl)) => {
                get_decl_idents(&export_decl.decl, &mut named_exports, true, module_kind);
            }
            ModuleItem::ModuleDecl(ModuleDecl::ExportNamed(named_export)) => {
                for specifier in &named_export.specifiers {
                    match specifier {
                        swc_ecma_ast::ExportSpecifier::Namespace(_) => {
                            todo!("Namespace re-exports not supported yet.");
                        }
                        swc_ecma_ast::ExportSpecifier::Default(default) => {
                            let name = default.exported.sym.to_string();
                            named_exports.push((
                                name,
                                Export::new(ExportKind::Unknown, Visibility::Exported),
                            ));
                        }
                        swc_ecma_ast::ExportSpecifier::Named(named) => {
                            let name = named.exported.as_ref().unwrap_or(&named.orig);
                            named_exports.push((
                                name.sym.to_string(),
                                Export::new(ExportKind::Unknown, Visibility::Exported),
                            ));
                        }
                    }
                }
            }
            ModuleItem::ModuleDecl(ModuleDecl::ExportDefaultExpr(_)) => {
                default_export = Some(ExportKind::Value);
            }
            ModuleItem::ModuleDecl(ModuleDecl::ExportDefaultDecl(decl)) => {
                let export_kind = match decl.decl {
                    DefaultDecl::Class(_) | DefaultDecl::Fn(_) => ExportKind::Value,
                    DefaultDecl::TsInterfaceDecl(_) => ExportKind::Type,
                };

                default_export = Some(export_kind);
            }
            ModuleItem::ModuleDecl(ModuleDecl::Import(import_decl)) => {
                find_imports(root, current_folder, import_decl, &mut imported_modules)?;
            }
            ModuleItem::Stmt(Stmt::Decl(decl)) if module_kind.is_declaration() => {
                get_decl_idents(&decl, &mut named_exports, false, module_kind)
            }
            _ => {}
        }
    }

    let mut exports = named_exports
        .into_iter()
        .map(|(name, export)| (ExportName::Named(Cow::Owned(name)), export))
        .collect::<HashMap<_, _>>();

    if let Some(kind) = default_export {
        exports.insert(ExportName::Default, Export::new(kind, Visibility::Exported));
    }

    let module = SourceModule {
        path: normalized_path,
        exports,
        imported_modules,
        is_wildcard_imported: Cell::new(false),
        is_dts: tsconfig.dts,
    };

    Ok(module)
}

fn find_imports(
    root: &Path,
    current_folder: &Path,
    import_decl: swc_ecma_ast::ImportDecl,
    imports: &mut HashMap<NormalizedModulePath, Vec<Import>>,
) -> anyhow::Result<()> {
    // If this doesn't start with . it's a global module -> bail
    // TODO: is there a better way to detect this?
    if !import_decl.src.value.starts_with(".") {
        return Ok(());
    }

    // TODO: handle CSS & other non-code imports

    let normalized_import_source =
        resolve_import_path(&root, current_folder, import_decl.src.value.deref())?;

    let module_imports = imports
        .entry(normalized_import_source)
        .or_insert_with(|| Vec::new());

    for specifier in &import_decl.specifiers {
        match specifier {
            ImportSpecifier::Named(named) => {
                let import_name = (named.imported.as_ref())
                    .unwrap_or(&named.local)
                    .sym
                    .to_string();

                if import_name == "default" {
                    module_imports.push(Import::Default)
                } else {
                    module_imports.push(Import::Named(import_name));
                }
            }
            ImportSpecifier::Default(_) => {
                module_imports.push(Import::Default);
            }
            ImportSpecifier::Namespace(_) => {
                module_imports.push(Import::Wildcard);
            }
        }
    }

    Ok(())
}
