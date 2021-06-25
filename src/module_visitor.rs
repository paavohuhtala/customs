// Scope analysis inspired by
// https://github.com/nestdotland/analyzer/blob/932db812b8467e1ad19ad1a5d440d56a2e64dd08/analyzer_tree/scopes.rs

use std::{
    collections::{HashMap, HashSet},
    fmt::Display,
};

use swc_atoms::JsWord;
use swc_common::Span;
use swc_ecma_ast::{
    ArrayPat, BindingIdent, BlockStmt, ClassDecl, ClassExpr, DefaultDecl, ExportDecl,
    ExportDefaultDecl, ExportDefaultExpr, ExportSpecifier, Expr, FnExpr, Function, Ident,
    ImportDefaultSpecifier, ImportNamedSpecifier, ImportSpecifier, ImportStarAsSpecifier,
    NamedExport, ObjectPatProp, TsInterfaceDecl, TsPropertySignature, TsType, TsTypeAliasDecl,
    TsTypeParam, TsTypeQuery, TsTypeRef,
};
use swc_ecma_visit::Node;

use crate::{
    ast_utils::walk_ts_qualified_name,
    dependency_graph::{ExportName, ImportName},
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

pub struct OwnedScope {
    pub id: ScopeId,
    pub kind: ScopeKind,
    pub parent: Option<ScopeId>,
    pub bindings: HashSet<String>,
    pub type_bindings: HashSet<String>,
    pub references: HashSet<String>,
    pub type_references: HashSet<String>,
}

#[derive(Debug, Clone)]
pub struct Scope {
    id: ScopeId,
    kind: ScopeKind,
    parent: Option<ScopeId>,
    bindings: HashSet<JsWord>,
    type_bindings: HashSet<JsWord>,
    ambiguous_bindings: HashSet<JsWord>,
    references: HashSet<JsWord>,
    type_references: HashSet<JsWord>,
    ambiguous_references: HashSet<JsWord>,
}

impl Scope {
    pub fn new(id: usize, parent: Option<ScopeId>, kind: ScopeKind) -> Self {
        Scope {
            id: ScopeId(id),
            kind,
            parent,
            bindings: HashSet::new(),
            type_bindings: HashSet::new(),
            ambiguous_bindings: HashSet::new(),
            references: HashSet::new(),
            type_references: HashSet::new(),
            ambiguous_references: HashSet::new(),
        }
    }

    pub fn to_owned(&self) -> OwnedScope {
        fn hashset_to_owned(hash_set: &HashSet<JsWord>) -> HashSet<String> {
            hash_set.iter().map(|elem| elem.to_string()).collect()
        }

        OwnedScope {
            id: self.id,
            kind: self.kind,
            parent: self.parent.clone(),
            bindings: hashset_to_owned(&self.bindings),
            references: hashset_to_owned(&self.references),
            type_bindings: hashset_to_owned(&self.type_bindings),
            type_references: hashset_to_owned(&self.type_references),
        }
    }
}

#[derive(Debug)]
pub struct ModuleExport {
    span: Span,
    name: ExportName,
    local_name: Option<JsWord>,
}

#[derive(Debug)]
pub struct ModuleImport {
    name: ImportName,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ExportState {
    Private,
    InExport,
    InDefaultExport,
}

#[derive(Debug)]
pub struct ModuleVisitor {
    scope_stack: Vec<ScopeId>,
    pub scopes: Vec<Scope>,
    in_type: bool,
    export_state: ExportState,

    pub exports: Vec<ModuleExport>,
    pub imports: HashMap<String, Vec<ModuleImport>>,
}

impl ModuleVisitor {
    pub fn new() -> Self {
        let root_scope = Scope::new(0, None, ScopeKind::Root);
        let scope_stack = vec![root_scope.id];
        let scopes = vec![root_scope.clone()];

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
        assert!(self.export_state == ExportState::Private);
        self.export_state = ExportState::InExport;
    }

    fn enter_default_export(&mut self) {
        assert!(self.export_state == ExportState::Private);
        self.export_state = ExportState::InDefaultExport;
    }

    fn exit_export(&mut self) {
        assert!(
            self.export_state == ExportState::InExport
                || self.export_state == ExportState::InDefaultExport
        );
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
        scope.type_bindings.insert(ident.sym.clone());
    }

    fn add_import_binding(&mut self, ident: &Ident) {
        let scope = self.current_scope();
        scope.ambiguous_bindings.insert(ident.sym.clone());
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

    fn register_decl(&mut self, name: &Ident, span: Span) {
        if !self.in_root_scope() {
            return;
        }

        match self.export_state {
            ExportState::Private => {}
            ExportState::InExport => self.exports.push(ModuleExport {
                name: ExportName::Named(name.sym.clone()),
                local_name: None,
                span,
            }),
            ExportState::InDefaultExport => self.exports.push(ModuleExport {
                name: ExportName::Default,
                local_name: None,
                span,
            }),
        }
    }
}

impl swc_ecma_visit::Visit for ModuleVisitor {
    fn visit_export_decl(&mut self, export_decl: &ExportDecl, parent: &dyn Node) {
        self.enter_export();
        self.visit_decl(&export_decl.decl, parent);
        self.exit_export();
    }

    fn visit_export_default_decl(&mut self, default_decl: &ExportDefaultDecl, parent: &dyn Node) {
        let local_ident = match &default_decl.decl {
            DefaultDecl::Class(ClassExpr {
                ident: Some(ident), ..
            })
            | DefaultDecl::Fn(FnExpr {
                ident: Some(ident), ..
            })
            | DefaultDecl::TsInterfaceDecl(TsInterfaceDecl { id: ident, .. }) => Some(ident),
            _ => None,
        };

        self.exports.push(ModuleExport {
            name: ExportName::Default,
            span: default_decl.span,
            local_name: local_ident.map(|ident| ident.sym.clone()),
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

        swc_ecma_visit::visit_export_default_decl(self, default_decl, parent);
    }

    fn visit_export_default_expr(
        &mut self,
        export_default_expr: &ExportDefaultExpr,
        _parent: &dyn Node,
    ) {
        self.exports.push(ModuleExport {
            local_name: None,
            name: ExportName::Default,
            span: export_default_expr.span,
        });

        match &*export_default_expr.expr {
            Expr::Ident(ident) => self.mark_ambiguous_used(&ident),
            _ => self.visit_expr(&export_default_expr.expr, export_default_expr),
        }
    }

    fn visit_named_export(&mut self, named_export: &NamedExport, _parent: &dyn Node) {
        // I don't like this code. We are building improt
        let (mut exports, mut imports): (Vec<ModuleExport>, Vec<ModuleImport>) = named_export
            .specifiers
            .iter()
            .filter_map(|specifier| match specifier {
                ExportSpecifier::Namespace(namespace_export) => Some((
                    ModuleExport {
                        name: ExportName::Named(namespace_export.name.sym.clone()),
                        local_name: None,
                        span: namespace_export.span,
                    },
                    ModuleImport {
                        name: ImportName::Wildcard,
                    },
                )),
                ExportSpecifier::Default(_default_export) => {
                    // Do nothing. As far as I can tell this form is not valid ES - why does it exist in SWC's AST?
                    None
                }
                ExportSpecifier::Named(named) => {
                    let name = named.exported.as_ref().unwrap_or(&named.orig).sym.clone();
                    Some((
                        ModuleExport {
                            name: ExportName::Named(name),
                            span: named.span,
                            local_name: Some(named.orig.sym.clone()),
                        },
                        ModuleImport {
                            name: ImportName::Named(named.orig.sym.clone()),
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
                .or_insert(Vec::new());
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

    fn visit_import_specifier(&mut self, import_specifier: &ImportSpecifier, _parent: &dyn Node) {
        match import_specifier {
            ImportSpecifier::Named(ImportNamedSpecifier { local, .. })
            | ImportSpecifier::Default(ImportDefaultSpecifier { local, .. })
            | ImportSpecifier::Namespace(ImportStarAsSpecifier { local, .. }) => {
                self.add_import_binding(local)
            }
        }
    }

    fn visit_fn_decl(&mut self, fn_decl: &swc_ecma_ast::FnDecl, _parent: &dyn Node) {
        self.register_decl(&fn_decl.ident, fn_decl.function.span);

        self.add_binding(&fn_decl.ident);

        self.visit_function(&fn_decl.function, fn_decl);
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
        self.register_decl(&class_decl.ident, class_decl.ident.span);

        self.add_binding(&class_decl.ident);
        self.add_type_binding(&class_decl.ident);
        self.visit_class(&class_decl.class, class_decl);
    }

    fn visit_ts_interface_decl(&mut self, interface_decl: &TsInterfaceDecl, _parent: &dyn Node) {
        self.register_decl(&interface_decl.id, interface_decl.id.span);
        self.add_type_binding(&interface_decl.id);

        for base in &interface_decl.extends {
            match &base.expr {
                swc_ecma_ast::TsEntityName::TsQualifiedName(_) => {
                    // TODO?
                }
                swc_ecma_ast::TsEntityName::Ident(ident) => {
                    self.mark_type_used(ident);
                }
            }
        }

        self.enter_type();
        self.enter_scope(ScopeKind::Type);

        if let Some(type_params) = &interface_decl.type_params {
            self.visit_ts_type_param_decl(type_params, interface_decl);
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

    fn visit_ts_type_alias_decl(&mut self, type_alias_decl: &TsTypeAliasDecl, _parent: &dyn Node) {
        self.register_decl(&type_alias_decl.id, type_alias_decl.id.span);
        self.add_type_binding(&type_alias_decl.id);

        if let Some(type_params) = &type_alias_decl.type_params {
            self.enter_scope(ScopeKind::Type);
            self.visit_ts_type_param_decl(type_params, type_alias_decl);
        }

        self.visit_ts_type(&type_alias_decl.type_ann, type_alias_decl);

        if type_alias_decl.type_params.is_some() {
            self.exit_scope();
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
            swc_ecma_ast::TsEntityName::TsQualifiedName(_) => {
                // TODO?
            }
            swc_ecma_ast::TsEntityName::Ident(ident) => {
                self.mark_type_used(ident);
            }
        }
    }

    fn visit_ts_type_query(&mut self, type_query: &TsTypeQuery, _parent: &dyn Node) {
        match &type_query.expr_name {
            swc_ecma_ast::TsTypeQueryExpr::TsEntityName(entity_name) => match entity_name {
                swc_ecma_ast::TsEntityName::TsQualifiedName(qualified_name) => {
                    let ident = walk_ts_qualified_name(&qualified_name);
                    self.mark_used(ident);
                }
                swc_ecma_ast::TsEntityName::Ident(ident) => {
                    self.mark_used(&ident);
                }
            },
            swc_ecma_ast::TsTypeQueryExpr::Import(_import) => {
                todo!("typeof on import items not implemented")
            }
        }
    }

    fn visit_ts_type(&mut self, ts_type: &TsType, parent: &dyn Node) {
        self.enter_type();
        swc_ecma_visit::visit_ts_type(self, ts_type, parent);
        self.exit_type();
    }

    fn visit_block_stmt(&mut self, block: &BlockStmt, _parent: &dyn Node) {
        self.enter_scope(ScopeKind::Block);
        self.visit_stmts(&block.stmts, block);
        self.exit_scope();
    }

    fn visit_binding_ident(&mut self, ident: &BindingIdent, _parent: &dyn Node) {
        self.register_decl(&ident.id, ident.id.span);
        self.add_binding(&ident.id);
    }

    fn visit_array_pat(&mut self, array: &ArrayPat, _parent: &dyn Node) {
        for elem in &array.elems {
            if let Some(pat) = elem {
                self.visit_pat(pat, array);
            }
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
                    swc_ecma_ast::PropName::Computed(_computed) => {}
                    swc_ecma_ast::PropName::BigInt(_bi) => {}
                }
                self.visit_pat(&kv.value, kv);
            }
            ObjectPatProp::Assign(assign) => {
                // self.add_binding(&assign.key);
                if let Some(expr) = &assign.value {
                    self.visit_expr(expr, assign);
                }
            }
            ObjectPatProp::Rest(rest) => {
                self.visit_pat(&rest.arg, rest);
            }
        }
    }

    fn visit_expr(&mut self, expr: &Expr, parent: &dyn Node) {
        match expr {
            Expr::Ident(ident) => {
                // TODO: this is not completely correct?
                self.mark_used(ident);
            }
            Expr::Member(member) => {
                match &member.obj {
                    swc_ecma_ast::ExprOrSuper::Super(_) => {}
                    swc_ecma_ast::ExprOrSuper::Expr(expr) => {
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
            otherwise => {
                swc_ecma_visit::visit_expr(self, otherwise, parent);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};

    use super::ModuleVisitor;
    use crate::{
        dependency_graph::ExportName,
        module_visitor::{Scope, ScopeId},
        parsing::module_from_source,
    };

    use itertools::Itertools;
    use swc_atoms::JsWord;
    use swc_ecma_visit::Visit;

    fn parse_and_visit(source: &'static str) -> ModuleVisitor {
        let (_, module) = module_from_source(
            String::from(source),
            crate::dependency_graph::ModuleKind::TS,
        )
        .unwrap();

        let mut visitor = ModuleVisitor::new();
        visitor.visit_module(&module, &module);
        visitor
    }

    #[test]
    pub fn parse_ts_type() {
        let source = r#"type Foo = { bar: string }"#;

        let spec = TestSpec {
            source,
            exports: vec![],
            scope: TestScope {
                type_bindings: vec!["Foo"],
                ..Default::default()
            },
        };

        run_test(spec);
    }

    #[test]
    pub fn parse_ts_interface() {
        let source = r#"interface Foo { bar: string }"#;

        let spec = TestSpec {
            source,
            exports: vec![],
            scope: TestScope {
                type_bindings: vec!["Foo"],
                inner: vec![TestScope::default()],

                ..Default::default()
            },
        };

        run_test(spec);
    }

    #[test]
    pub fn parse_type_and_value_of_same_name() {
        let source = r#"
            interface Foo { bar: number }
            const Foo = 123
        "#;

        let spec = TestSpec {
            source,
            exports: vec![],
            scope: TestScope {
                bindings: vec!["Foo"],
                type_bindings: vec!["Foo"],
                inner: vec![TestScope::default()],
                ..Default::default()
            },
        };

        run_test(spec);
    }

    #[test]
    pub fn scoping_block() {
        let source = r#"
            const foo = 123
            {
                type Bar = number
                const foo = "456"
            }
        "#;

        let spec = TestSpec {
            source,
            exports: vec![],
            scope: TestScope {
                bindings: vec!["foo"],
                inner: vec![TestScope {
                    bindings: vec!["foo"],
                    type_bindings: vec!["Bar"],
                    ..Default::default()
                }],
                ..Default::default()
            },
        };

        run_test(spec);
    }

    #[test]
    pub fn scoping_function() {
        let source = r#"
            function outerFunction()
            {
                function innerFunction() { }
            }
        "#;

        let spec = TestSpec {
            source,
            exports: vec![],
            scope: TestScope {
                bindings: vec!["outerFunction"],
                inner: vec![TestScope {
                    bindings: vec!["innerFunction"],
                    inner: vec![TestScope::default()],
                    ..Default::default()
                }],
                ..Default::default()
            },
        };

        run_test(spec);
    }

    #[test]
    pub fn scoping_function_generics() {
        let source = r#"
            function f<T>() { }
        "#;

        let spec = TestSpec {
            source,
            exports: vec![],
            scope: TestScope {
                bindings: vec!["f"],
                inner: vec![TestScope {
                    type_bindings: vec!["T"],
                    ..Default::default()
                }],
                ..Default::default()
            },
        };

        run_test(spec);
    }

    #[test]
    pub fn scoping_function_initial() {
        let source = r#"
            function f(a: string, b: string = a) { }
        "#;

        let spec = TestSpec {
            source,
            exports: vec![],
            scope: TestScope {
                bindings: vec!["f"],
                inner: vec![TestScope {
                    bindings: vec!["a", "b"],
                    references: vec!["a"],
                    ..Default::default()
                }],
                ..Default::default()
            },
        };

        run_test(spec);
    }

    #[test]
    pub fn scoping_function_self_reference() {
        let source = r#"
            function f<T>(a: T, b: T = a): T { return f(b) }
        "#;

        let spec = TestSpec {
            source,
            exports: vec![],
            scope: TestScope {
                bindings: vec!["f"],
                inner: vec![TestScope {
                    type_bindings: vec!["T"],
                    bindings: vec!["a", "b"],
                    type_references: vec!["T"],
                    references: vec!["a", "b", "f"],
                    ..Default::default()
                }],
                ..Default::default()
            },
        };

        run_test(spec);
    }

    #[test]
    pub fn scoping_interface_type_parameters() {
        let source = r#"
            interface Foo<T> { x: T }
        "#;

        let spec = TestSpec {
            source,
            exports: vec![],
            scope: TestScope {
                type_bindings: vec!["Foo"],
                inner: vec![TestScope {
                    type_bindings: vec!["T"],
                    type_references: vec!["T"],
                    ..Default::default()
                }],
                ..Default::default()
            },
        };

        run_test(spec);
    }

    #[test]
    pub fn exports_smoke() {
        let source = r#"
            export const exportedConstant = {}
            export function exportedFunction() { }
            export type ExportedType = { }
            export interface ExportedInterface { }
        "#;

        let spec = TestSpec {
            source,
            exports: vec![
                "exportedConstant",
                "exportedFunction",
                "ExportedType",
                "ExportedInterface",
            ],
            scope: TestScope {
                bindings: vec!["exportedConstant", "exportedFunction"],
                type_bindings: vec!["ExportedType", "ExportedInterface"],
                inner: vec![TestScope::default(), TestScope::default()],
                ..Default::default()
            },
        };

        run_test(spec);
    }

    #[test]
    pub fn exports_inner_scope() {
        let source = r#"
            export const exportedFunction = function() {
                const notExported = 10
                function norThis<T>() { }
                const [a, b, c] = [1, 2, 3]
            }
        "#;

        let spec = TestSpec {
            source,
            exports: vec!["exportedFunction"],
            scope: TestScope {
                bindings: vec!["exportedFunction"],
                inner: vec![TestScope {
                    bindings: vec!["notExported", "norThis", "a", "b", "c"],
                    inner: vec![TestScope {
                        type_bindings: vec!["T"],
                        ..Default::default()
                    }],
                    ..Default::default()
                }],
                ..Default::default()
            },
        };

        run_test(spec);
    }

    #[test]
    pub fn exports_export_statement() {
        let source = r#"
            const a = 10
            type Foo = number
            export { a, Foo }
        "#;

        let spec = TestSpec {
            source,
            exports: vec!["a", "Foo"],
            scope: TestScope {
                bindings: vec!["a"],
                type_bindings: vec!["Foo"],
                ambiguous_references: vec!["a", "Foo"],
                ..Default::default()
            },
        };

        run_test(spec);
    }

    #[test]
    pub fn exports_re_export() {
        let source = r#"
            export { a, Foo } from "./a"
        "#;

        let spec = TestSpec {
            source,
            exports: vec!["a", "Foo"],
            scope: TestScope::default(),
        };

        run_test(spec);
    }

    #[test]
    pub fn exports_rename() {
        let source = r#"
            const a = "foo"
            export { a as b }
        "#;

        let spec = TestSpec {
            source,
            exports: vec!["b"],
            scope: TestScope {
                bindings: vec!["a"],
                ambiguous_references: vec!["a"],
                ..Default::default()
            },
        };

        run_test(spec);
    }

    #[test]
    pub fn exports_default_function() {
        let source = r#"
            export default function foo() { }
        "#;

        let spec = TestSpec {
            source,
            exports: vec!["default"],
            scope: TestScope {
                bindings: vec!["foo"],
                inner: vec![TestScope::default()],
                ..Default::default()
            },
        };

        run_test(spec);
    }

    #[test]
    pub fn exports_default_unnamed_function() {
        let source = r#"
            export default function() { }
        "#;

        let spec = TestSpec {
            source,
            exports: vec!["default"],
            scope: TestScope {
                inner: vec![TestScope::default()],
                ..Default::default()
            },
        };

        run_test(spec);
    }

    #[test]
    pub fn exports_default_interface() {
        let source = r#"
            export default interface Foo { a: string, b: number }
        "#;

        let spec = TestSpec {
            source,
            exports: vec!["default"],
            scope: TestScope {
                type_bindings: vec!["Foo"],
                inner: vec![TestScope::default()],
                ..Default::default()
            },
        };

        run_test(spec);
    }

    #[test]
    pub fn exports_default_statement_const() {
        let source = r#"
            const foo = "bar"
            export default foo
        "#;

        let spec = TestSpec {
            source,
            exports: vec!["default"],
            scope: TestScope {
                bindings: vec!["foo"],
                ambiguous_references: vec!["foo"],
                ..Default::default()
            },
        };

        run_test(spec);
    }

    #[test]
    pub fn exports_default_statement_interface() {
        let source = r#"
            interface Foo { x: number }
            export default Foo
        "#;

        let spec = TestSpec {
            source,
            exports: vec!["default"],
            scope: TestScope {
                type_bindings: vec!["Foo"],
                ambiguous_references: vec!["Foo"],
                inner: vec![TestScope::default()],
                ..Default::default()
            },
        };

        run_test(spec);
    }

    #[test]
    pub fn usages_typeof() {
        let source = r#"
            const foo = { a: 10, b: 20 }
            type Foo = typeof foo
            type Bar = Foo
        "#;

        let spec = TestSpec {
            source,
            exports: vec![],
            scope: TestScope {
                bindings: vec!["foo"],
                type_bindings: vec!["Foo", "Bar"],
                references: vec!["foo"],
                type_references: vec!["Foo"],
                ..Default::default()
            },
        };

        run_test(spec);
    }

    #[test]
    pub fn usages_path() {
        let source = r#"
            const foo = { a: { b: { c: 10 } } }
            const bar = { a: { b: { c: 10 } } }
            {
                const bar = foo.a.b.c
                type Bar = typeof bar.a.b.c
            }
        "#;

        let spec = TestSpec {
            source,
            exports: vec![],
            scope: TestScope {
                bindings: vec!["foo", "bar"],
                inner: vec![TestScope {
                    bindings: vec!["bar"],
                    type_bindings: vec!["Bar"],
                    references: vec!["foo", "bar"],
                    ..TestScope::default()
                }],
                ..TestScope::default()
            },
        };

        run_test(spec);
    }

    struct TestScope {
        references: Vec<&'static str>,
        type_references: Vec<&'static str>,
        ambiguous_references: Vec<&'static str>,
        bindings: Vec<&'static str>,
        type_bindings: Vec<&'static str>,
        inner: Vec<TestScope>,
    }

    impl Default for TestScope {
        fn default() -> Self {
            TestScope {
                references: vec![],
                type_references: vec![],
                ambiguous_references: vec![],
                bindings: vec![],
                type_bindings: vec![],
                inner: vec![],
            }
        }
    }

    struct TestSpec {
        source: &'static str,
        exports: Vec<&'static str>,
        scope: TestScope,
    }

    fn run_test(spec: TestSpec) {
        let visitor = parse_and_visit(spec.source);
        println!("{:#?}", visitor);

        assert_eq!(
            spec.exports.len(),
            visitor.exports.len(),
            "Expected export counts to match"
        );

        for export in &spec.exports {
            let export_name = match *export {
                "default" => ExportName::Default,
                name => ExportName::Named(JsWord::from(name)),
            };

            assert!(
                visitor
                    .exports
                    .iter()
                    .find(|export| export.name == export_name)
                    .is_some(),
                "Should contain export {}",
                export
            );
        }

        let scopes_by_id: HashMap<_, _> = visitor
            .scopes
            .iter()
            .map(|scope| (scope.id, scope))
            .collect();

        let scopes_by_parent: HashMap<ScopeId, Vec<&Scope>> = visitor
            .scopes
            .iter()
            .filter_map(|scope| scope.parent.map(|parent_id| (parent_id, scope)))
            .into_group_map();

        fn assert_vec_set_equal(
            kind_singular: &'static str,
            kind_plural: &'static str,
            expected: &[&'static str],
            was: &HashSet<JsWord>,
            scope_id: ScopeId,
        ) {
            assert_eq!(
                expected.len(),
                was.len(),
                "Expected scope {} to contain {} {}",
                scope_id,
                expected.len(),
                kind_plural
            );

            for binding in expected {
                let as_atom = JsWord::from(*binding);
                assert!(
                    was.contains(&as_atom),
                    "Scope {} should contain {} {}",
                    scope_id,
                    kind_singular,
                    binding
                );
            }
        }

        fn check_scope(
            test_scope: &TestScope,
            scope: &Scope,
            scopes_by_parent: &HashMap<ScopeId, Vec<&Scope>>,
        ) {
            assert_vec_set_equal(
                "binding",
                "bindings",
                &test_scope.bindings,
                &scope.bindings,
                scope.id,
            );
            assert_vec_set_equal(
                "type binding",
                "type bindings",
                &test_scope.type_bindings,
                &scope.type_bindings,
                scope.id,
            );
            assert_vec_set_equal(
                "reference",
                "references",
                &test_scope.references,
                &scope.references,
                scope.id,
            );
            assert_vec_set_equal(
                "type reference",
                "type references",
                &test_scope.type_references,
                &scope.type_references,
                scope.id,
            );
            assert_vec_set_equal(
                "ambiguous reference",
                "ambiguous references",
                &test_scope.ambiguous_references,
                &scope.ambiguous_references,
                scope.id,
            );

            let empty_vec = Vec::new();
            let child_scopes = scopes_by_parent.get(&scope.id).unwrap_or(&empty_vec);

            assert_eq!(
                test_scope.inner.len(),
                child_scopes.len(),
                "Expected scope {} to have {} child scopes",
                scope.id,
                test_scope.inner.len()
            );

            for (scope, test_scope) in child_scopes.iter().zip(test_scope.inner.iter()) {
                check_scope(test_scope, scope, scopes_by_parent);
            }
        }

        let root_scope = scopes_by_id.get(&ScopeId::root()).unwrap();
        check_scope(&spec.scope, root_scope, &scopes_by_parent);
    }
}
