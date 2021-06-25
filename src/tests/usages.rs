use crate::tests::utils::{run_test, TestScope, TestSpec};

#[test]
pub fn typeof_uses_variable() {
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
pub fn path() {
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
