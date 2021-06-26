// Scope analysis inspired by
// https://github.com/nestdotland/analyzer/blob/932db812b8467e1ad19ad1a5d440d56a2e64dd08/analyzer_tree/scopes.rs

use std::{
    collections::{HashMap, HashSet},
    fmt::Display,
};

use swc_atoms::JsWord;
use swc_common::Span;
use swc_ecma_ast::{
    ArrayPat, ArrowExpr, BindingIdent, BlockStmt, BlockStmtOrExpr, ClassDecl, ClassExpr,
    DefaultDecl, ExportDecl, ExportDefaultDecl, ExportDefaultExpr, ExportSpecifier, Expr,
    ExprOrSuper, FnDecl, FnExpr, Function, Ident, ImportDecl, ImportDefaultSpecifier,
    ImportNamedSpecifier, ImportSpecifier, ImportStarAsSpecifier, MemberExpr, NamedExport,
    ObjectPatProp, PropName, TsConditionalType, TsEntityName, TsEnumDecl, TsExprWithTypeArgs,
    TsIndexSignature, TsInterfaceDecl, TsMappedType, TsPropertySignature, TsType, TsTypeAliasDecl,
    TsTypeParam, TsTypeQuery, TsTypeQueryExpr, TsTypeRef,
};
use swc_ecma_visit::Node;

use crate::{
    ast_utils::walk_ts_qualified_name,
    dependency_graph::{ExportKind, ExportName, ImportName},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScopeKind {
    Root,
    Type,
    Block,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, PartialOrd, Ord, Hash)]
pub struct ScopeId(usize);

impl ScopeId {
    pub fn root() -> Self {
        ScopeId(0)
    }
}

impl Display for ScopeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone)]
pub struct Scope {
    pub(crate) id: ScopeId,
    pub(crate) kind: ScopeKind,
    pub(crate) parent: Option<ScopeId>,
    pub(crate) bindings: HashSet<JsWord>,
    pub(crate) type_bindings: HashMap<JsWord, Span>,
    pub(crate) references: HashSet<JsWord>,
    pub(crate) type_references: HashSet<JsWord>,
    pub(crate) ambiguous_references: HashSet<JsWord>,
}

impl Scope {
    pub fn new(id: usize, parent: Option<ScopeId>, kind: ScopeKind) -> Self {
        Scope {
            id: ScopeId(id),
            kind,
            parent,
            bindings: HashSet::new(),
            type_bindings: HashMap::new(),
            references: HashSet::new(),
            type_references: HashSet::new(),
            ambiguous_references: HashSet::new(),
        }
    }
}

#[derive(Debug)]
pub struct ModuleExport {
    pub(crate) span: Span,
    pub(crate) name: ExportName,
    pub(crate) local_name: Option<JsWord>,
    pub(crate) kind: ExportKind,
}

#[derive(Debug)]
pub struct ModuleImport {
    pub imported_name: ImportName,
    pub local_binding: Option<JsWord>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ExportState {
    Private,
    InExport,
}

#[derive(Debug)]
pub struct ModuleVisitor {
    pub(crate) scope_stack: Vec<ScopeId>,
    pub(crate) scopes: Vec<Scope>,

    pub(crate) exports: Vec<ModuleExport>,
    pub(crate) imports: HashMap<String, Vec<ModuleImport>>,

    in_type: bool,
    export_state: ExportState,
}

impl ModuleVisitor {
    pub fn new() -> Self {
        let root_scope = Scope::new(0, None, ScopeKind::Root);
        let scope_stack = vec![root_scope.id];
        let scopes = vec![root_scope];

        ModuleVisitor {
            scope_stack,
            scopes,
            in_type: false,
            export_state: ExportState::Private,
            exports: Vec::new(),
            imports: HashMap::new(),
        }
    }

    fn enter_scope(&mut self, kind: ScopeKind) {
        let new_scope = Scope::new(self.scopes.len(), self.scope_stack.last().copied(), kind);
        self.scope_stack.push(new_scope.id);
        self.scopes.push(new_scope);
    }

    fn exit_scope(&mut self) {
        self.scope_stack
            .pop()
            .expect("Scope stack should always contain at least one element");
    }

    fn enter_type(&mut self) {
        //assert!(!self.in_type);
        self.in_type = true;
    }

    fn exit_type(&mut self) {
        //assert!(self.in_type);
        self.in_type = false;
    }

    fn enter_export(&mut self) {
        self.export_state = ExportState::InExport;
    }

    fn exit_export(&mut self) {
        self.export_state = ExportState::Private;
    }

    fn current_scope(&mut self) -> &mut Scope {
        let scope_id = self
            .scope_stack
            .last()
            .expect("Scope stack should always contain at least one element");

        &mut self.scopes[scope_id.0]
    }

    fn add_binding(&mut self, ident: &Ident) {
        let scope = self.current_scope();
        scope.bindings.insert(ident.sym.clone());
    }

    fn add_type_binding(&mut self, ident: &Ident) {
        let scope = self.current_scope();
        scope.type_bindings.insert(ident.sym.clone(), ident.span);
    }

    fn mark_used_atom(&mut self, atom: &JsWord) {
        let scope = self.current_scope();
        scope.references.insert(atom.clone());
    }

    fn mark_used(&mut self, ident: &Ident) {
        self.mark_used_atom(&ident.sym);
    }

    fn mark_type_used(&mut self, ident: &Ident) {
        let scope = self.current_scope();
        scope.type_references.insert(ident.sym.clone());
    }

    fn mark_ambiguous_used_atom(&mut self, atom: &JsWord) {
        let scope = self.current_scope();
        scope.ambiguous_references.insert(atom.clone());
    }

    fn mark_ambiguous_used(&mut self, ident: &Ident) {
        self.mark_ambiguous_used_atom(&ident.sym);
    }

    fn in_root_scope(&self) -> bool {
        self.scope_stack.last().unwrap().0 == 0
    }

    fn register_decl(&mut self, name: &Ident, span: Span, kind: ExportKind) {
        if !self.in_root_scope() {
            return;
        }

        match self.export_state {
            ExportState::Private => {}
            ExportState::InExport => self.exports.push(ModuleExport {
                name: ExportName::Named(name.sym.clone()),
                local_name: Some(name.sym.clone()),
                span,
                kind,
            }),
        }
    }
}

impl Default for ModuleVisitor {
    fn default() -> Self {
        Self::new()
    }
}

impl swc_ecma_visit::Visit for ModuleVisitor {
    fn visit_export_decl(&mut self, export_decl: &ExportDecl, parent: &dyn Node) {
        self.enter_export();
        self.visit_decl(&export_decl.decl, parent);
        self.exit_export();
    }

    fn visit_export_default_decl(&mut self, default_decl: &ExportDefaultDecl, _parent: &dyn Node) {
        let (local_ident, kind) = match &default_decl.decl {
            DefaultDecl::Class(ClassExpr { ident, .. }) => (ident.as_ref(), ExportKind::Class),
            DefaultDecl::Fn(FnExpr { ident, .. }) => (ident.as_ref(), ExportKind::Value),
            DefaultDecl::TsInterfaceDecl(TsInterfaceDecl { id: ident, .. }) => {
                (Some(ident), ExportKind::Type)
            }
        };

        self.exports.push(ModuleExport {
            name: ExportName::Default,
            span: default_decl.span,
            local_name: local_ident.map(|ident| ident.sym.clone()),
            kind,
        });

        if let Some(local_ident) = local_ident {
            match default_decl.decl {
                DefaultDecl::Fn(_) => {
                    self.add_binding(local_ident);
                }
                DefaultDecl::TsInterfaceDecl(_) => {
                    self.add_type_binding(local_ident);
                }
                DefaultDecl::Class(_) => {
                    self.add_binding(local_ident);
                    self.add_type_binding(local_ident);
                }
            }
        }

        match &default_decl.decl {
            DefaultDecl::Class(class) => {
                self.visit_class_expr(class, default_decl);
            }
            DefaultDecl::Fn(fn_expr) => self.visit_fn_expr(fn_expr, default_decl),
            DefaultDecl::TsInterfaceDecl(ts_interface) => {
                self.visit_ts_interface_decl(ts_interface, default_decl);
            }
        }
    }

    fn visit_export_default_expr(
        &mut self,
        export_default_expr: &ExportDefaultExpr,
        _parent: &dyn Node,
    ) {
        self.exports.push(ModuleExport {
            name: ExportName::Default,
            local_name: None,
            span: export_default_expr.span,
            kind: ExportKind::Unknown,
        });

        match &*export_default_expr.expr {
            Expr::Ident(ident) => self.mark_ambiguous_used(&ident),
            _ => self.visit_expr(&export_default_expr.expr, export_default_expr),
        }
    }

    fn visit_named_export(&mut self, named_export: &NamedExport, _parent: &dyn Node) {
        // I don't like this code.
        let (mut exports, mut imports): (Vec<ModuleExport>, Vec<ModuleImport>) = named_export
            .specifiers
            .iter()
            .filter_map(|specifier| match specifier {
                ExportSpecifier::Namespace(namespace_export) => Some((
                    ModuleExport {
                        name: ExportName::Named(namespace_export.name.sym.clone()),
                        local_name: None,
                        span: namespace_export.span,
                        kind: ExportKind::Unknown,
                    },
                    ModuleImport {
                        imported_name: ImportName::Wildcard,
                        local_binding: None,
                    },
                )),
                ExportSpecifier::Default(_default_export) => {
                    // Do nothing. As far as I can tell this form is not valid ES - why does it exist in SWC's AST?
                    None
                }
                ExportSpecifier::Named(named) => {
                    let name = named.exported.as_ref().unwrap_or(&named.orig).sym.clone();

                    let export_name = match name.as_ref() {
                        "default" => ExportName::Default,
                        _ => ExportName::Named(name),
                    };

                    Some((
                        ModuleExport {
                            name: export_name,
                            span: named.span,
                            local_name: Some(named.orig.sym.clone()),
                            kind: ExportKind::Unknown,
                        },
                        ModuleImport {
                            imported_name: ImportName::Named(named.orig.sym.clone()),
                            local_binding: None,
                        },
                    ))
                }
            })
            .unzip();

        // TODO - this technically allows show invalid forms? You can't re-export * without specifying a source
        if let Some(source) = &named_export.src {
            let imports_for_module = self
                .imports
                .entry(source.value.to_string())
                .or_insert_with(Vec::new);
            imports_for_module.append(&mut imports);
        }

        // If this is not a re-export, mark referenced local identifiers as used
        if named_export.src.is_none() {
            for export in &exports {
                if let Some(local_name) = &export.local_name {
                    self.mark_ambiguous_used_atom(local_name);
                }
            }
        }

        self.exports.append(&mut exports);
    }

    fn visit_import_decl(&mut self, import_decl: &ImportDecl, _parent: &dyn Node) {
        let mut new_imports = Vec::new();

        // TODO: Do we ever need to access import_decl.asserts? What does it do and why?

        // TODO: Should we take advantage of import_decl.type_only (import type {} from "foo")?
        // We treat all imports as ambiguous anyways.

        for specifier in &import_decl.specifiers {
            match specifier {
                ImportSpecifier::Named(ImportNamedSpecifier {
                    local, imported, ..
                }) => {
                    let imported = imported.as_ref().unwrap_or(local);

                    let name = match imported.sym.as_ref() {
                        "default" => ImportName::Default,
                        _ => ImportName::Named(imported.sym.clone()),
                    };

                    new_imports.push(ModuleImport {
                        imported_name: name,
                        local_binding: Some(local.sym.clone()),
                    });
                }
                ImportSpecifier::Default(ImportDefaultSpecifier { local, .. }) => {
                    new_imports.push(ModuleImport {
                        imported_name: ImportName::Default,
                        local_binding: Some(local.sym.clone()),
                    });
                }
                ImportSpecifier::Namespace(ImportStarAsSpecifier { local, .. }) => {
                    new_imports.push(ModuleImport {
                        imported_name: ImportName::Wildcard,
                        local_binding: Some(local.sym.clone()),
                    });
                }
            }
        }

        let module_imports = self
            .imports
            .entry(import_decl.src.value.to_string())
            .or_insert_with(Vec::new);

        module_imports.append(&mut new_imports);
    }

    fn visit_fn_decl(&mut self, fn_decl: &FnDecl, _parent: &dyn Node) {
        self.register_decl(&fn_decl.ident, fn_decl.function.span, ExportKind::Value);

        self.add_binding(&fn_decl.ident);

        self.visit_function(&fn_decl.function, fn_decl);
    }

    fn visit_fn_expr(&mut self, fn_expr: &FnExpr, _parent: &dyn Node) {
        self.visit_function(&fn_expr.function, fn_expr);
    }

    fn visit_arrow_expr(&mut self, arrow_expr: &ArrowExpr, _parent: &dyn Node) {
        self.enter_scope(ScopeKind::Block);

        // Notably we skip the extra scope introduced by BlockStmtOrExpr

        self.visit_pats(&arrow_expr.params, arrow_expr);

        if let Some(type_params) = &arrow_expr.type_params {
            self.visit_ts_type_param_decl(type_params, arrow_expr);
        }

        if let Some(return_type) = &arrow_expr.return_type {
            self.visit_ts_type_ann(return_type, arrow_expr);
        }

        match &arrow_expr.body {
            BlockStmtOrExpr::BlockStmt(block) => {
                for statement in &block.stmts {
                    self.visit_stmt(statement, block);
                }
            }
            BlockStmtOrExpr::Expr(expr) => {
                self.visit_expr(expr, arrow_expr);
            }
        }

        self.exit_scope();
    }

    fn visit_function(&mut self, function: &Function, _parent: &dyn Node) {
        // We create a scope here, because type parameters and arguments are part of the same scope as the body.
        self.enter_scope(ScopeKind::Block);

        self.visit_params(&function.params, function);
        self.visit_decorators(&function.decorators, function);

        if let Some(return_type) = &function.return_type {
            self.visit_ts_type_ann(return_type, function);
        }

        if let Some(type_param_decl) = &function.type_params {
            self.visit_ts_type_param_decl(type_param_decl, function)
        }

        // Do this explicitly instead of calling visit_block_stmt, because we don't want a separate block scope.
        if let Some(body) = &function.body {
            self.visit_stmts(&body.stmts, body);
        }

        self.exit_scope();
    }

    fn visit_class_decl(&mut self, class_decl: &ClassDecl, _parent: &dyn Node) {
        self.register_decl(&class_decl.ident, class_decl.ident.span, ExportKind::Class);

        self.add_binding(&class_decl.ident);
        self.add_type_binding(&class_decl.ident);

        self.enter_scope(ScopeKind::Type);
        self.visit_class(&class_decl.class, class_decl);
        self.exit_scope();
    }

    fn visit_ts_interface_decl(&mut self, interface_decl: &TsInterfaceDecl, _parent: &dyn Node) {
        self.register_decl(&interface_decl.id, interface_decl.id.span, ExportKind::Type);
        self.add_type_binding(&interface_decl.id);

        self.enter_type();
        self.enter_scope(ScopeKind::Type);

        if let Some(type_params) = &interface_decl.type_params {
            self.visit_ts_type_param_decl(type_params, interface_decl);
        }

        for ts_expr in &interface_decl.extends {
            self.visit_ts_expr_with_type_args(ts_expr, interface_decl);
        }

        self.visit_ts_interface_body(&interface_decl.body, interface_decl);

        self.exit_scope();
        self.exit_type();
    }

    fn visit_ts_property_signature(
        &mut self,
        property_signature: &TsPropertySignature,
        _parent: &dyn Node,
    ) {
        if let Some(type_param_decl) = &property_signature.type_params {
            self.visit_ts_type_param_decl(type_param_decl, property_signature);
        }

        if let Some(type_ann) = &property_signature.type_ann {
            self.visit_ts_type_ann(type_ann, property_signature);
        }

        // TODO: Should we ever visit init or params?
    }

    fn visit_ts_index_signature(&mut self, index_signature: &TsIndexSignature, parent: &dyn Node) {
        self.enter_scope(ScopeKind::Block);

        swc_ecma_visit::visit_ts_index_signature(self, index_signature, parent);

        self.exit_scope();
    }

    fn visit_prop_name(&mut self, prop_name: &PropName, _parent: &dyn Node) {
        if let PropName::Computed(computed) = prop_name {
            self.visit_expr(&computed.expr, prop_name);
        }
    }

    fn visit_ts_type_alias_decl(&mut self, type_alias_decl: &TsTypeAliasDecl, _parent: &dyn Node) {
        self.register_decl(
            &type_alias_decl.id,
            type_alias_decl.id.span,
            ExportKind::Type,
        );
        self.add_type_binding(&type_alias_decl.id);

        self.enter_type();
        self.enter_scope(ScopeKind::Type);

        if let Some(type_params) = &type_alias_decl.type_params {
            self.visit_ts_type_param_decl(type_params, type_alias_decl);
        }

        self.visit_ts_type(&type_alias_decl.type_ann, type_alias_decl);

        self.exit_scope();
        self.exit_type();
    }

    fn visit_ts_mapped_type(&mut self, mapped_type: &TsMappedType, parent: &dyn Node) {
        self.enter_scope(ScopeKind::Type);
        swc_ecma_visit::visit_ts_mapped_type(self, mapped_type, parent);
        self.exit_scope();
    }

    fn visit_ts_conditional_type(
        &mut self,
        conditional_type: &TsConditionalType,
        _parent: &dyn Node,
    ) {
        self.enter_scope(ScopeKind::Type);

        self.visit_ts_type(&conditional_type.check_type, conditional_type);
        self.visit_ts_type(&conditional_type.extends_type, conditional_type);

        self.enter_scope(ScopeKind::Type);
        self.visit_ts_type(&conditional_type.true_type, conditional_type);
        self.exit_scope();

        self.enter_scope(ScopeKind::Type);
        self.visit_ts_type(&conditional_type.false_type, conditional_type);
        self.exit_scope();

        self.exit_scope();
    }

    fn visit_ts_expr_with_type_args(&mut self, ts_expr: &TsExprWithTypeArgs, _parent: &dyn Node) {
        match &ts_expr.expr {
            TsEntityName::TsQualifiedName(_) => {
                // TODO?
            }
            TsEntityName::Ident(ident) => {
                self.mark_type_used(ident);
            }
        }

        if let Some(type_args) = &ts_expr.type_args {
            self.visit_ts_type_param_instantiation(type_args, ts_expr);
        }
    }

    fn visit_ts_type_param(&mut self, type_param: &TsTypeParam, _parent: &dyn Node) {
        self.add_type_binding(&type_param.name);

        if let Some(default) = &type_param.default {
            self.visit_ts_type(default, type_param);
        }

        if let Some(constraint) = &type_param.constraint {
            self.visit_ts_type(constraint, constraint);
        }
    }

    fn visit_ts_type_ref(&mut self, type_ref: &TsTypeRef, _parent: &dyn Node) {
        match &type_ref.type_name {
            TsEntityName::TsQualifiedName(_) => {
                // TODO?
            }
            TsEntityName::Ident(ident) => {
                self.mark_type_used(ident);
            }
        }

        if let Some(type_params) = &type_ref.type_params {
            self.visit_ts_type_param_instantiation(type_params, type_ref);
        }
    }

    fn visit_ts_type_query(&mut self, type_query: &TsTypeQuery, _parent: &dyn Node) {
        match &type_query.expr_name {
            TsTypeQueryExpr::TsEntityName(entity_name) => match entity_name {
                TsEntityName::TsQualifiedName(qualified_name) => {
                    let ident = walk_ts_qualified_name(&qualified_name);
                    self.mark_used(ident);
                }
                TsEntityName::Ident(ident) => {
                    self.mark_used(&ident);
                }
            },
            TsTypeQueryExpr::Import(_import) => {
                todo!("typeof on import items not implemented")
            }
        }
    }

    fn visit_ts_type(&mut self, ts_type: &TsType, parent: &dyn Node) {
        self.enter_type();
        swc_ecma_visit::visit_ts_type(self, ts_type, parent);
        self.exit_type();
    }

    fn visit_ts_enum_decl(&mut self, ts_enum_decl: &TsEnumDecl, _parent: &dyn Node) {
        self.register_decl(&ts_enum_decl.id, ts_enum_decl.span, ExportKind::Enum);
        self.visit_ts_enum_members(&ts_enum_decl.members, ts_enum_decl);
    }

    fn visit_block_stmt(&mut self, block: &BlockStmt, _parent: &dyn Node) {
        self.enter_scope(ScopeKind::Block);
        self.visit_stmts(&block.stmts, block);
        self.exit_scope();
    }

    fn visit_binding_ident(&mut self, ident: &BindingIdent, _parent: &dyn Node) {
        self.register_decl(&ident.id, ident.id.span, ExportKind::Value);
        self.add_binding(&ident.id);

        if let Some(type_ann) = &ident.type_ann {
            self.visit_ts_type_ann(type_ann, ident);
        }
    }

    fn visit_array_pat(&mut self, array: &ArrayPat, _parent: &dyn Node) {
        for pat in array.elems.iter().flatten() {
            self.visit_pat(pat, array);
        }
    }

    fn visit_object_pat_prop(&mut self, pat_prop: &ObjectPatProp, _parent: &dyn Node) {
        match pat_prop {
            ObjectPatProp::KeyValue(kv) => {
                match &kv.key {
                    // TODO?
                    swc_ecma_ast::PropName::Ident(_ident) => {}
                    swc_ecma_ast::PropName::Str(_s) => {}
                    swc_ecma_ast::PropName::Num(_n) => {}
                    swc_ecma_ast::PropName::Computed(computed) => {
                        self.visit_expr(&computed.expr, kv);
                    }
                    swc_ecma_ast::PropName::BigInt(_bi) => {}
                }
                self.visit_pat(&kv.value, kv);
            }
            ObjectPatProp::Assign(assign) => {
                self.register_decl(&assign.key, assign.span, ExportKind::Value);
                self.add_binding(&assign.key);

                if let Some(expr) = &assign.value {
                    self.visit_expr(expr, assign);
                }
            }
            ObjectPatProp::Rest(rest) => {
                self.visit_pat(&rest.arg, rest);
            }
        }
    }

    fn visit_ident(&mut self, ident: &Ident, _parent: &dyn Node) {
        self.mark_used(ident);
    }

    fn visit_member_expr(&mut self, member: &MemberExpr, _parent: &dyn Node) {
        match &member.obj {
            ExprOrSuper::Super(_) => {}
            ExprOrSuper::Expr(expr) => {
                self.visit_expr(expr, member);
            }
        }

        if member.computed {
            self.visit_expr(&member.prop, member);
        } else {
            // TODO: Handle non-computed prop?
            // Could be useful for detecting unnecessary default / wildcard imports
        }
    }
}
