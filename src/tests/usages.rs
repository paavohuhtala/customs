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
        imports: vec![],
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
        imports: vec![],
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
        imports: vec![],
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
        imports: vec![],
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
        imports: vec![],
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
        imports: vec![],
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
        imports: vec![],
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
        imports: vec![],
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
        imports: vec![],
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
        function f() { return f() }
    "#;

    let spec = TestSpec {
        source,
        exports: vec![],
        imports: vec![],
        scope: TestScope {
            bindings: vec!["f"],
            inner: vec![TestScope {
                references: vec!["f"],
                ..Default::default()
            }],
            ..Default::default()
        },
    };

    run_test(spec);
}

#[test]
pub fn arrow_function() {
    let source = r#"
        type Foo = string
        const f = (x: Foo) => { }
    "#;

    let spec = TestSpec {
        source,
        exports: vec![],
        imports: vec![],
        scope: TestScope {
            bindings: vec!["f"],
            type_bindings: vec!["Foo"],
            inner: vec![
                TestScope::default(),
                TestScope {
                    bindings: vec!["x"],
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
pub fn mapped_type() {
    let source = r#"
        type Key = "a" | "b"
        type Foo = {
            [k in Key]: number;
        }
    "#;

    let spec = TestSpec {
        source,
        exports: vec![],
        imports: vec![],
        scope: TestScope {
            type_bindings: vec!["Key", "Foo"],
            inner: vec![
                TestScope::default(),
                TestScope {
                    inner: vec![TestScope {
                        type_bindings: vec!["k"],
                        type_references: vec!["Key"],
                        ..Default::default()
                    }],
                    ..Default::default()
                },
            ],
            ..Default::default()
        },
    };

    run_test(spec);
}

#[test]
pub fn indexed_type() {
    let source = r#"
        type Foo = {
            [key: string]: number;
        }
    "#;

    let spec = TestSpec {
        source,
        exports: vec![],
        imports: vec![],
        scope: TestScope {
            type_bindings: vec!["Foo"],
            inner: vec![TestScope {
                inner: vec![TestScope {
                    bindings: vec!["key"],
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
pub fn union_type() {
    let source = r#"
        type Foo = "foo"
        type Bar = "bar"
        type FooOrBar = Foo | Bar
    "#;

    let spec = TestSpec {
        source,
        exports: vec![],
        imports: vec![],
        scope: TestScope {
            type_bindings: vec!["Foo", "Bar", "FooOrBar"],
            inner: vec![
                TestScope::default(),
                TestScope::default(),
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
pub fn intersection_type() {
    let source = r#"
        type Foo = { a: string }
        type Bar = { b: number }
        type FooAndBar = Foo & Bar
    "#;

    let spec = TestSpec {
        source,
        exports: vec![],
        imports: vec![],
        scope: TestScope {
            type_bindings: vec!["Foo", "Bar", "FooAndBar"],
            inner: vec![
                TestScope::default(),
                TestScope::default(),
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
pub fn recursive_type() {
    let source = r#"
        type Foo = Foo[]
    "#;

    let spec = TestSpec {
        source,
        exports: vec![],
        imports: vec![],
        scope: TestScope {
            type_bindings: vec!["Foo"],
            inner: vec![TestScope {
                type_references: vec!["Foo"],
                ..Default::default()
            }],
            ..Default::default()
        },
    };

    run_test(spec);
}

#[test]
pub fn conditional() {
    let source = r#"
        type ElementOf<A> = A extends Array<infer E> ? E : never
    "#;

    let spec = TestSpec {
        source,
        exports: vec![],
        imports: vec![],
        scope: TestScope {
            type_bindings: vec!["ElementOf"],
            inner: vec![TestScope {
                type_bindings: vec!["A"],
                inner: vec![TestScope {
                    type_references: vec!["A", "Array"],
                    type_bindings: vec!["E"],
                    inner: vec![
                        TestScope {
                            type_references: vec!["E"],
                            ..TestScope::default()
                        },
                        TestScope::default(),
                    ],
                    ..TestScope::default()
                }],
                ..Default::default()
            }],
            ..Default::default()
        },
    };

    run_test(spec);
}

#[test]
pub fn type_annotation_basic() {
    let source = r#"
        type Foo = string
        const x: Foo = "bar"
    "#;

    let spec = TestSpec {
        source,
        exports: vec![],
        imports: vec![],
        scope: TestScope {
            type_bindings: vec!["Foo"],
            bindings: vec!["x"],
            type_references: vec!["Foo"],
            inner: vec![TestScope::default()],
            ..Default::default()
        },
    };

    run_test(spec);
}

#[test]
pub fn type_annotation_mapped_type() {
    let source = r#"
        type Key = "a" | "b"
        const x: { [k in Key]: number } = { a: 10, b: 20 }
    "#;

    let spec = TestSpec {
        source,
        exports: vec![],
        imports: vec![],
        scope: TestScope {
            type_bindings: vec!["Key"],
            bindings: vec!["x"],
            inner: vec![
                TestScope::default(),
                TestScope {
                    type_references: vec!["Key"],
                    type_bindings: vec!["k"],
                    ..Default::default()
                },
            ],
            ..Default::default()
        },
    };

    run_test(spec);
}

#[test]
pub fn as_cast() {
    let source = r#"
        type Foo = string
        const x = "bar" as Foo
    "#;

    let spec = TestSpec {
        source,
        exports: vec![],
        imports: vec![],
        scope: TestScope {
            type_bindings: vec!["Foo"],
            bindings: vec!["x"],
            type_references: vec!["Foo"],
            inner: vec![TestScope::default()],
            ..Default::default()
        },
    };

    run_test(spec);
}

#[test]
pub fn as_cast_object_literal() {
    let source = r#"
        type Foo = string
        const x = { a: 10 } as { a: number, b?: Foo }
    "#;

    let spec = TestSpec {
        source,
        exports: vec![],
        imports: vec![],
        scope: TestScope {
            type_bindings: vec!["Foo"],
            bindings: vec!["x"],
            type_references: vec!["Foo"],
            inner: vec![TestScope::default()],
            ..Default::default()
        },
    };

    run_test(spec);
}

#[test]
pub fn as_cast_mapped_type_literal() {
    let source = r#"
        type Key = "a" | "b"
        const x = { a: 10 } as { [k in Key]?: number }
    "#;

    let spec = TestSpec {
        source,
        exports: vec![],
        imports: vec![],
        scope: TestScope {
            type_bindings: vec!["Key"],
            bindings: vec!["x"],
            inner: vec![
                TestScope::default(),
                TestScope {
                    type_bindings: vec!["k"],
                    type_references: vec!["Key"],
                    ..Default::default()
                },
            ],
            ..Default::default()
        },
    };

    run_test(spec);
}
