use std::{borrow::Cow, collections::HashMap, ops::Deref, path::Path, sync::Arc};

use anyhow::anyhow;
use rayon::prelude::*;
use swc_common::{FilePathMapping, SourceMap, Span};
use swc_ecma_ast::{Decl, DefaultDecl, Ident, ModuleDecl, ModuleItem, ObjectPatProp, Pat, Stmt};

use crate::dependency_graph::{
    normalize_module_path, resolve_import_path, Export, ExportKind, ExportName, Import, Module,
    ModuleKind, ModuleSourceAndLine, NormalizedModulePath, Visibility,
};

fn create_export_source(
    module: &Module,
    source_map: &SourceMap,
    span: Span,
) -> ModuleSourceAndLine {
    let line = source_map
        .lookup_line(span.lo())
        .expect("The offset should always be in bounds")
        .line;
    ModuleSourceAndLine::new(module.source.clone(), line)
}

fn create_export_from_id<'a>(
    module: &Module,
    source_map: &SourceMap,
    ident: &Ident,
    kind: ExportKind,
    visibility: Visibility,
) -> (ExportName<'a>, Export) {
    let line = source_map
        .lookup_line(ident.span.lo())
        .expect("The offset should always be in bounds")
        .line;

    let location = ModuleSourceAndLine::new(module.source.clone(), line);
    let export = Export::new(kind, visibility, location);

    let name = ExportName::Named(Cow::Owned(ident.sym.to_string()));

    (name, export)
}

fn get_decl_exports(
    decl: &Decl,
    is_export_declaration: bool,
    module: &mut Module,
    source_map: &SourceMap,
) {
    let should_export_value = is_export_declaration;
    let should_export_type = is_export_declaration || module.kind.is_declaration();
    let type_visibility = if module.kind.is_declaration() {
        Visibility::ImplicitlyExported
    } else {
        Visibility::Exported
    };

    match decl {
        // TODO is this correct with classes exported from d.ts files?
        Decl::Class(class) if should_export_value => {
            module.add_export(create_export_from_id(
                module,
                source_map,
                &class.ident,
                ExportKind::Value,
                Visibility::Exported,
            ));
        }
        Decl::Fn(f) if should_export_value => {
            module.add_export(create_export_from_id(
                module,
                source_map,
                &f.ident,
                ExportKind::Value,
                Visibility::Exported,
            ));
        }
        Decl::Var(var) if should_export_value => {
            for declarator in &var.decls {
                match &declarator.name {
                    Pat::Ident(id) => {
                        module.add_export(create_export_from_id(
                            module,
                            source_map,
                            &id.id,
                            ExportKind::Value,
                            Visibility::Exported,
                        ));
                    }
                    Pat::Object(pat) => {
                        for prop in pat.props.iter() {
                            match prop {
                                ObjectPatProp::KeyValue(_) | ObjectPatProp::Rest(_) => {}
                                ObjectPatProp::Assign(assign) => {
                                    module.add_export(create_export_from_id(
                                        module,
                                        source_map,
                                        &assign.key,
                                        ExportKind::Value,
                                        Visibility::Exported,
                                    ))
                                }
                            }
                        }
                    }
                    // TODO: Are these even syntactically valid?
                    Pat::Array(_)
                    | Pat::Rest(_)
                    | Pat::Assign(_)
                    | Pat::Invalid(_)
                    | Pat::Expr(_) => {
                        todo!()
                    }
                }
            }
        }
        Decl::TsInterface(interface_decl) if should_export_type => {
            module.add_export(create_export_from_id(
                module,
                source_map,
                &interface_decl.id,
                ExportKind::Type,
                type_visibility,
            ));
        }
        Decl::TsTypeAlias(type_alias_decl) if should_export_type => {
            module.add_export(create_export_from_id(
                module,
                source_map,
                &type_alias_decl.id,
                ExportKind::Type,
                type_visibility,
            ));
        }
        Decl::TsEnum(enum_decl) if should_export_type => {
            module.add_export(create_export_from_id(
                module,
                source_map,
                &enum_decl.id,
                ExportKind::Type,
                type_visibility,
            ));
        }
        _ => {}
    }
}

fn parse_import_decl(
    root: &Path,
    current_folder: &Path,
    import_decl: swc_ecma_ast::ImportDecl,
    module: &mut Module,
) -> anyhow::Result<()> {
    use swc_ecma_ast::ImportSpecifier;

    // If this doesn't start with . it's a global module -> bail
    // TODO: is there a better way to detect this?
    if !import_decl.src.value.starts_with('.') {
        return Ok(());
    }

    // TODO: handle CSS & other non-code imports

    let normalized_import_source =
        resolve_import_path(&root, current_folder, import_decl.src.value.deref())?;

    let module_imports = module
        .imported_modules
        .entry(normalized_import_source)
        .or_insert_with(Vec::new);

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

    let source_map = SourceMap::new(FilePathMapping::empty());
    let source_file = source_map.load_file(file_path)?;
    let input = StringInput::from(source_file.deref());

    let lexer = Lexer::new(
        Syntax::Typescript(tsconfig),
        swc_ecma_parser::JscTarget::Es2020,
        input,
        None,
    );

    let file_path = Arc::new(file_path.to_path_buf());

    let mut parser = Parser::new_from(lexer);

    // TODO put behind debug logging
    // println!("Parsing {}", file_path.to_string_lossy());

    let swc_module = parser
        .parse_module()
        .map_err(|err| anyhow!("Failed to parse module: {:?}", err))?;

    let mut module = Module::new(file_path, normalized_path, module_kind);

    let mut default_export: Option<(ExportKind, ModuleSourceAndLine)> = None;

    for item in swc_module.body {
        match item {
            ModuleItem::ModuleDecl(ModuleDecl::ExportDecl(export_decl)) => {
                get_decl_exports(&export_decl.decl, true, &mut module, &source_map);
            }
            ModuleItem::Stmt(Stmt::Decl(decl)) if module_kind.is_declaration() => {
                get_decl_exports(&decl, false, &mut module, &source_map)
            }
            ModuleItem::ModuleDecl(ModuleDecl::ExportNamed(named_export)) => {
                for specifier in &named_export.specifiers {
                    match specifier {
                        swc_ecma_ast::ExportSpecifier::Namespace(_) => {
                            todo!("Namespace re-exports not supported yet.");
                        }
                        swc_ecma_ast::ExportSpecifier::Default(default) => {
                            module.add_export(create_export_from_id(
                                &module,
                                &source_map,
                                &default.exported,
                                ExportKind::Unknown,
                                Visibility::Exported,
                            ));
                        }
                        swc_ecma_ast::ExportSpecifier::Named(named) => {
                            let name = named.exported.as_ref().unwrap_or(&named.orig);

                            module.add_export(create_export_from_id(
                                &module,
                                &source_map,
                                name,
                                ExportKind::Unknown,
                                Visibility::Exported,
                            ))
                        }
                    }
                }
            }
            ModuleItem::ModuleDecl(ModuleDecl::ExportDefaultExpr(default_export_expr)) => {
                let location = create_export_source(&module, &source_map, default_export_expr.span);
                default_export = Some((ExportKind::Value, location));
            }
            ModuleItem::ModuleDecl(ModuleDecl::ExportDefaultDecl(decl)) => {
                let export_kind = match decl.decl {
                    DefaultDecl::Class(_) | DefaultDecl::Fn(_) => ExportKind::Value,
                    DefaultDecl::TsInterfaceDecl(_) => ExportKind::Type,
                };

                let location = create_export_source(&&module, &source_map, decl.span);
                default_export = Some((export_kind, location));
            }
            ModuleItem::ModuleDecl(ModuleDecl::Import(import_decl)) => {
                parse_import_decl(root, current_folder, import_decl, &mut module)?;
            }

            _ => {}
        }
    }

    if let Some((kind, location)) = default_export {
        module.add_export((
            ExportName::Default,
            Export::new(kind, Visibility::Exported, location),
        ));
    }

    Ok(module)
}

pub fn parse_all_modules(root: &Path) -> HashMap<NormalizedModulePath, Module> {
    ignore::Walk::new(&root)
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

            match parse_module(&root, &file_path, module_kind) {
                Ok(module) => Some((module.normalized_path.clone(), module)),
                Err(err) => {
                    eprintln!("Error while parsing {}: {}", file_path.display(), err);
                    None
                }
            }
        })
        .collect()
}
