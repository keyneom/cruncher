use crate::ast::Ast;
use crate::parser;
use crate::Error;
use hashbrown::HashMap;

/// Evaluate a single expression from `input`.
///
/// Returns `Ok(result)` if the evaluation is successful, or `Err(cause)` if
/// parsing or evaluating the expression failed.
///
/// # Example
///
/// ```
/// # use hashbrown::HashMap;
/// use cruncher::{eval};
///
/// assert_eq!(eval("45 - 2^3", &HashMap::new()), Ok(37.0));
///
/// let mut context :HashMap<String,f64> = HashMap::new();
/// context.insert("a".into(), -5.0);
/// assert_eq!(eval("3 * a", &context), Ok(-15.0));
/// ```
pub fn eval<'a, C>(input: &str, context: C) -> Result<f64, Error>
where
    C: Into<&'a HashMap<String, f64>>,
{
    Expr::parse(input).and_then(|expr| expr.eval(context))
}

/// A parsed and optimized mathematical expression (AST interpreter).
///
/// # Examples
/// ```
/// # use cruncher::{Expr};
/// # use hashbrown::HashMap;
/// let expr = Expr::parse("3 + 5 * 2").unwrap();
/// assert_eq!(expr.eval(&HashMap::new()), Ok(13.0));
///
/// let mut context :HashMap<String,f64> = HashMap::new();
/// context.insert("a".into(), 42.0);
/// let expr = Expr::parse("-2 * a").unwrap();
/// assert_eq!(expr.eval(&context), Ok(-84.0));
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct Expr {
    ast: Ast,
}

impl Expr {
    /// Parse the given mathematical `expression` into an `Expr` (Pratt parser).
    ///
    /// # Examples
    /// ```
    /// # use cruncher::Expr;
    /// // A valid expression
    /// assert!(Expr::parse("3 + 5 * 2").is_ok());
    /// // an invalid expression
    /// assert!(Expr::parse("3eff + 5 * 2").is_err());
    /// ```
    pub fn parse(expression: &str) -> Result<Self, Error> {
        Ok(Self {
            ast: parser::parse(expression)?,
        })
    }

    /// Evaluate the expression in a given optional `context`.
    ///
    /// # Examples
    ///
    /// ```
    /// # use cruncher::{Expr};
    /// # use hashbrown::HashMap;
    /// let expr = Expr::parse("3 + 5 * 2").unwrap();
    /// assert_eq!(expr.eval(&HashMap::new()), Ok(13.0));
    ///
    /// let expr = Expr::parse("3 + a").unwrap();
    ///
    /// let mut context :HashMap<String,f64> = HashMap::new();
    /// context.insert("a".into(), -5.0);
    /// assert_eq!(expr.eval(&context), Ok(-2.0));
    /// context.insert("a".into(), 2.0);
    /// assert_eq!(expr.eval(&context), Ok(5.0));
    /// ```
    pub fn eval<'a, C>(&self, context: C) -> Result<f64, Error>
    where
        C: Into<&'a HashMap<String, f64>>,
    {
        Self::inner_eval(&self.ast, context.into())
    }

    /// Evaluate the expression over columnar variable data.
    ///
    /// Each key in `columns` is a variable name; each value is a slice of samples.
    /// All used variables must have at least `n` values. Returns `n` results.
    ///
    /// This avoids building a per-row `HashMap` and is the preferred path for
    /// Monte Carlo / scenario batches on the AST interpreter.
    pub fn eval_batch(
        &self,
        columns: &HashMap<String, &[f64]>,
        n: usize,
    ) -> Result<Vec<f64>, Error> {
        let mut out = vec![0.0; n];
        self.eval_batch_into(columns, &mut out)?;
        Ok(out)
    }

    /// Like [`Self::eval_batch`], but writes into a caller-provided buffer
    /// (zero allocation on the hot path aside from the AST walk).
    pub fn eval_batch_into(
        &self,
        columns: &HashMap<String, &[f64]>,
        out: &mut [f64],
    ) -> Result<(), Error> {
        let n = out.len();
        let mut needed: Vec<&str> = Vec::new();
        Self::collect_vars(&self.ast, &mut needed);
        needed.sort_unstable();
        needed.dedup();

        let mut resolved: Vec<(&str, &[f64])> = Vec::with_capacity(needed.len());
        for name in needed {
            let col = columns
                .get(name)
                .ok_or_else(|| Error::name(format!("name '{}' is not defined", name)))?;
            if col.len() < n {
                return Err(Error::name(format!("Missing data for parameter: {}", name)));
            }
            resolved.push((name, *col));
        }

        for (i, slot) in out.iter_mut().enumerate() {
            *slot = Self::inner_eval_row(&self.ast, &resolved, i)?;
        }
        Ok(())
    }

    fn collect_vars<'a>(ast: &'a Ast, out: &mut Vec<&'a str>) {
        match ast {
            Ast::Variable(name) => out.push(name),
            Ast::Value(_) => {}
            Ast::Function(_, arg) => Self::collect_vars(arg, out),
            Ast::Add(l, r) | Ast::Sub(l, r) | Ast::Mul(l, r) | Ast::Div(l, r) | Ast::Exp(l, r) => {
                Self::collect_vars(l, out);
                Self::collect_vars(r, out);
            }
        }
    }

    fn inner_eval_row(
        ast: &Ast,
        columns: &[(&str, &[f64])],
        row: usize,
    ) -> Result<f64, Error> {
        match *ast {
            Ast::Variable(ref name) => {
                for &(n, col) in columns {
                    if n == name {
                        return Ok(col[row]);
                    }
                }
                Err(Error::name(format!("name '{}' is not defined", name)))
            }
            Ast::Value(number) => Ok(number),
            Ast::Add(ref left, ref right) => Ok(
                Self::inner_eval_row(left, columns, row)?
                    + Self::inner_eval_row(right, columns, row)?,
            ),
            Ast::Sub(ref left, ref right) => Ok(
                Self::inner_eval_row(left, columns, row)?
                    - Self::inner_eval_row(right, columns, row)?,
            ),
            Ast::Mul(ref left, ref right) => Ok(
                Self::inner_eval_row(left, columns, row)?
                    * Self::inner_eval_row(right, columns, row)?,
            ),
            Ast::Div(ref left, ref right) => Ok(
                Self::inner_eval_row(left, columns, row)?
                    / Self::inner_eval_row(right, columns, row)?,
            ),
            Ast::Exp(ref left, ref right) => Ok(Self::inner_eval_row(left, columns, row)?
                .powf(Self::inner_eval_row(right, columns, row)?)),
            Ast::Function(ref name, ref arg) => {
                let func = crate::util::FUNCTIONS
                    .get(name)
                    .ok_or_else(|| Error::name(format!("unknown function '{}'", name)))?;
                Ok(func(Self::inner_eval_row(arg, columns, row)?))
            }
        }
    }

    fn inner_eval(ast: &Ast, context: &HashMap<String, f64>) -> Result<f64, Error> {
        match *ast {
            Ast::Variable(ref name) => context
                .get(name)
                .map(std::borrow::ToOwned::to_owned)
                .ok_or_else(|| Error::name(format!("name '{}' is not defined", name))),
            Ast::Value(number) => Ok(number),
            Ast::Add(ref left, ref right) => {
                Ok(Self::inner_eval(left, context)? + Self::inner_eval(right, context)?)
            }
            Ast::Sub(ref left, ref right) => {
                Ok(Self::inner_eval(left, context)? - Self::inner_eval(right, context)?)
            }
            Ast::Mul(ref left, ref right) => {
                Ok(Self::inner_eval(left, context)? * Self::inner_eval(right, context)?)
            }
            Ast::Div(ref left, ref right) => {
                Ok(Self::inner_eval(left, context)? / Self::inner_eval(right, context)?)
            }
            Ast::Exp(ref left, ref right) => {
                Ok(Self::inner_eval(left, context)?.powf(Self::inner_eval(right, context)?))
            }
            Ast::Function(ref name, ref arg) => {
                let func = crate::util::FUNCTIONS
                    .get(name)
                    .ok_or_else(|| Error::name(format!("unknown function '{}'", name)))?;
                Ok(func(Self::inner_eval(arg, context)?))
            }
        }
    }
}

/// A JIT-compiled expression with a columnar batch API.
///
/// Prefer this over repeatedly calling [`Expr::eval`] when evaluating the same
/// formula over large Monte Carlo / scenario columns.
pub struct CompiledExpr {
    func: Box<crate::jit::BatchFunction>,
}

impl CompiledExpr {
    /// Parse and JIT-compile `expression`.
    pub fn compile(expression: &str) -> Result<Self, Error> {
        let mut jit = crate::jit::JIT::new();
        Ok(Self {
            func: jit.compile(expression)?,
        })
    }

    /// Evaluate over `n` columnar rows, allocating a new result buffer.
    pub fn eval_batch(
        &self,
        columns: &HashMap<String, &[f64]>,
        n: usize,
    ) -> Result<Vec<f64>, Error> {
        (self.func)(columns, n)
    }

    /// Evaluate into a caller-provided buffer (`out.len()` rows).
    pub fn eval_batch_into(
        &self,
        columns: &HashMap<String, &[f64]>,
        out: &mut [f64],
    ) -> Result<(), Error> {
        let n = out.len();
        let values = (self.func)(columns, n)?;
        out.copy_from_slice(&values);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{CompiledExpr, Expr, HashMap};

    #[test]
    fn parse() {
        let valid_expressions = [
            "3 + +5e67",
            "(3 + -5)*45",
            "(3. + 5.0)*\t\n45",
            "(3 + 5^5e-6)*45",
            "sin(34.0) ^ sqrt(28.0)",
            "abc[ty8789]",
        ];
        for expr in &valid_expressions {
            assert!(Expr::parse(expr).is_ok());
        }
    }

    #[test]
    fn eval() {
        let mut context: HashMap<String, f64> = HashMap::new();
        context.insert("a".into(), 1.0);
        context.insert("b".into(), 2.0);

        let empty_context = &HashMap::new();

        let eval_pairs = [
            ("3 + 5", empty_context, 8.0),
            ("2+2", empty_context, 4.0),
            ("2 + 2", empty_context, 4.0),
            ("10-3*2", empty_context, 4.0),
            ("2 - 5", empty_context, -3.0),
            ("2 * 5", empty_context, 10.0),
            ("10 / 5", empty_context, 2.0),
            ("2 ^ 3", empty_context, 8.0),
            ("-3", empty_context, -3.0),
            ("25 + -3", empty_context, 22.0),
            ("25 - -3", empty_context, 28.0),
            ("25 - -3", empty_context, 28.0),
            ("3 + 5 * 2", empty_context, 13.0),
            ("sqrt(9)", empty_context, 3.0),
            ("sin(18.0) * 3", empty_context, 3.0 * f64::sin(18.0)),
            ("2 * a", &context, 2.0),
            ("(a + b)^2", &context, 9.0),
        ];
        for eval_pair in &eval_pairs {
            assert_eq!(super::eval(eval_pair.0, eval_pair.1), Ok(eval_pair.2));
        }

        let result = super::eval("2 * z", &context);
        assert_eq!(
            result.err().unwrap().to_string(),
            "NameError: name 'z' is not defined"
        );
        let result = super::eval("2 * a", empty_context);
        assert_eq!(
            result.err().unwrap().to_string(),
            "NameError: name 'a' is not defined"
        );
    }

    #[test]
    fn eval_batch_columns() {
        let expr = Expr::parse("(var1 + var2 * 3) / (2 + 3) - something").unwrap();
        let mut owned: HashMap<String, Vec<f64>> = HashMap::new();
        owned.insert("var1".into(), vec![10.0, 11.0]);
        owned.insert("var2".into(), vec![20.0, 21.0]);
        owned.insert("something".into(), vec![30.0, 31.0]);
        let cols: HashMap<String, &[f64]> = owned
            .iter()
            .map(|(k, v)| (k.clone(), v.as_slice()))
            .collect();
        let results = expr.eval_batch(&cols, 2).unwrap();
        assert!((results[0] - ((10.0 + 20.0 * 3.0) / 5.0 - 30.0)).abs() < 1e-12);
        assert!((results[1] - ((11.0 + 21.0 * 3.0) / 5.0 - 31.0)).abs() < 1e-12);

        let mut into = [0.0; 2];
        expr.eval_batch_into(&cols, &mut into).unwrap();
        assert_eq!(into.to_vec(), results);
    }

    #[test]
    fn compiled_expr_batch() {
        let compiled = CompiledExpr::compile("a + b + c + d + e").unwrap();
        let mut owned: HashMap<String, Vec<f64>> = HashMap::new();
        for (name, base) in [("a", 1.0), ("b", 2.0), ("c", 3.0), ("d", 4.0), ("e", 5.0)] {
            owned.insert(name.into(), vec![base, base + 1.0]);
        }
        let cols: HashMap<String, &[f64]> = owned
            .iter()
            .map(|(k, v)| (k.clone(), v.as_slice()))
            .collect();
        let results = compiled.eval_batch(&cols, 2).unwrap();
        assert!((results[0] - 15.0).abs() < 1e-12);
        assert!((results[1] - 20.0).abs() < 1e-12);
    }

    use std::time::Instant;
    #[test]
    #[ignore = "heavy micro-benchmark; use criterion benches instead"]
    fn bench() {
        let watch = Instant::now();
        let t = Expr::parse("(var1 + var2 * 3) / (2 + 3) - something").unwrap();
        let capacity = 5_000_000;
        let iterations = 5_000_000;
        let mut dicts = Vec::with_capacity(capacity);
        for i in 1..=iterations {
            let mut dict: HashMap<String, f64> = HashMap::with_capacity(3);
            dict.insert("var1".to_owned(), 10.0 + f64::from(i));
            dict.insert("var2".to_owned(), 20.0 + f64::from(i));
            dict.insert("something".to_owned(), 30.0 + f64::from(i));
            dicts.push(dict);
        }
        let watch = watch.elapsed();

        let mut results: Vec<f64> = Vec::with_capacity(capacity);

        let watch2 = Instant::now();
        for dict in &dicts {
            results.push(t.eval(dict).unwrap());
        }
        let watch2 = watch2.elapsed();

        println!("{}", results[0]);
        println!("{}", watch.as_millis());
        println!("{}", watch2.as_millis());
    }
}
