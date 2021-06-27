# Glossary in `customs`

## Module

A module is TypeScript source file which contains at least one import or export statement.

## Scope

Scopes are regions of a module which control the visibility of bindings.

The scopes of a module form a [tree](<https://en.wikipedia.org/wiki/Tree_(graph_theory)>). The root node is known as a _root scope_. Each node can have 0 to n _child scopes_, and every node (with the exception of the root scope) has a _parent scope_.

Scopes often, but not always, correspond to areas separated by curly brackets (`{ }`) in a TypeScript program.

## Binding

Bindings are named declarations within a scope.

- Function and constant declarations as well as function and constructor parameters are known as _value bindings_ (usually just _bindings_).
- Interface and type alias declarations as well as type parameters are considered _type bindings_.
- Class and enum declarations are considered both value and type bindings.

Each binding of a kind must have a unique name in its scope. In other words, it is permitted to have a value binding `Foo` and a type binding `Foo` in the same scope, but it is not allowed to have two value bindings called `Bar` in the same scope.

When a binding is declared in a scope it is allowed to declare a binding of the same kind and name in any its child scopes (recursively). This is known as _shadowing_. A binding which shadows an another binding is known as a _shadowing binding_.

## Reference

Bindings in any accessible scope (current or any of the parent scopes) can be _referenced_ by the name of the binding.

- References to value bindings are known as _value references_ (usually just _references_).
  - Examples: assigning to a variable, calling a function, getting the type of variable with TypeScript's `typeof` operator.
- References to type bindings are known as _type references_.
  - Examples: using a type in a type annotation, using a type as constraint of a type parameter, extending an interface.
- Sometimes the kind of a reference cannot be determined by syntactic analysis alone. These are known as _ambiguous references_.
  - Examples: the export statement (`export { foo }`).

A reference implies a dependency relationship with a binding. From a dead code elimination POV, an unexported binding with zero references is considered _unused_ and can therefore be safely removed. Correspondingly, a binding with at least one reference is considered _locally used_.

## Export

Each module can have 0 to n _named exports_, and 0 or 1 _default exports_<sup>1</sup>. Exports can, but don't have to, correspond to bindings in the root scope. Both types and values can be exported. Named exports of a kind have to be unique, but it is allowed to export a type and a value of the same name.

Exporting a binding **does not** count as a reference. An export only makes it possible for other modules to refer to the binding, but it does not imply the identifier is used.

If an export is imported in an another module, the export is considered _externally used_.

<sup>1</sup> A module **can** have both a type and a value default export, but both must have corresponding a binding of the same name and they must he exported with a single `export default` statement. This is also the case when default exporting a class or an enum.

## Import

Each module can have 0 to n _import declarations_, which bring items from other modules into the root scope. There are several kinds of import declarations:

- Named import declaration brings all listed named bindings into the root scope.
  - `import { a, b } from './foo'`)
- Default imports brings the default export of the module into a new binding.
  - `import a from './foo'`
- Wildcard import brings all exports of the module into members of a new binding.
  - `import * as all from './foo'`
