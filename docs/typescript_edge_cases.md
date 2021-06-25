# Interesting TypeScript edge cases

## Inconsistent return type scoping

### Background

A function's parameters and type parameters are in the same scope as body of the function. This can be demonstrated by trying to declare a variable or a type with the same name as any parameter or type parameter, respectively.

```typescript
function foo(x: string) {
  const x = "aa";
}
```

The snippet results in the following compilation error:

```
Duplicate identifier 'x'.
```

Shadowing the argument in an inner scope is acceptable.

```typescript
function foo(x: string) {
  {
    const x = "aa";
  }
}
```

Normally TypeScript disallows referring to a variable before it has been declared and initialised, as expected. However this limitation does not apply to type declarations (`type` and `interface`), because unlike variables they don't need to be initialised and accessed in a particular order.

Types can also be derived from the (inferred) types of variables with the `typeof` operator, and in this case the order doesn't matter because the type of variable is known at compile time before actual run time initialisation.

```typescript
// this
type AlwaysConstant = typeof CONSTANT;
const CONSTANT = "constant";

// and this
const CONSTANT = "constant";
type AlwaysConstant = typeof CONSTANT;

// are accepted and equivalent
```

### The bug(?)

Given these statements, it makes sense that return type annotations can refer to any variable defined in the top-level scope using the `typeof` type-level operator:

```typescript
function foo(): typeof CONSTANT {
  const CONSTANT = "constant";
  return CONSTANT;
}
```

And this is accepted by the compiler without issue.

Since the return type can depend on a variable defined inside the function, it would (IMHO) be natural for the compiler to also allow using types defined in the root scope of the function:

```typescript
function foo(): AlwaysConstant {
  type AlwaysConstant = "constant";
  return "constant";
}
```

But this doesn't work - it causes the following compilation error:

```
Cannot find name 'AlwaysConstant'
```

On one hand, I can accept that type is considered an internal implementation detail and therefore it should not be allowed to leak outside the function body. On the other hand, leaking implementation details is already allowed using `typeof`, and the error message implies `AlwaysConstant` is not in scope, even though (in my mental model) it is in a scope the return type annotation can already refer to.

This is not an issue in practise for most users, but because of this discrepancy it seems that a simple (and dare I say, obvious) hierarchical block scoping model cannot be used with type-level TypeScript.
