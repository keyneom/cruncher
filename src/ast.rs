extern crate nom;

use crate::error::Error;
use crate::lexer::is_variable;
use crate::token::{Op, Token};
use crate::util::FUNCTIONS;
use nom::character::complete::{alpha1, multispace0};
use nom::sequence::delimited;
use nom::{branch::alt, IResult};

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
    Function(fn(f64) -> f64, Box<Ast>),
}

impl Ast {
    /// Construct the AST for a vector of tokens in reverse polish notation.
    /// This function eats the tokens as it uses them
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
                    if let Some(&func) = FUNCTIONS.get(&value) {
                        let args = Box::new(Self::from_tokens(tokens, " in function call")?);
                        Ok(Self::Function(func, args))
                    } else if let Ok(number) = value.parse() {
                        Ok(Self::Value(number))
                    } else if is_variable(&value) {
                        Ok(Self::Variable(value))
                    } else {
                        Err(Error::ParseError(format!("invalid value {}", value)))
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
            Err(Error::ParseError(format!("empty expression{}", context)))
        }
    }

    /// If the AST node correspond to a constant, get `Some(constant)`. Else,
    /// get `None`
    pub fn extract_value(&self) -> Option<f64> {
        if let Self::Value(value) = *self {
            Some(value)
        } else {
            None
        }
    }

    /// Optimize the AST by doing constants propagation
    pub fn optimize(self) -> Self {
        match self {
            Self::Variable(_) | Self::Value(_) => self,
            Self::Function(func, arg) => {
                let arg = arg.optimize();
                if let Some(arg) = arg.extract_value() {
                    return Self::Value(func(arg));
                }
                return Self::Function(func, Box::new(arg));
            }
            Self::Add(left, right) => {
                let left = left.optimize();
                let right = right.optimize();
                if let Some(left) = left.extract_value() {
                    if let Some(right) = right.extract_value() {
                        return Self::Value(left + right);
                    }
                }
                return Self::Add(Box::new(left), Box::new(right));
            }
            Self::Sub(left, right) => {
                let left = left.optimize();
                let right = right.optimize();
                if let Some(left) = left.extract_value() {
                    if let Some(right) = right.extract_value() {
                        return Self::Value(left - right);
                    }
                }
                return Self::Sub(Box::new(left), Box::new(right));
            }
            Self::Mul(left, right) => {
                let left = left.optimize();
                let right = right.optimize();
                if let Some(left) = left.extract_value() {
                    if let Some(right) = right.extract_value() {
                        return Self::Value(left * right);
                    }
                }
                return Self::Mul(Box::new(left), Box::new(right));
            }
            Self::Div(left, right) => {
                let left = left.optimize();
                let right = right.optimize();
                if let Some(left) = left.extract_value() {
                    if let Some(right) = right.extract_value() {
                        return Self::Value(left / right);
                    }
                }
                return Self::Div(Box::new(left), Box::new(right));
            }
            Self::Exp(left, right) => {
                let left = left.optimize();
                let right = right.optimize();
                if let Some(left) = left.extract_value() {
                    if let Some(right) = right.extract_value() {
                        return Self::Value(left.powf(right));
                    }
                }
                return Self::Exp(Box::new(left), Box::new(right));
            }
        }
    }

    fn parse_operation(input: &str) -> IResult<&str, Self, Error> {
        todo!("unimplemented")
    }

    fn function(partial: &str) -> IResult<&str, Self, Error> {
        todo!("unimplemented")
    }

    fn value(partial: &str) -> IResult<&str, Self, Error> {
        todo!("unimplemented")
    }

    fn variable(partial: &str) -> IResult<&str, Self, Error> {
        let (remaining, var_name) = delimited(multispace0, alpha1, multispace0)(partial)?;
        let var = Self::Variable(var_name.to_string());
        Ok((remaining, var))
    }

    fn exponent(partial: &str) -> IResult<&str, Self, Error> {
        todo!("unimplemented")
    }

    fn multiplication(partial: &str) -> IResult<&str, Self, Error> {
        todo!("unimplemented")
    }

    fn division(partial: &str) -> IResult<&str, Self, Error> {
        todo!("unimplemented")
    }

    fn addition(partial: &str) -> IResult<&str, Self, Error> {
        todo!("unimplemented")
    }

    fn subtraction(partial: &str) -> IResult<&str, Self, Error> {
        todo!("unimplemented")
    }

    pub fn new(equation: &str) -> Result<Self, Error> {
        let mut result = Err(Error::ParseError("Unknown Parse Error".into()));
        loop {
            let parse_result = alt((Self::value, Self::variable))(equation);
            // match parse_result {
            //     Ok((remaining, token)) => {
            //         result = Ok(token);
            //     }
            //     Err(nom::Err::Error(e)) => {
            //         result = Err(e);
            //     }
            //     Err(_) => {
            //         result = Err(Error::ParseError(
            //             "Unknown Parse Error building AST".to_string(),
            //         ))
            //     }
            // }

            if result.is_ok() {
                let (remaining, ast) = parse_result.unwrap();
                if remaining.is_empty() {
                    result = Ok(ast);
                    break;
                }
            } else {
                match parse_result {
                    Err(nom::Err::Error(e)) => {
                        result = Err(e);
                    }
                    Err(_) => {
                        result = Err(Error::ParseError(
                            "Unknown Parse Error building AST".to_string(),
                        ))
                    }
                    Ok(_) => panic!("Should be impossible!"),
                }
                break;
            }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::Lexer;
    use test_case::test_case;

    #[test_case("2" => Ok(Ast::Value(2.0)) ; "single-digit integers are parsed properly")]
    #[test_case("256" => Ok(Ast::Value(256.0)) ; "multi-digit integers are parsed properly")]
    #[test_case("2.56" => Ok(Ast::Value(2.56)) ; "floats are parsed properly")]
    #[test_case("x" => Ok(Ast::Variable("x".to_string())) ; "single-character variables are parsed properly")]
    #[test_case("xyz" => Ok(Ast::Variable("xyz".to_string())) ; "multi-character variables are parsed properly")]
    // #[test_case("2 + 2" => Ok(vec![Token::Value("2".to_string()), Token::Value("2".to_string()), Token::Op(Op::Plus)]) ; "addition is parsed properly")]
    // #[test_case("6 - 2" => Ok(vec![Token::Value("6".to_string()), Token::Value("2".to_string()), Token::Op(Op::Minus)]) ; "subtraction is parsed properly")]
    // #[test_case("2 * 2" => Ok(vec![Token::Value("2".to_string()), Token::Value("2".to_string()), Token::Op(Op::Mul)]) ; "multiplication is parsed properly")]
    // #[test_case("8 / 2" => Ok(vec![Token::Value("8".to_string()), Token::Value("2".to_string()), Token::Op(Op::Div)]) ; "division is parsed properly")]
    // #[test_case("2+2" => Ok(vec![Token::Value("2".to_string()), Token::Value("2".to_string()), Token::Op(Op::Plus)]) ; "spaces are parsed properly")]
    // #[test_case("6 + -2" => Ok(vec![Token::Value("6".to_string()), Token::Value("-2".to_string()), Token::Op(Op::Plus)]) ; "negatives are parsed properly")]
    // #[test_case("4+5*3" => Ok(vec![Token::Value("4".to_string()), Token::Value("5".to_string()), Token::Value("3".to_string()), Token::Op(Op::Mul), Token::Op(Op::Plus)]) ; "negatives are parsed properly")]
    fn parse(equation: &str) -> Result<Ast, Error> {
        Ast::new(equation)
    }

    #[test]
    // Verifies that we have reduced the input to a single value token when possible
    fn optimize() {
        let mut lexer = Lexer::new("4 + 5 * 3");
        let ast = Ast::from_tokens(&mut lexer.parse().unwrap(), "").unwrap();
        assert_eq!(ast.extract_value(), Some(18.0));

        let mut lexer = Lexer::new("3 + 5");
        let ast = Ast::from_tokens(&mut lexer.parse().unwrap(), "").unwrap();
        assert_eq!(ast.extract_value(), Some(8.0));

        let mut lexer = Lexer::new("(3 + 5^2)*45");
        let ast = Ast::from_tokens(&mut lexer.parse().unwrap(), "").unwrap();
        assert_eq!(ast.extract_value(), Some(1260.0));

        let mut lexer = Lexer::new("sqrt(9)");
        let ast = Ast::from_tokens(&mut lexer.parse().unwrap(), "").unwrap();
        assert_eq!(ast.extract_value(), Some(3.0));
    }
}
