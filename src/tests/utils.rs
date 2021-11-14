use std::{
    borrow::Borrow,
    collections::{HashMap, HashSet},
};

use crate::{
    dependency_graph::{ExportName, ImportName},
    module_visitor::{ModuleVisitor, Scope, ScopeId},
    parsing::module_from_source,
};

use anyhow::Context;
use pretty_assertions::assert_eq;
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

use std::cmp::Eq;
use std::hash::Hash;

trait SetLike<T> {
    fn contains<Q>(&self, key: &Q) -> bool
    where
        T: Borrow<Q>,
        Q: Hash + Eq;

    fn len(&self) -> usize;
}

impl<K: Hash + Eq, V> SetLike<K> for HashMap<K, V> {
    fn contains<Q>(&self, key: &Q) -> bool
    where
        K: Borrow<Q>,
        Q: Hash + Eq,
    {
        self.contains_key(key)
    }

    fn len(&self) -> usize {
        HashMap::len(self)
    }
}

impl<K: Hash + Eq> SetLike<K> for HashSet<K> {
    fn contains<Q>(&self, key: &Q) -> bool
    where
        K: Borrow<Q>,
        Q: Hash + Eq,
    {
        HashSet::contains(self, key)
    }

    fn len(&self) -> usize {
        HashSet::len(self)
    }
}

pub struct TestSpec {
    pub(crate) source: &'static str,
    pub(crate) exports: Vec<&'static str>,
    pub(crate) imports: Vec<(&'static str, Vec<(&'static str, Option<&'static str>)>)>,
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

    assert_eq!(
        spec.imports.len(),
        visitor.imports.len(),
        "Expected import source counts to match"
    );

    for (source, imports) in &spec.imports {
        let imports_from_source = visitor
            .imports
            .get(*source)
            .with_context(|| format!("Expected import map to contain module {}", source))
            .unwrap();

        let imports_by_name = imports_from_source
            .iter()
            .map(|import| (import.imported_name.clone(), import))
            .collect::<HashMap<_, _>>();

        assert_eq!(
            imports.len(),
            imports_from_source.len(),
            "Expected import from {} to contain {} items",
            source,
            imports.len()
        );

        for &(expected_symbol, expected_local_name) in imports {
            let expected_import_name = match expected_symbol {
                "default" => ImportName::Default,
                "*" => ImportName::Wildcard,
                otherwise => ImportName::Named(JsWord::from(otherwise)),
            };

            let expected_local_name = expected_local_name.map(JsWord::from);

            let imported_symbol = imports_by_name
                .get(&expected_import_name)
                .with_context(|| {
                    format!(
                        "Expected imports from {} to contain {}",
                        source, expected_symbol,
                    )
                })
                .unwrap();

            assert_eq!(
                expected_local_name, imported_symbol.local_binding,
                "Expected local binding names to match"
            );
        }
    }

    fn assert_vec_set_equal(
        kind_singular: &'static str,
        kind_plural: &'static str,
        expected: &[&'static str],
        was: &impl SetLike<JsWord>,
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

    fn check_scope(test_scope: &TestScope, scope: &Scope, scopes: &[Scope]) {
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

        let child_scopes = scope
            .children
            .iter()
            .map(|&id| &scopes[id.index()])
            .collect::<Vec<_>>();

        assert_eq!(
            test_scope.inner.len(),
            child_scopes.len(),
            "Expected scope {} to have {} child scopes",
            scope.id,
            test_scope.inner.len()
        );

        for (scope, test_scope) in child_scopes.iter().zip(test_scope.inner.iter()) {
            check_scope(test_scope, scope, scopes);
        }
    }

    let root_scope = &visitor.scopes[0];
    check_scope(&spec.scope, root_scope, &visitor.scopes);
}
