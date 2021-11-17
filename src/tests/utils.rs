use std::{
    borrow::Borrow,
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::Arc,
};

use crate::{
    dependency_graph::{
        normalize_module_path, ExportName, ImportName, Module, ModuleKind, ModulePath,
    },
    module_visitor::{BindingLike, ModuleVisitor, Scope, ScopeId, SelfExport},
    parsing::{analyze_module, module_from_source},
};

use anyhow::Context;
use pretty_assertions::assert_eq;
use swc_atoms::JsWord;
use swc_ecma_visit::Visit;

pub(crate) fn parse_and_visit(virtual_path: &'static str, source: &'static str) -> ModuleVisitor {
    let (source_map, module) = module_from_source(
        String::from(source),
        crate::dependency_graph::ModuleKind::TS,
    )
    .unwrap();

    // println!("{:#?}", module);

    let mut visitor = ModuleVisitor::new(PathBuf::from(virtual_path), source_map);
    visitor.visit_module(&module, &module);
    visitor
}

pub(crate) fn parse_and_analyze(virtual_path: &'static str, source: &'static str) -> Module {
    let module_kind = if virtual_path.ends_with(".d.ts") {
        ModuleKind::DTS
    } else if virtual_path.ends_with(".tsx") {
        ModuleKind::TSX
    } else {
        ModuleKind::TS
    };

    let visitor = parse_and_visit(virtual_path, source);

    let root = Arc::new(PathBuf::from(""));

    let virtual_path = PathBuf::from(virtual_path);
    let normalized = normalize_module_path(&root, &virtual_path).unwrap();

    let module = Module::new(
        ModulePath {
            root,
            root_relative: Arc::new(virtual_path),
            normalized,
        },
        module_kind,
    );

    analyze_module(module, visitor).unwrap()
}

pub struct TestBinding {
    pub(crate) name: &'static str,
    pub(crate) exported: SelfExport,
}

impl TestBinding {
    pub fn private(name: &'static str) -> Self {
        Self {
            name,
            exported: SelfExport::Private,
        }
    }

    pub fn exported(name: &'static str) -> Self {
        Self {
            name,
            exported: SelfExport::Named,
        }
    }

    pub fn default_exported(name: &'static str) -> Self {
        Self {
            name,
            exported: SelfExport::Default,
        }
    }
}

pub struct TestScope {
    pub(crate) references: Vec<&'static str>,
    pub(crate) type_references: Vec<&'static str>,
    pub(crate) ambiguous_references: Vec<&'static str>,
    pub(crate) bindings: Vec<TestBinding>,
    pub(crate) type_bindings: Vec<TestBinding>,
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
    let visitor = parse_and_visit("unknown.ts", spec.source);

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
            "Expected scope {} to contain {} {}, was {}",
            scope_id,
            expected.len(),
            kind_plural,
            was.len()
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

    fn assert_bindings_equal<B: BindingLike>(
        kind_singular: &'static str,
        kind_plural: &'static str,
        expected: &[TestBinding],
        was: &HashMap<JsWord, B>,
        scope_id: ScopeId,
    ) {
        assert_eq!(
            expected.len(),
            was.len(),
            "Expected scope {} to contain {} {}, was {}",
            scope_id,
            expected.len(),
            kind_plural,
            was.len(),
        );

        for expected_binding in expected {
            let as_atom = JsWord::from(expected_binding.name);
            let binding = was.get(&as_atom);

            match (expected_binding, binding) {
                (_, None) => {
                    panic!(
                        "Did not find {} {} in scope {}",
                        kind_singular, expected_binding.name, scope_id
                    );
                }
                (expected_binding, Some(binding)) => {
                    assert_eq!(
                        expected_binding.exported,
                        binding.export(),
                        "Expected {} {} to have export state {:?}, was {:?}",
                        kind_singular,
                        expected_binding.name,
                        expected_binding.exported,
                        binding.export()
                    );
                }
            }
        }
    }

    fn check_scope(test_scope: &TestScope, scope: &Scope, scopes: &[Scope]) {
        assert_bindings_equal(
            "binding",
            "bindings",
            &test_scope.bindings,
            &scope.bindings,
            scope.id,
        );
        assert_bindings_equal(
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
