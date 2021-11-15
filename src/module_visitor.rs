// Scope analysis inspired by
// https://github.com/nestdotland/analyzer/blob/932db812b8467e1ad19ad1a5d440d56a2e64dd08/analyzer_tree/scopes.rs

use std::{
    collections::{HashMap, HashSet},
    fmt::Display,
    path::PathBuf,
    sync::Arc,
};

use swc_atoms::JsWord;
use swc_common::{SourceMap, Span};
use swc_ecma_ast::{
    ArrayPat, ArrowExpr, AssignExpr, BindingIdent, BlockStmt, BlockStmtOrExpr, ClassDecl,
    ClassExpr, ClassMember, ClassProp, Constructor, DefaultDecl, DoWhileStmt, ExportDecl,
    ExportDefaultDecl, ExportDefaultExpr, ExportSpecifier, Expr, ExprOrSuper, FnDecl, FnExpr,
    ForInStmt, ForOfStmt, ForStmt, Function, Ident, ImportDecl, ImportDefaultSpecifier,
    ImportNamedSpecifier, ImportSpecifier, ImportStarAsSpecifier, MemberExpr, NamedExport,
    ObjectPatProp, PrivateProp, PropName, TsConditionalType, TsEntityName, TsEnumDecl,
    TsEnumMember, TsExprWithTypeArgs, TsFnType, TsIndexSignature, TsInterfaceDecl, TsMappedType,
    TsMethodSignature, TsPropertySignature, TsType, TsTypeAliasDecl, TsTypeParam, TsTypeQuery,
    TsTypeQueryExpr, TsTypeRef, WhileStmt,
};
use swc_ecma_visit::Node;

use crate::{
    ast_utils::walk_ts_qualified_name,
    dependency_graph::{ExportKind, ExportName, ImportName, ModuleSourceAndLine},
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

    pub fn index(self) -> usize {
        self.0
    }
}

impl Display for ScopeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BindingKind {
    Value,
    Function,
    TsFunctionOverload,
}

#[derive(Debug, Clone)]
pub struct Binding {
    name: JsWord,
    span: Span,
    kind: BindingKind,
}

impl Binding {
    fn new(ident: &Ident, kind: BindingKind) -> Self {
        Binding {
            name: ident.sym.clone(),
            span: ident.span,
            kind,
        }
    }

    fn can_be_shadowed_by(&self, other_kind: BindingKind) -> bool {
        match (self.kind, other_kind) {
            (
                BindingKind::TsFunctionOverload,
                BindingKind::TsFunctionOverload | BindingKind::Function,
            ) => true,
            _ => false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct TypeBinding {
    pub source: ModuleSourceAndLine,
}

#[derive(Debug, Clone)]
pub struct Scope {
    pub(crate) id: ScopeId,
    pub(crate) kind: ScopeKind,
    pub(crate) bindings: HashMap<JsWord, Binding>,
    pub(crate) type_bindings: HashMap<JsWord, TypeBinding>,
    pub(crate) references: HashSet<JsWord>,
    pub(crate) type_references: HashSet<JsWord>,
    pub(crate) ambiguous_references: HashSet<JsWord>,

    pub(crate) parent: Option<ScopeId>,
    pub(crate) children: Vec<ScopeId>,
}

impl Scope {
    pub fn new(id: usize, parent: Option<ScopeId>, kind: ScopeKind) -> Self {
        Scope {
            id: ScopeId(id),
            kind,
            bindings: HashMap::new(),
            type_bindings: HashMap::new(),
            references: HashSet::new(),
            type_references: HashSet::new(),
            ambiguous_references: HashSet::new(),

            parent,
            children: Vec::new(),
        }
    }
}

#[derive(Debug)]
pub struct ModuleExport {
    pub(crate) name: ExportName,
    pub(crate) local_name: Option<JsWord>,
    pub(crate) kind: ExportKind,
    pub(crate) source: ModuleSourceAndLine,
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

struct SourceMapDebugNopAdapter(SourceMap);

impl std::fmt::Debug for SourceMapDebugNopAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("SourceMap").finish()
    }
}

#[derive(Debug)]
pub struct ModuleVisitor {
    root_relative_path: Arc<PathBuf>,

    source_map: SourceMapDebugNopAdapter,

    pub(crate) scope_stack: Vec<ScopeId>,
    pub(crate) scopes: Vec<Scope>,

    pub(crate) exports: Vec<ModuleExport>,
    pub(crate) imports: HashMap<String, Vec<ModuleImport>>,

    in_type: bool,
    export_state: ExportState,
    in_assign_lhs: bool,
}

struct ScopeIterator<'a> {
    scopes: &'a [Scope],
    stack: Vec<&'a Scope>,
}

impl<'a> ScopeIterator<'a> {
    pub fn new(scopes: &'a [Scope], root_scope: &'a Scope) -> Self {
        ScopeIterator {
            scopes,
            stack: vec![root_scope],
        }
    }
}

impl<'a> Iterator for ScopeIterator<'a> {
    type Item = &'a Scope;

    fn next(&mut self) -> Option<Self::Item> {
        let scope = self.stack.pop()?;

        for child_id in &scope.children {
            let child = &self.scopes[child_id.0];
            self.stack.push(child);
        }

        Some(scope)
    }
}

impl ModuleVisitor {
    pub fn new(path: impl Into<Arc<PathBuf>>, source_map: SourceMap) -> Self {
        let root_scope = Scope::new(0, None, ScopeKind::Root);
        let scope_stack = vec![root_scope.id];
        let scopes = vec![root_scope];

        let source_map = SourceMapDebugNopAdapter(source_map);

        ModuleVisitor {
            root_relative_path: path.into(),
            source_map,
            scope_stack,
            scopes,
            in_type: false,
            export_state: ExportState::Private,
            exports: Vec::new(),
            imports: HashMap::new(),
            in_assign_lhs: false,
        }
    }

    fn enter_scope(&mut self, kind: ScopeKind) {
        let new_id = self.scopes.len();
        let curent_scope = self.current_scope();
        curent_scope.children.push(ScopeId(new_id));

        let new_scope = Scope::new(new_id, Some(curent_scope.id), kind);
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

    fn add_binding(&mut self, ident: &Ident, kind: BindingKind) {
        let path = self.root_relative_path.clone();
        let scope = self.current_scope();

        let entry = scope.bindings.entry(ident.sym.clone());

        entry
            .and_modify(|old_binding| {
                if old_binding.can_be_shadowed_by(kind) {
                    old_binding.span = old_binding.span.until(ident.span);
                    old_binding.kind = kind;
                } else {
                    panic!(
                        "Expected {} not to be redeclared ({}:{:?})",
                        ident.sym,
                        path.display(),
                        &ident.span
                    );
                }
            })
            .or_insert_with(|| Binding::new(ident, kind));
    }

    fn add_type_binding(&mut self, ident: &Ident) {
        let source = self.create_span_source(ident.span);
        let scope = self.current_scope();

        let was_in = scope
            .type_bindings
            .insert(ident.sym.clone(), TypeBinding { source });

        debug_assert!(
            was_in.is_none(),
            "Expected {} not to be redeclared",
            ident.sym
        );
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
                kind,
                source: self.create_span_source(span),
            }),
        }
    }

    pub fn child_scopes<'a>(&'a self, scope: &'a Scope) -> impl Iterator<Item = &'a Scope> {
        ScopeIterator::new(&self.scopes, scope)
    }

    pub fn get_scope(&self, scope_id: ScopeId) -> &Scope {
        &self.scopes[scope_id.0]
    }

    fn create_span_source(&self, span: Span) -> ModuleSourceAndLine {
        let line = self
            .source_map
            .0
            // https://github.com/swc-project/swc/issues/2757
            .lookup_line(span.lo())
            .map(|source_and_line| source_and_line.line)
            .unwrap_or(0);

        ModuleSourceAndLine::new(self.root_relative_path.clone(), line)
    }
}

impl swc_ecma_visit::Visit for ModuleVisitor {
    fn visit_export_decl(&mut self, export_decl: &ExportDecl, parent: &dyn Node) {
        self.enter_export();
        self.visit_decl(&export_decl.decl, parent);
        self.exit_export();
    }

    fn visit_export_default_decl(&mut self, default_decl: &ExportDefaultDecl, _parent: &dyn Node) {
        // This is always true... except in TS declare module blocks
        if self.in_root_scope() {
            let (local_ident, kind) = match &default_decl.decl {
                DefaultDecl::Class(ClassExpr { ident, .. }) => (ident.as_ref(), ExportKind::Class),
                DefaultDecl::Fn(FnExpr { ident, .. }) => (ident.as_ref(), ExportKind::Value),
                DefaultDecl::TsInterfaceDecl(TsInterfaceDecl { id: ident, .. }) => {
                    (Some(ident), ExportKind::Type)
                }
            };

            self.exports.push(ModuleExport {
                name: ExportName::Default,
                local_name: local_ident.map(|ident| ident.sym.clone()),
                kind,
                source: self.create_span_source(default_decl.span),
            });
        }

        match &default_decl.decl {
            DefaultDecl::Class(class) => {
                if let Some(ident) = &class.ident {
                    self.add_binding(ident, BindingKind::Value);
                    self.add_type_binding(ident);
                }

                self.visit_class_expr(class, default_decl);
            }
            DefaultDecl::Fn(fn_expr) => {
                if let Some(ident) = &fn_expr.ident {
                    self.add_binding(ident, BindingKind::Function);
                }

                self.visit_fn_expr(fn_expr, default_decl);
            }
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
        if self.in_root_scope() {
            self.exports.push(ModuleExport {
                name: ExportName::Default,
                local_name: None,
                kind: ExportKind::Unknown,
                source: self.create_span_source(export_default_expr.span),
            });
        }

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
                        kind: ExportKind::Unknown,
                        source: self.create_span_source(namespace_export.span),
                    },
                    ModuleImport {
                        imported_name: ImportName::Wildcard,
                        local_binding: None,
                    },
                )),
                ExportSpecifier::Default(_default_export) => {
                    // Do nothing. As far as I can tell this form is not valid ES - why does it exist in SWC's AST?
                    unreachable!("Named default exports should be impossible");
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
                            local_name: Some(named.orig.sym.clone()),
                            kind: ExportKind::Unknown,
                            source: self.create_span_source(named.span),
                        },
                        ModuleImport {
                            imported_name: ImportName::Named(named.orig.sym.clone()),
                            local_binding: None,
                        },
                    ))
                }
            })
            .unzip();

        // TODO - this technically allows invalid forms? You can't re-export * without specifying a source
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
        let kind = if fn_decl.function.body.is_some() {
            BindingKind::Function
        } else {
            BindingKind::TsFunctionOverload
        };

        if kind != BindingKind::TsFunctionOverload {
            self.register_decl(&fn_decl.ident, fn_decl.function.span, ExportKind::Value);
        }

        self.add_binding(&fn_decl.ident, kind);

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

        self.add_binding(&class_decl.ident, BindingKind::Value);
        self.add_type_binding(&class_decl.ident);

        self.visit_class(&class_decl.class, class_decl);
    }

    fn visit_class_members(&mut self, class_members: &[ClassMember], parent: &dyn Node) {
        self.enter_scope(ScopeKind::Type);
        for class_member in class_members {
            self.visit_class_member(class_member, parent);
        }
        self.exit_scope();
    }

    fn visit_class_expr(&mut self, class_expr: &ClassExpr, _parent: &dyn Node) {
        // Do not visit the name

        self.visit_class(&class_expr.class, class_expr);
    }

    fn visit_class_prop(&mut self, class_prop: &ClassProp, _parent: &dyn Node) {
        // Do not visit key, because it's not a reference nor a binding

        if let Some(value) = &class_prop.value {
            self.visit_expr(value, class_prop);
        }

        if let Some(type_ann) = &class_prop.type_ann {
            self.visit_ts_type_ann(type_ann, class_prop);
        }
    }

    fn visit_private_prop(&mut self, class_prop: &PrivateProp, _parent: &dyn Node) {
        // Do not visit key, because it's not a reference nor a binding

        if let Some(value) = &class_prop.value {
            self.visit_expr(value, class_prop);
        }

        if let Some(type_ann) = &class_prop.type_ann {
            self.visit_ts_type_ann(type_ann, class_prop);
        }
    }

    fn visit_constructor(&mut self, constructor: &Constructor, _parent: &dyn Node) {
        self.enter_scope(ScopeKind::Block);

        self.visit_param_or_ts_param_props(&constructor.params, constructor);

        if let Some(body) = &constructor.body {
            for statement in &body.stmts {
                self.visit_stmt(statement, constructor);
            }
        }

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

    fn visit_ts_fn_type(&mut self, ts_fn_type: &TsFnType, _parent: &dyn Node) {
        self.enter_scope(ScopeKind::Type);

        if let Some(type_params) = &ts_fn_type.type_params {
            self.visit_ts_type_param_decl(type_params, ts_fn_type);
        }

        self.visit_ts_fn_params(&ts_fn_type.params, ts_fn_type);
        self.visit_ts_type_ann(&ts_fn_type.type_ann, ts_fn_type);

        self.exit_scope();
    }

    fn visit_ts_enum_decl(&mut self, ts_enum_decl: &TsEnumDecl, _parent: &dyn Node) {
        self.register_decl(&ts_enum_decl.id, ts_enum_decl.span, ExportKind::Enum);
        self.add_binding(&ts_enum_decl.id, BindingKind::Value);
        self.add_type_binding(&ts_enum_decl.id);

        self.enter_scope(ScopeKind::Type);

        self.visit_ts_enum_members(&ts_enum_decl.members, ts_enum_decl);

        self.exit_scope();
    }

    fn visit_ts_method_signature(
        &mut self,
        ts_method_signature: &TsMethodSignature,
        _parent: &dyn Node,
    ) {
        self.visit_opt_ts_type_param_decl(
            ts_method_signature.type_params.as_ref(),
            ts_method_signature,
        );

        self.visit_opt_ts_type_ann(ts_method_signature.type_ann.as_ref(), ts_method_signature);

        self.enter_scope(ScopeKind::Type);
        self.visit_ts_fn_params(&ts_method_signature.params, ts_method_signature);
        self.exit_scope();
    }

    fn visit_ts_enum_members(&mut self, ts_enum_members: &[TsEnumMember], _parent: &dyn Node) {
        for member in ts_enum_members {
            if let Some(init) = &member.init {
                self.visit_expr(init, member);
            }
        }
    }

    fn visit_block_stmt(&mut self, block: &BlockStmt, _parent: &dyn Node) {
        self.enter_scope(ScopeKind::Block);
        self.visit_stmts(&block.stmts, block);
        self.exit_scope();
    }

    fn visit_binding_ident(&mut self, ident: &BindingIdent, _parent: &dyn Node) {
        // Assignments can have a Pat[tern] on the left side, which use binding idents.
        // Without this little hack assignments cause extraneous bindings.
        if self.in_assign_lhs {
            self.mark_used(&ident.id);
        } else {
            self.register_decl(&ident.id, ident.id.span, ExportKind::Value);
            self.add_binding(&ident.id, BindingKind::Value);
        }

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
                self.add_binding(&assign.key, BindingKind::Value);

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

    fn visit_assign_expr(&mut self, assign_expr: &AssignExpr, _parent: &dyn Node) {
        self.in_assign_lhs = true;
        self.visit_pat_or_expr(&assign_expr.left, assign_expr);
        self.in_assign_lhs = false;

        self.visit_expr(&assign_expr.right, assign_expr);
    }

    fn visit_for_in_stmt(&mut self, for_in_statement: &ForInStmt, parent: &dyn Node) {
        self.enter_scope(ScopeKind::Block);
        swc_ecma_visit::visit_for_in_stmt(self, for_in_statement, parent);
        self.exit_scope();
    }

    fn visit_for_of_stmt(&mut self, for_of_statement: &ForOfStmt, parent: &dyn Node) {
        self.enter_scope(ScopeKind::Block);
        swc_ecma_visit::visit_for_of_stmt(self, for_of_statement, parent);
        self.exit_scope();
    }

    fn visit_for_stmt(&mut self, for_statement: &ForStmt, parent: &dyn Node) {
        self.enter_scope(ScopeKind::Block);
        swc_ecma_visit::visit_for_stmt(self, for_statement, parent);
        self.exit_scope();
    }

    fn visit_while_stmt(&mut self, while_statement: &WhileStmt, parent: &dyn Node) {
        self.enter_scope(ScopeKind::Block);
        swc_ecma_visit::visit_while_stmt(self, while_statement, parent);
        self.exit_scope();
    }

    fn visit_do_while_stmt(&mut self, do_while_statement: &DoWhileStmt, parent: &dyn Node) {
        self.enter_scope(ScopeKind::Block);
        swc_ecma_visit::visit_do_while_stmt(self, do_while_statement, parent);
        self.exit_scope();
    }

    fn visit_ts_module_decl(&mut self, n: &swc_ecma_ast::TsModuleDecl, parent: &dyn Node) {
        self.enter_scope(ScopeKind::Block);
        swc_ecma_visit::visit_ts_module_decl(self, n, parent);
        self.exit_scope();
    }
}
