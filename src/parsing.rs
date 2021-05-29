use std::{borrow::Cow, collections::HashMap, ops::Deref, path::Path};

use anyhow::anyhow;
use rayon::prelude::*;
use swc_ecma_ast::{Decl, DefaultDecl, ModuleDecl, ModuleItem, Stmt};

use crate::dependency_graph::{
    normalize_module_path, resolve_import_path, Export, ExportKind, ExportName, Import, Module,
    ModuleKind, NormalizedModulePath, Visibility,
};

fn get_decl_exports(
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

fn get_decl_imports(
    root: &Path,
    current_folder: &Path,
    import_decl: swc_ecma_ast::ImportDecl,
    imports: &mut HashMap<NormalizedModulePath, Vec<Import>>,
) -> anyhow::Result<()> {
    use swc_ecma_ast::ImportSpecifier;

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

fn parse_module(
    root: &Path,
    file_path: &Path,
    module_kind: ModuleKind,
) -> anyhow::Result<Module<'static>> {
    use swc_common::BytePos;
    use swc_ecma_parser::{lexer::Lexer, Parser, StringInput, Syntax, TsConfig};

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
                get_decl_exports(&export_decl.decl, &mut named_exports, true, module_kind);
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
                get_decl_imports(root, current_folder, import_decl, &mut imported_modules)?;
            }
            ModuleItem::Stmt(Stmt::Decl(decl)) if module_kind.is_declaration() => {
                get_decl_exports(&decl, &mut named_exports, false, module_kind)
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

    Ok(Module::new(
        normalized_path,
        exports,
        imported_modules,
        module_kind.is_declaration(),
    ))
}

pub fn parse_all_modules(root: &Path) -> HashMap<NormalizedModulePath, Module> {
    walkdir::WalkDir::new(&root)
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
        .collect()
}
