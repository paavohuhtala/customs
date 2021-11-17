use crate::tests::utils::{run_test, TestBinding, TestScope, TestSpec};

#[test]
pub fn ts_type() {
    let source = r#"type Foo = { bar: string }"#;

    let spec = TestSpec {
        source,
        exports: vec![],
        imports: vec![],
        scope: TestScope {
            type_bindings: vec![TestBinding::private("Foo")],
            inner: vec![TestScope::default()],
            ..Default::default()
        },
    };

    run_test(spec);
}

#[test]
pub fn ts_interface() {
    let source = r#"interface Foo { bar: string }"#;

    let spec = TestSpec {
        source,
        exports: vec![],
        imports: vec![],
        scope: TestScope {
            type_bindings: vec![TestBinding::private("Foo")],
            inner: vec![TestScope::default()],

            ..Default::default()
        },
    };

    run_test(spec);
}

#[test]
pub fn type_and_value_of_same_name() {
    let source = r#"
        interface Foo { bar: number }
        const Foo = 123
    "#;

    let spec = TestSpec {
        source,
        exports: vec![],
        imports: vec![],
        scope: TestScope {
            bindings: vec![TestBinding::private("Foo")],
            type_bindings: vec![TestBinding::private("Foo")],
            inner: vec![TestScope::default()],
            ..Default::default()
        },
    };

    run_test(spec);
}
