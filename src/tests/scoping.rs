use crate::tests::utils::{run_test, TestScope, TestSpec};

#[test]
pub fn block() {
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
                inner: vec![TestScope::default()],
                ..Default::default()
            }],
            ..Default::default()
        },
    };

    run_test(spec);
}

#[test]
pub fn function() {
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
pub fn function_generics() {
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
pub fn interface_generics() {
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
pub fn type_generics() {
    let source = r#"
        type Foo<T> = { x: T }
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