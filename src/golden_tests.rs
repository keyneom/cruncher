//! Golden / table-driven evaluation tests for spacing and common expressions.

use crate::eval;
use hashbrown::HashMap;

#[test]
fn spacing_around_operators() {
    let empty = HashMap::new();
    let cases = [
        ("2+2", 4.0),
        ("2 + 2", 4.0),
        ("2+ 2", 4.0),
        ("2 +2", 4.0),
        ("10-3*2", 4.0),
        ("(1+2)*3", 9.0),
        ("1+2*3", 7.0),
        ("2^3", 8.0),
        ("2 ^ 3", 8.0),
    ];
    for (expr, expected) in cases {
        assert_eq!(eval(expr, &empty), Ok(expected), "expr: {expr}");
    }
}

#[test]
fn scientific_notation_and_unary() {
    let empty = HashMap::new();
    assert_eq!(eval("1e-3", &empty), Ok(0.001));
    assert_eq!(eval("2.5E+2", &empty), Ok(250.0));
    assert_eq!(eval("-3", &empty), Ok(-3.0));
    assert_eq!(eval("25+-3", &empty), Ok(22.0));
}

#[test]
fn math_functions_match_libm() {
    let empty = HashMap::new();
    assert_eq!(eval("sqrt(9)", &empty), Ok(3.0));
    assert_eq!(eval("abs(-4)", &empty), Ok(4.0));
    let sin_v = eval("sin(1)", &empty).unwrap();
    assert!((sin_v - 1.0_f64.sin()).abs() < 1e-12);
}
