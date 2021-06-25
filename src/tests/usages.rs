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
            inner: vec![
                TestScope {
                    references: vec!["foo"],
                    ..Default::default()
                },
                TestScope {
                    type_references: vec!["Foo"],
                    ..Default::default()
                },
            ],
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
                references: vec!["foo"],
                inner: vec![TestScope {
                    references: vec!["bar"],
                    ..Default::default()
                }],
                ..TestScope::default()
            }],
            ..TestScope::default()
        },
    };

    run_test(spec);
}

#[test]
pub fn type_array() {
    let source = r#"
        type Foo = number
        type FooArray = Foo[]
    "#;

    let spec = TestSpec {
        source,
        exports: vec![],
        scope: TestScope {
            type_bindings: vec!["Foo", "FooArray"],
            inner: vec![
                TestScope::default(),
                TestScope {
                    type_references: vec!["Foo"],
                    ..Default::default()
                },
            ],
            ..Default::default()
        },
    };

    run_test(spec);
}

#[test]
pub fn type_parametrised() {
    let source = r#"
        type Bar = string
        type Foo<T> = T
        type FooOfBar = Foo<Bar>
    "#;

    let spec = TestSpec {
        source,
        exports: vec![],
        scope: TestScope {
            type_bindings: vec!["Bar", "Foo", "FooOfBar"],
            inner: vec![
                TestScope::default(),
                TestScope {
                    type_bindings: vec!["T"],
                    type_references: vec!["T"],
                    ..Default::default()
                },
                TestScope {
                    type_references: vec!["Foo", "Bar"],
                    ..Default::default()
                },
            ],
            ..Default::default()
        },
    };

    run_test(spec);
}

#[test]
pub fn interface_extends() {
    let source = r#"
        interface Foo { a: string }
        interface Bar extends Foo { b: number }
    "#;

    let spec = TestSpec {
        source,
        exports: vec![],
        scope: TestScope {
            type_bindings: vec!["Foo", "Bar"],
            inner: vec![
                TestScope::default(),
                TestScope {
                    type_references: vec!["Foo"],
                    ..Default::default()
                },
            ],
            ..Default::default()
        },
    };

    run_test(spec);
}

#[test]
pub fn interface_extends_generics() {
    let source = r#"
        interface Foo<T> { a: Array<T> }
        interface Bar<T> extends Foo<T> { }
    "#;

    let spec = TestSpec {
        source,
        exports: vec![],
        scope: TestScope {
            type_bindings: vec!["Foo", "Bar"],
            inner: vec![
                TestScope {
                    type_bindings: vec!["T"],
                    type_references: vec!["Array", "T"],
                    ..Default::default()
                },
                TestScope {
                    type_bindings: vec!["T"],
                    type_references: vec!["Foo", "T"],
                    ..Default::default()
                },
            ],
            ..Default::default()
        },
    };

    run_test(spec);
}

#[test]
pub fn type_generics_constrint() {
    let source = r#"
        type Foo = number
        type Bar<T extends Foo> = { a: T[] }
    "#;

    let spec = TestSpec {
        source,
        exports: vec![],
        scope: TestScope {
            type_bindings: vec!["Foo", "Bar"],
            inner: vec![
                TestScope::default(),
                TestScope {
                    type_bindings: vec!["T"],
                    type_references: vec!["Foo", "T"],
                    ..Default::default()
                },
            ],
            ..Default::default()
        },
    };

    run_test(spec);
}

#[test]
pub fn interface_generics_constrint() {
    let source = r#"
        type Foo = number
        interface Bar<T extends Foo> { a: T[] }
    "#;

    let spec = TestSpec {
        source,
        exports: vec![],
        scope: TestScope {
            type_bindings: vec!["Foo", "Bar"],
            inner: vec![
                TestScope::default(),
                TestScope {
                    type_bindings: vec!["T"],
                    type_references: vec!["Foo", "T"],
                    ..Default::default()
                },
            ],
            ..Default::default()
        },
    };

    run_test(spec);
}

#[test]
pub fn function_initial() {
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
pub fn function_self_reference() {
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
