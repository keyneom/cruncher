use crate::error::Error;
use crate::lexer::is_variable;
use crate::token::{Op, Token};
use crate::util::FUNCTIONS;

/// Ast nodes for the expressions
#[derive(Debug, Clone, PartialEq)]
pub enum Ast {
    /// A variable, to be resolved later
    Variable(String),
    /// A constant value
    Value(f64),
    /// <left> + <right>
    Add(Box<Ast>, Box<Ast>),
    /// <left> - <right>
    Sub(Box<Ast>, Box<Ast>),
    /// <left> * <right>
    Mul(Box<Ast>, Box<Ast>),
    /// <left> / <right>
    Div(Box<Ast>, Box<Ast>),
    /// <left> ^ <right>
    Exp(Box<Ast>, Box<Ast>),
    /// fn(<arg>)
    Function(String, Box<Ast>),
}

impl Ast {
    /// Construct the AST for a vector of tokens in reverse polish notation.
    /// This function eats the tokens as it uses them.
    ///
    /// Prefer [`crate::parser::parse`] (Pratt) for new code; this remains for
    /// compatibility with the legacy shunting-yard token stream.
    pub fn from_tokens(tokens: &mut Vec<Token>, context: &str) -> Result<Self, Error> {
        let unoptimized_ast = Self::from_tokens_internal(tokens, context);
        if let Ok(unoptimized_ast) = unoptimized_ast {
            Ok(Self::optimize(unoptimized_ast))
        } else {
            unoptimized_ast
        }
    }

    fn from_tokens_internal(tokens: &mut Vec<Token>, context: &str) -> Result<Self, Error> {
        if let Some(token) = tokens.pop() {
            match token {
                Token::Value(value) => {
                    if FUNCTIONS.contains_key(&value) {
                        let args = Box::new(Self::from_tokens(tokens, " in function call")?);
                        Ok(Self::Function(value, args))
                    } else if let Ok(number) = value.parse() {
                        Ok(Self::Value(number))
                    } else if is_variable(&value) {
                        Ok(Self::Variable(value))
                    } else {
                        Err(Error::parse(format!("invalid value {}", value)))
                    }
                }
                Token::Op(op) => {
                    let right = Box::new(Self::from_tokens(tokens, " after operator")?);
                    let left = Box::new(Self::from_tokens(tokens, " befor operator")?);
                    match op {
                        Op::Plus => Ok(Self::Add(left, right)),
                        Op::Minus => Ok(Self::Sub(left, right)),
                        Op::Mul => Ok(Self::Mul(left, right)),
                        Op::Div => Ok(Self::Div(left, right)),
                        Op::Exp => Ok(Self::Exp(left, right)),
                    }
                }
                other => panic!("Internal error: got {:?} token after shunting yard", other),
            }
        } else {
            Err(Error::parse(format!("empty expression{}", context)))
        }
    }

    /// If the AST node correspond to a constant, get `Some(constant)`. Else,
    /// get `None`
    pub fn value(&self) -> Option<f64> {
        if let Self::Value(value) = *self {
            Some(value)
        } else {
            None
        }
    }

    /// Optimize the AST by doing constants propagation and algebraic identities.
    pub fn optimize(self) -> Self {
        match self {
            Self::Variable(_) | Self::Value(_) => self,
            Self::Function(name, arg) => {
                let arg = arg.optimize();
                if let Some(arg_val) = arg.value() {
                    if let Some(func) = FUNCTIONS.get(&name) {
                        return Self::Value(func(arg_val));
                    }
                }
                Self::Function(name, Box::new(arg))
            }
            Self::Add(left, right) => {
                let left = left.optimize();
                let right = right.optimize();
                match (left.value(), right.value()) {
                    (Some(l), Some(r)) => Self::Value(l + r),
                    (Some(0.0), _) => right,
                    (_, Some(0.0)) => left,
                    _ => Self::Add(Box::new(left), Box::new(right)),
                }
            }
            Self::Sub(left, right) => {
                let left = left.optimize();
                let right = right.optimize();
                match (left.value(), right.value()) {
                    (Some(l), Some(r)) => Self::Value(l - r),
                    (_, Some(0.0)) => left,
                    _ => Self::Sub(Box::new(left), Box::new(right)),
                }
            }
            Self::Mul(left, right) => {
                let left = left.optimize();
                let right = right.optimize();
                match (left.value(), right.value()) {
                    (Some(l), Some(r)) => Self::Value(l * r),
                    (Some(0.0), _) | (_, Some(0.0)) => Self::Value(0.0),
                    (Some(1.0), _) => right,
                    (_, Some(1.0)) => left,
                    (Some(-1.0), _) => Self::Mul(Box::new(Self::Value(-1.0)), Box::new(right)),
                    _ => Self::Mul(Box::new(left), Box::new(right)),
                }
            }
            Self::Div(left, right) => {
                let left = left.optimize();
                let right = right.optimize();
                match (left.value(), right.value()) {
                    (Some(l), Some(r)) => Self::Value(l / r),
                    (_, Some(1.0)) => left,
                    (Some(0.0), _) => Self::Value(0.0),
                    _ => Self::Div(Box::new(left), Box::new(right)),
                }
            }
            Self::Exp(left, right) => {
                let left = left.optimize();
                let right = right.optimize();
                match (left.value(), right.value()) {
                    (Some(l), Some(r)) => Self::Value(l.powf(r)),
                    (_, Some(1.0)) => left,
                    (_, Some(0.0)) => Self::Value(1.0),
                    (Some(1.0), _) => Self::Value(1.0),
                    _ => Self::Exp(Box::new(left), Box::new(right)),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Ast;
    use crate::parser;

    #[test]
    fn optimize() {
        let ast = parser::parse("3 + 5").unwrap();
        assert_eq!(ast.value(), Some(8.0));

        let ast = parser::parse("(3 + 5^2)*45").unwrap();
        assert_eq!(ast.value(), Some(1260.0));

        let ast = parser::parse("sqrt(9)").unwrap();
        assert_eq!(ast.value(), Some(3.0));
    }

    #[test]
    fn algebraic_identities() {
        // x*1 and x+0 fold away when the other side is a variable
        let ast = parser::parse("a * 1 + 0").unwrap();
        assert_eq!(ast, Ast::Variable("a".into()));

        let ast = parser::parse("a ^ 1").unwrap();
        assert_eq!(ast, Ast::Variable("a".into()));

        let ast = parser::parse("a * 0").unwrap();
        assert_eq!(ast, Ast::Value(0.0));
    }
}
