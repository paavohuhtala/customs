use std::collections::{HashMap, HashSet};

use crate::{
    dependency_graph::ExportName,
    module_visitor::{ModuleVisitor, Scope, ScopeId},
    parsing::module_from_source,
};

use itertools::Itertools;
use swc_atoms::JsWord;
use swc_ecma_visit::Visit;

pub fn parse_and_visit(source: &'static str) -> ModuleVisitor {
    let (_, module) = module_from_source(
        String::from(source),
        crate::dependency_graph::ModuleKind::TS,
    )
    .unwrap();

    let mut visitor = ModuleVisitor::new();
    visitor.visit_module(&module, &module);
    visitor
}

pub struct TestScope {
    pub(crate) references: Vec<&'static str>,
    pub(crate) type_references: Vec<&'static str>,
    pub(crate) ambiguous_references: Vec<&'static str>,
    pub(crate) bindings: Vec<&'static str>,
    pub(crate) type_bindings: Vec<&'static str>,
    pub(crate) inner: Vec<TestScope>,
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

pub struct TestSpec {
    pub(crate) source: &'static str,
    pub(crate) exports: Vec<&'static str>,
    pub(crate) scope: TestScope,
}

pub fn run_test(spec: TestSpec) {
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
