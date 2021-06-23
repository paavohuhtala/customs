// Scope analysis inspired by
// https://github.com/nestdotland/analyzer/blob/932db812b8467e1ad19ad1a5d440d56a2e64dd08/analyzer_tree/scopes.rs

use std::collections::{HashMap, HashSet};

use swc_atoms::JsWord;
use swc_common::Span;
use swc_ecma_ast::{
    ArrayPat, BindingIdent, BlockStmt, ClassDecl, DefaultDecl, ExportDecl, ExportDefaultDecl,
    ExportSpecifier, Expr, Ident, ImportDefaultSpecifier, ImportNamedSpecifier, ImportSpecifier,
    ImportStarAsSpecifier, NamedExport, ObjectPatProp, TsInterfaceDecl, TsType, TsTypeAliasDecl,
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

    fn mark_used(&mut self, ident: &Ident) {
        let scope = self.current_scope();
        scope.references.insert(ident.sym.clone());
    }

    fn mark_type_used(&mut self, ident: &Ident) {
        let scope = self.current_scope();
        scope.type_references.insert(ident.sym.clone());
    }

    fn mark_ambiguous_used(&mut self, ident: &Ident) {
        let scope = self.current_scope();
        scope.ambiguous_references.insert(ident.sym.clone());
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

    fn visit_export_default_decl(&mut self, default_decl: &ExportDefaultDecl, _parent: &dyn Node) {
        match &default_decl.decl {
            DefaultDecl::Class(_) | DefaultDecl::Fn(_) | DefaultDecl::TsInterfaceDecl(_) => {
                self.exports.push(ModuleExport {
                    name: ExportName::Default,
                    span: default_decl.span,
                    local_name: None,
                })
            }
        }
    }

    fn visit_named_export(&mut self, named_export: &NamedExport, _parent: &dyn Node) {
        let (mut exports, mut imports) = named_export
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

        // function contains a block statement -> no need to explicitly enter scope
        self.visit_function(&fn_decl.function, fn_decl);
    }

    fn visit_class_decl(&mut self, class_decl: &ClassDecl, _parent: &dyn Node) {
        self.register_decl(&class_decl.ident, class_decl.ident.span);

        self.add_binding(&class_decl.ident);
        self.add_type_binding(&class_decl.ident);
        self.visit_class(&class_decl.class, class_decl);
    }

    /*fn visit_var_decl(&mut self, var_decl: &VarDecl, _parent: &dyn Node) {
        self.register_decl(&class_decl.ident, class_decl.ident.span);

        self.add_binding(&class_decl.ident);
        self.add_type_binding(&class_decl.ident);
        self.visit_class(&class_decl.class, class_decl);
    }*/

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
        self.visit_ts_interface_body(&interface_decl.body, interface_decl);
        self.exit_type();
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
                // TODO: this is not completely correct
                // if !self.in_type {
                self.mark_used(ident);
                //}
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
        let visitor = parse_and_visit(source);

        assert_eq!(1, visitor.scopes.len());
        let root_scope = visitor.scopes.first().unwrap();

        assert_eq!(1, root_scope.type_bindings.len());
        assert!(root_scope.bindings.is_empty());

        assert!(root_scope.type_bindings.contains(&JsWord::from("Foo")));
    }

    #[test]
    pub fn parse_ts_interface() {
        let source = r#"interface Foo { bar: string }"#;
        let visitor = parse_and_visit(source);

        assert_eq!(1, visitor.scopes.len());
        let root_scope = visitor.scopes.first().unwrap();

        assert_eq!(1, root_scope.type_bindings.len());
        assert!(root_scope.bindings.is_empty());

        assert!(root_scope.type_bindings.contains(&JsWord::from("Foo")));
    }

    #[test]
    pub fn parse_type_and_value_of_same_name() {
        let source = r#"
            interface Foo { bar: number }
            const Foo = 123
        "#;
        let visitor = parse_and_visit(source);

        assert_eq!(1, visitor.scopes.len());
        let root_scope = visitor.scopes.first().unwrap();

        assert_eq!(1, root_scope.type_bindings.len());
        assert_eq!(1, root_scope.bindings.len());

        assert!(root_scope.type_bindings.contains(&JsWord::from("Foo")));

        assert!(root_scope.bindings.contains(&JsWord::from("Foo")));
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
        let visitor = parse_and_visit(source);

        assert_eq!(2, visitor.scopes.len());
        let root_scope = &visitor.scopes[0];
        let inner_scope = &visitor.scopes[1];

        assert_eq!(Some(root_scope.id), inner_scope.parent);

        assert_eq!(1, root_scope.bindings.len());
        assert!(root_scope.type_bindings.is_empty());

        assert_eq!(1, inner_scope.bindings.len());
        assert_eq!(1, inner_scope.type_bindings.len());

        assert!(root_scope.bindings.contains(&JsWord::from("foo")));

        assert!(inner_scope.bindings.contains(&JsWord::from("foo")));

        assert!(inner_scope.type_bindings.contains(&JsWord::from("Bar")));
    }

    #[test]
    pub fn scoping_function() {
        let source = r#"
            const outerConstant = "foo" 
            function outerFunction()
            {
                function innerFunction() { }
            }
        "#;
        let visitor = parse_and_visit(source);

        assert_eq!(3, visitor.scopes.len());
        let root_scope = &visitor.scopes[0];
        let outer_function_scope = &visitor.scopes[1];
        let inner_function_scope = &visitor.scopes[2];

        assert_eq!(Some(root_scope.id), outer_function_scope.parent);
        assert_eq!(Some(outer_function_scope.id), inner_function_scope.parent);

        assert_eq!(2, root_scope.bindings.len());
        assert!(root_scope.type_bindings.is_empty());

        assert_eq!(1, outer_function_scope.bindings.len());
        assert!(outer_function_scope.type_bindings.is_empty());

        assert!(inner_function_scope.bindings.is_empty());
        assert!(outer_function_scope.type_bindings.is_empty());

        assert!(root_scope.bindings.contains(&JsWord::from("outerConstant")));

        assert!(root_scope.bindings.contains(&JsWord::from("outerFunction")));

        assert!(outer_function_scope
            .bindings
            .contains(&JsWord::from("innerFunction")));
    }

    #[test]
    pub fn exports_smoke() {
        let source = r#"
            export const exportedConstant = {}
            export function exportedFunction() { }
            export type ExportedType = { }
            export interface ExportedInterface { }
        "#;
        let visitor = parse_and_visit(source);

        let export_names: HashSet<_> = visitor
            .exports
            .iter()
            .map(|export| export.name.clone())
            .collect();

        assert_eq!(4, export_names.len());

        for id in [
            "exportedConstant",
            "exportedFunction",
            "ExportedType",
            "ExportedInterface",
        ] {
            let id = ExportName::Named(JsWord::from(id));
            export_names.contains(&id);
        }
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

        let visitor = parse_and_visit(source);
        assert_eq!(1, visitor.exports.len());

        assert_eq!(
            &ExportName::Named(JsWord::from("exportedFunction")),
            &visitor.exports[0].name
        );
    }

    #[test]
    pub fn usages_typeof() {
        let source = r#"
            const foo = { a: 10, b: 20 }
            type Foo = typeof foo
            type Bar = Foo
        "#;

        let visitor = parse_and_visit(source);
        let root_scope = &visitor.scopes[0];

        assert_eq!(
            1,
            root_scope.references.len(),
            "Should have at exactly one value reference"
        );

        assert_eq!(
            1,
            root_scope.type_references.len(),
            "Should have exactly one type reference"
        );

        assert!(root_scope.references.contains(&JsWord::from("foo")));
        assert!(root_scope.type_references.contains(&JsWord::from("Foo")));
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

        let visitor = parse_and_visit(source);
        let root_scope = &visitor.scopes[0];
        let inner_scope = &visitor.scopes[1];

        assert!(root_scope.references.is_empty());
        assert!(root_scope.type_references.is_empty());

        assert_eq!(2, inner_scope.references.len());
        assert!(inner_scope.type_references.is_empty());

        assert!(inner_scope.references.contains(&JsWord::from("foo")));
        assert!(inner_scope.references.contains(&JsWord::from("bar")));
    }

    struct TestScope {
        references: Vec<&'static str>,
        type_references: Vec<&'static str>,
        inner: Vec<TestScope>,
        bindings: Vec<&'static str>,
        type_bindings: Vec<&'static str>,
    }

    struct TestSpec {
        source: &'static str,
        exports: Vec<&'static str>,
        scope: TestScope,
    }

    fn run_test(spec: TestSpec) {
        let visitor = parse_and_visit(spec.source);

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

        let check_scope = |spec, test_scope, scope_id| {
            let scope = scopes_by_id.get(&scope_id).unwrap();
        };

        // let root_scope = &visitor.scopes[0];
        check_scope(&spec, &spec.scope, ScopeId::root());
    }
}
