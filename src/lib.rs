// Note: rustc 1.95 can ICE rendering diagnostics when terminal width <= 10.
// `.cargo/config.toml` sets `--diagnostic-width=120` as a workaround
// (https://github.com/rust-lang/rust/issues/154258).
// Avoid lint attributes that interact poorly with that ICE path.
#![allow(dead_code)]

//! Cruncher, a crate for dynamic evaluation of mathematical expressions.
//!
//! This crate provide run-time evaluation of mathematical expressions,
//! embedded in strings. The easiest way to use this crate is with the
//! [`eval`](fn.eval.html) function:
//!
//! ```
//! use hashbrown::HashMap;
//! assert_eq!(cruncher::eval("3 + 5 * 2", &HashMap::new()), Ok(13.0));
//! ```
//!
//! The second argument to `eval` is a [`HashMap`](struct.HashMap.html), that
//! can define variables:
//!
//! ```
//! use hashbrown::HashMap;
//!
//! let mut context :HashMap<String,f64> = HashMap::new();
//! context.insert("a".into(), 3.5);
//! assert_eq!(cruncher::eval("2 * a", &context), Ok(7.0));
//! ```
//!
//! It is also possible to separate the parsing from the evaluation of an
//! expression with the [`Expr`](struct.Expr.html) type. This allow to reuse
//! the same expression with different values for variables.
//!
//! ```
//! use hashbrown::HashMap;
//! use cruncher::{Expr};
//!
//! let expr = Expr::parse("3 + 5 * 2").unwrap();
//! assert_eq!(expr.eval(&HashMap::new()), Ok(13.0));
//!
//! let expr = Expr::parse("3 / c + b").unwrap();
//! let mut context :HashMap<String,f64> = HashMap::new();
//! context.insert("c".into(), 1.0);
//! context.insert("b".into(), 5.0);
//! assert_eq!(expr.eval(&context), Ok(8.0));
//!
//! context.insert("b".into(), 10.0);
//! assert_eq!(expr.eval(&context), Ok(13.0));
//! ```
//!
//! For bulk / Monte Carlo evaluation, prefer [`Expr::eval_batch`] or the
//! JIT-backed [`CompiledExpr`].
//!
//! # Language definition
//!
//! The language implemented by cruncher can contain the following elements:
//!
//! - float literal values: `-12.456`, `+0.0045e78`, ...;
//! - left and right parenthesis;
//! - mathematical operators: `+` for addition, `-` for subtraction,
//!   `*` for multiplication, `/` for division and `^` for exponentiation
//!   (`std::f64::powf`);
//! - variables. Variables names are ASCII only, and can start by a letter or
//!   `_`, and can contain letters, digits, `.`, `_`, `[` or `]`.
//! - function call: `sin(a)`, `atan(22.0)`. The following function are
//!   accessible, with the same meaning as the corresponding `std::f64`
//!   function: `sqrt`, `cbrt`, `sin`, `cos`, `tan`, `asin`, `acos`, `atan`,
//!   `sinh`, `cosh`, `tanh`, `asinh`, `acosh`, `atanh`, `floor`, `ceil`,
//!   `abs`, `exp`, `ln`, `log2`, `log10`.
//!
//! Any other symbol is forbidden in the input.
//!
//! The mathematical operators obey the usual relations of associativity and
//! precedence, but still carry the floating point properties: addition is not
//! commutative, `NaN` and infinities exist, ...
//!
//! # Technical details
//!
//! cruncher uses a Pratt parser into an AST, with constant folding, plus an
//! optional Cranelift JIT for bulk evaluation. It works with `f64` data.

mod ast;
mod error;
mod expr;
mod jit;
mod lexer;
mod parser;
mod token;
mod util;
mod value;

pub use error::{Error, Span};
pub use expr::{eval, CompiledExpr, Expr};
pub use jit::JIT;
pub use lexer::is_variable;
pub use value::{theme_park_revenue_columns, Curve, Dist, Distribution, Lcg64, RngCore, Value};

#[cfg(test)]
mod golden_tests;

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
