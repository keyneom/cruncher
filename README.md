# Cruncher

JIT-compiled mathematical expression calculator for Rust.

This library takes a string such as `"x + 2 / 3 * ( z ^ 4 )"`, parses it, and
evaluates it with a dictionary of variable values. Expressions can be evaluated
through an AST interpreter or compiled with Cranelift JIT for bulk evaluation.

## Example

```rust
use hashbrown::HashMap;
use cruncher::eval;

assert_eq!(eval("2+2", &HashMap::new()), Ok(4.0));
assert_eq!(eval("sin(0)", &HashMap::new()), Ok(0.0));

let mut ctx = HashMap::new();
ctx.insert("x".into(), 3.0);
assert_eq!(eval("x * 4", &ctx), Ok(12.0));
```

See the crate docs and tests for more usage, including [`Expr`] reuse and the
[`JIT`] compiler.

## Status

Work in progress / experimental. The lexer accepts expressions with or without
spaces around operators (`2+2` and `2 + 2`).

Repository: <https://github.com/keyneom/cruncher>

## Roadmap

See [docs/ROADMAP.md](docs/ROADMAP.md) for the path from the current expression
evaluator toward curves, distributions, and Monte Carlo / scenario analysis.

