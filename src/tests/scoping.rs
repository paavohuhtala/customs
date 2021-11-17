use crate::tests::utils::{run_test, TestBinding, TestScope, TestSpec};

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
        imports: vec![],
        scope: TestScope {
            bindings: vec![TestBinding::private("foo")],
            inner: vec![TestScope {
                bindings: vec![TestBinding::private("foo")],
                type_bindings: vec![TestBinding::private("Bar")],
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
        imports: vec![],
        scope: TestScope {
            bindings: vec![TestBinding::private("outerFunction")],
            inner: vec![TestScope {
                bindings: vec![TestBinding::private("innerFunction")],
                inner: vec![TestScope::default()],
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
        const outerFunction = () =>
        {
            const innerFunction = () => { }
        }
    "#;

    let spec = TestSpec {
        source,
        exports: vec![],
        imports: vec![],
        scope: TestScope {
            bindings: vec![TestBinding::private("outerFunction")],
            inner: vec![TestScope {
                bindings: vec![TestBinding::private("innerFunction")],
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
        imports: vec![],
        scope: TestScope {
            bindings: vec![TestBinding::private("f")],
            inner: vec![TestScope {
                type_bindings: vec![TestBinding::private("T")],
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
        imports: vec![],
        scope: TestScope {
            type_bindings: vec![TestBinding::private("Foo")],
            inner: vec![TestScope {
                type_bindings: vec![TestBinding::private("T")],
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
        imports: vec![],
        scope: TestScope {
            type_bindings: vec![TestBinding::private("Foo")],
            inner: vec![TestScope {
                type_bindings: vec![TestBinding::private("T")],
                type_references: vec!["T"],
                ..Default::default()
            }],
            ..Default::default()
        },
    };

    run_test(spec);
}

#[test]
pub fn class() {
    let source = r#"
        class Foo {
            x: number

            constructor(x: number) {
                this.x = x;
            }

            getX(): number {
                return this.x;
            }
        }
    "#;

    let spec = TestSpec {
        source,
        exports: vec![],
        imports: vec![],
        scope: TestScope {
            bindings: vec![TestBinding::private("Foo")],
            type_bindings: vec![TestBinding::private("Foo")],
            inner: vec![TestScope {
                inner: vec![
                    TestScope {
                        bindings: vec![TestBinding::private("x")],
                        references: vec!["x"],
                        ..Default::default()
                    },
                    TestScope::default(),
                ],
                ..Default::default()
            }],
            ..Default::default()
        },
    };

    run_test(spec);
}

#[test]
pub fn ts_enum() {
    let source = r#"
        enum Foo { A, B, C = getBar() }
    "#;

    let spec = TestSpec {
        source,
        exports: vec![],
        imports: vec![],
        scope: TestScope {
            bindings: vec![TestBinding::private("Foo")],
            type_bindings: vec![TestBinding::private("Foo")],
            inner: vec![TestScope {
                references: vec!["getBar"],
                ..Default::default()
            }],

            ..Default::default()
        },
    };

    run_test(spec);
}

#[test]
pub fn loops_in_closures() {
    let source = r#"
        it("does thing", () => {
            const x = "shadowed"
            for (const x of l1) { }
        })

        it("does something else", () => {
            const x = "shadowed"
            for (const x of l2) { }
        })
    "#;

    let spec = TestSpec {
        source,
        exports: vec![],
        imports: vec![],
        scope: TestScope {
            references: vec!["it"],
            inner: vec![
                TestScope {
                    bindings: vec![TestBinding::private("x")],
                    inner: vec![TestScope {
                        references: vec!["l1"],
                        bindings: vec![TestBinding::private("x")],
                        inner: vec![TestScope::default()],
                        ..Default::default()
                    }],
                    ..Default::default()
                },
                TestScope {
                    bindings: vec![TestBinding::private("x")],
                    inner: vec![TestScope {
                        references: vec!["l2"],
                        bindings: vec![TestBinding::private("x")],
                        inner: vec![TestScope::default()],
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
pub fn ts_overloads() {
    let source = r#"
        export function foo()
        export function foo(x: string)
        export function foo(x?: any) { }
    "#;

    let spec = TestSpec {
        source,
        exports: vec![],
        imports: vec![],
        scope: TestScope {
            bindings: vec![TestBinding::exported("foo")],
            inner: vec![
                TestScope::default(),
                TestScope {
                    bindings: vec![TestBinding::private("x")],
                    ..Default::default()
                },
                TestScope {
                    bindings: vec![TestBinding::private("x")],
                    ..Default::default()
                },
            ],

            ..Default::default()
        },
    };

    run_test(spec);
}

#[test]
pub fn ts_declare_module() {
    let source = r#"
        declare module "*.svg" {
            import { SvgProps } from "react-native-svg";
            const content: React.FC<SvgProps>;
            export default content;
        }
        
        declare module "*.png" {
            import { ImageSourcePropType } from "react-native";
            const content: ImageSourcePropType;
            export default content;
        }
    "#;

    // TODO: This misses the reference to React.FC
    let spec = TestSpec {
        source,
        exports: vec![],
        imports: vec![
            ("react-native-svg", vec![("SvgProps", Some("SvgProps"))]),
            (
                "react-native",
                vec![("ImageSourcePropType", Some("ImageSourcePropType"))],
            ),
        ],
        scope: TestScope {
            bindings: vec![],
            inner: vec![
                TestScope {
                    bindings: vec![TestBinding::private("content")],
                    type_references: vec!["SvgProps"],
                    ambiguous_references: vec!["content"],
                    ..Default::default()
                },
                TestScope {
                    bindings: vec![TestBinding::private("content")],
                    type_references: vec!["ImageSourcePropType"],
                    ambiguous_references: vec!["content"],
                    ..Default::default()
                },
            ],

            ..Default::default()
        },
    };

    run_test(spec);
}
