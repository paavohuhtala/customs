use crate::tests::utils::{run_test, TestBinding, TestScope, TestSpec};

#[test]
pub fn smoke() {
    let source = r#"
        export const exportedConstant = {}
        export function exportedFunction() { }
        export type ExportedType = { }
        export interface ExportedInterface { }
    "#;

    let spec = TestSpec {
        source,
        exports: vec![],
        imports: vec![],
        scope: TestScope {
            bindings: vec![
                TestBinding::exported("exportedConstant"),
                TestBinding::exported("exportedFunction"),
            ],
            type_bindings: vec![
                TestBinding::exported("ExportedType"),
                TestBinding::exported("ExportedInterface"),
            ],
            inner: vec![
                TestScope::default(),
                TestScope::default(),
                TestScope::default(),
            ],
            ..Default::default()
        },
    };

    run_test(spec);
}

#[test]
pub fn inner_scope() {
    let source = r#"
        export const exportedFunction = function() {
            const notExported = 10
            function norThis<T>() { }
            const [a, b, c] = [1, 2, 3]
        }
    "#;

    let spec = TestSpec {
        source,
        exports: vec![],
        imports: vec![],
        scope: TestScope {
            bindings: vec![TestBinding::exported("exportedFunction")],
            inner: vec![TestScope {
                bindings: vec![
                    TestBinding::private("notExported"),
                    TestBinding::private("norThis"),
                    TestBinding::private("a"),
                    TestBinding::private("b"),
                    TestBinding::private("c"),
                ],
                inner: vec![TestScope {
                    type_bindings: vec![TestBinding::private("T")],
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
pub fn export_statement() {
    let source = r#"
        const a = 10
        type Foo = number
        export { a, Foo }
    "#;

    let spec = TestSpec {
        source,
        exports: vec!["a", "Foo"],
        imports: vec![],
        scope: TestScope {
            bindings: vec![TestBinding::private("a")],
            type_bindings: vec![TestBinding::private("Foo")],
            ambiguous_references: vec!["a", "Foo"],
            inner: vec![TestScope::default()],
            ..Default::default()
        },
    };

    run_test(spec);
}

#[test]
pub fn export_statement_default() {
    let source = r#"
        type Foo = { x: number }
        export { Foo as default }
    "#;

    let spec = TestSpec {
        source,
        exports: vec!["default"],
        imports: vec![],
        scope: TestScope {
            type_bindings: vec![TestBinding::private("Foo")],
            ambiguous_references: vec!["Foo"],
            inner: vec![TestScope::default()],
            ..Default::default()
        },
    };

    run_test(spec);
}

#[test]
pub fn re_export() {
    let source = r#"
        export { a, Foo } from "./a"
    "#;

    let spec = TestSpec {
        source,
        exports: vec!["a", "Foo"],
        imports: vec![("./a", vec![("a", None), ("Foo", None)])],
        scope: TestScope::default(),
    };

    run_test(spec);
}

#[test]
pub fn rename() {
    let source = r#"
        const a = "foo"
        export { a as b }
    "#;

    let spec = TestSpec {
        source,
        exports: vec!["b"],
        imports: vec![],
        scope: TestScope {
            bindings: vec![TestBinding::private("a")],
            ambiguous_references: vec!["a"],
            ..Default::default()
        },
    };

    run_test(spec);
}

#[test]
pub fn default_function() {
    let source = r#"
        export default function foo() { }
    "#;

    let spec = TestSpec {
        source,
        exports: vec![],
        imports: vec![],
        scope: TestScope {
            bindings: vec![TestBinding::default_exported("foo")],
            inner: vec![TestScope::default()],
            ..Default::default()
        },
    };

    run_test(spec);
}

#[test]
pub fn default_unnamed_function() {
    let source = r#"
        export default function() { }
    "#;

    let spec = TestSpec {
        source,
        exports: vec!["default"],
        imports: vec![],
        scope: TestScope {
            inner: vec![TestScope::default()],
            ..Default::default()
        },
    };

    run_test(spec);
}

#[test]
pub fn default_interface() {
    let source = r#"
        export default interface Foo { a: string, b: number }
    "#;

    let spec = TestSpec {
        source,
        exports: vec![],
        imports: vec![],
        scope: TestScope {
            type_bindings: vec![TestBinding::default_exported("Foo")],
            inner: vec![TestScope::default()],
            ..Default::default()
        },
    };

    run_test(spec);
}

#[test]
pub fn default_class() {
    let source = r#"
        export default class Foo { a: string = "a" }
    "#;

    let spec = TestSpec {
        source,
        exports: vec![],
        imports: vec![],
        scope: TestScope {
            bindings: vec![TestBinding::default_exported("Foo")],
            type_bindings: vec![TestBinding::default_exported("Foo")],
            inner: vec![TestScope::default()],
            ..Default::default()
        },
    };

    run_test(spec);
}

#[test]
pub fn default_unnamed_class() {
    let source = r#"
        export default class { a: string = "a" }
    "#;

    let spec = TestSpec {
        source,
        exports: vec!["default"],
        imports: vec![],
        scope: TestScope {
            inner: vec![TestScope::default()],
            ..Default::default()
        },
    };

    run_test(spec);
}

#[test]
pub fn default_statement_const() {
    let source = r#"
        const foo = "bar"
        export default foo
    "#;

    let spec = TestSpec {
        source,
        exports: vec!["default"],
        imports: vec![],
        scope: TestScope {
            bindings: vec![TestBinding::private("foo")],
            ambiguous_references: vec!["foo"],
            ..Default::default()
        },
    };

    run_test(spec);
}

#[test]
pub fn default_statement_interface() {
    let source = r#"
        interface Foo { x: number }
        export default Foo
    "#;

    let spec = TestSpec {
        source,
        exports: vec!["default"],
        imports: vec![],
        scope: TestScope {
            type_bindings: vec![TestBinding::private("Foo")],
            ambiguous_references: vec!["Foo"],
            inner: vec![TestScope::default()],
            ..Default::default()
        },
    };

    run_test(spec);
}

#[test]
pub fn default_statement_type() {
    let source = r#"
        type Foo = { x: number }
        export default Foo
    "#;

    let spec = TestSpec {
        source,
        exports: vec!["default"],
        imports: vec![],
        scope: TestScope {
            type_bindings: vec![TestBinding::private("Foo")],
            ambiguous_references: vec!["Foo"],
            inner: vec![TestScope::default()],
            ..Default::default()
        },
    };

    run_test(spec);
}

#[test]
pub fn destructured() {
    let source = r#"
        export const { x: { y } } = { x: { y: "hello" } }
    "#;

    let spec = TestSpec {
        source,
        exports: vec![],
        imports: vec![],
        scope: TestScope {
            bindings: vec![TestBinding::exported("y")],
            ..Default::default()
        },
    };

    run_test(spec);
}
