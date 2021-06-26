use crate::tests::utils::{run_test, TestScope, TestSpec};

#[test]
pub fn named() {
    let source = r#"
        import { a, b } from "./foo" 
    "#;

    let spec = TestSpec {
        source,
        exports: vec![],
        imports: vec![("./foo", vec![("a", Some("a")), ("b", Some("b"))])],
        scope: TestScope::default(),
    };

    run_test(spec);
}

#[test]
pub fn named_as() {
    let source = r#"
        import { a as b } from "./foo" 
    "#;

    let spec = TestSpec {
        source,
        exports: vec![],
        imports: vec![("./foo", vec![("a", Some("b"))])],
        scope: TestScope::default(),
    };

    run_test(spec);
}

#[test]
pub fn default() {
    let source = r#"
        import defaultExport from "./foo" 
    "#;

    let spec = TestSpec {
        source,
        exports: vec![],
        imports: vec![("./foo", vec![("default", Some("defaultExport"))])],
        scope: TestScope::default(),
    };

    run_test(spec);
}

#[test]
pub fn default_as() {
    let source = r#"
        import { default as renamedDefault } from "./foo" 
    "#;

    let spec = TestSpec {
        source,
        exports: vec![],
        imports: vec![("./foo", vec![("default", Some("renamedDefault"))])],
        scope: TestScope::default(),
    };

    run_test(spec);
}

#[test]
pub fn wildcard_as() {
    let source = r#"
        import * as wildcard from "./foo" 
    "#;

    let spec = TestSpec {
        source,
        exports: vec![],
        imports: vec![("./foo", vec![("*", Some("wildcard"))])],
        scope: TestScope::default(),
    };

    run_test(spec);
}

#[test]
pub fn multiple_default_wildcard() {
    let source = r#"
        import defaultExport, * as wildcard from "./foo" 
    "#;

    let spec = TestSpec {
        source,
        exports: vec![],
        imports: vec![(
            "./foo",
            vec![("default", Some("defaultExport")), ("*", Some("wildcard"))],
        )],
        scope: TestScope::default(),
    };

    run_test(spec);
}

#[test]
pub fn multiple_default_named() {
    let source = r#"
        import defaultExport, { a, b as c } from "./foo" 
    "#;

    let spec = TestSpec {
        source,
        exports: vec![],
        imports: vec![(
            "./foo",
            vec![
                ("default", Some("defaultExport")),
                ("a", Some("a")),
                ("b", Some("c")),
            ],
        )],
        scope: TestScope::default(),
    };

    run_test(spec);
}
