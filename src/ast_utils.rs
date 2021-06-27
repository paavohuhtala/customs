use swc_ecma_ast::{Ident, TsEntityName, TsQualifiedName};

pub fn walk_ts_qualified_name(qualified_name: &TsQualifiedName) -> &Ident {
    match &qualified_name.left {
        TsEntityName::TsQualifiedName(name) => walk_ts_qualified_name(&name),
        TsEntityName::Ident(ident) => ident,
    }
}
