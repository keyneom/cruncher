extern crate nom;

use crate::error::Error;
use crate::lexer::is_variable;
use crate::token::Token;
use crate::util::FUNCTIONS;
use nom::bytes::complete::is_not;
use nom::bytes::complete::tag;
use nom::bytes::complete::take_till;
use nom::bytes::complete::take_until;
use nom::character::complete::one_of;
use nom::character::complete::{alpha1, multispace0};
use nom::number::complete::double;
use nom::sequence::delimited;
use nom::{branch::alt, IResult};

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub enum Op {
    Minus,
    Plus,
    Div,
    Mul,
    Exp,
}

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
                        crate::token::Op::Plus => Ok(Self::Add(left, right)),
                        crate::token::Op::Minus => Ok(Self::Sub(left, right)),
                        crate::token::Op::Mul => Ok(Self::Mul(left, right)),
                        crate::token::Op::Div => Ok(Self::Div(left, right)),
                        crate::token::Op::Exp => Ok(Self::Exp(left, right)),
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
        let (remaining, val) = delimited(multispace0, double, multispace0)(partial)?;
        Ok((remaining, Self::Value(val)))
    }

    fn variable(partial: &str) -> IResult<&str, Self, Error> {
        let (remaining, var_name) = delimited(multispace0, alpha1, multispace0)(partial)?;
        let var = Self::Variable(var_name.to_string());
        Ok((remaining, var))
    }

    fn exponent(partial: &str) -> IResult<&str, Self, Error> {
        let (right_side_clause, left_side_clause) = take_until("^")(partial)?;
        let left_side_ast = Self::new(left_side_clause).map_err(nom::Err::Error)?;
        let right_side_ast = Self::new(&right_side_clause[1..]).map_err(nom::Err::Error)?;
        Ok((
            "",
            Self::Exp(Box::new(left_side_ast), Box::new(right_side_ast)),
        ))
    }

    fn multiplication(partial: &str) -> IResult<&str, Self, Error> {
        let (right_side_clause, left_side_clause) = take_until("*")(partial)?;
        let left_side_ast = Self::new(left_side_clause).map_err(nom::Err::Error)?;
        let right_side_ast = Self::new(&right_side_clause[1..]).map_err(nom::Err::Error)?;
        Ok((
            "",
            Self::Mul(Box::new(left_side_ast), Box::new(right_side_ast)),
        ))
    }

    fn division(partial: &str) -> IResult<&str, Self, Error> {
        let (right_side_clause, left_side_clause) = take_until("/")(partial)?;
        let left_side_ast = Self::new(left_side_clause).map_err(nom::Err::Error)?;
        let right_side_ast = Self::new(&right_side_clause[1..]).map_err(nom::Err::Error)?;
        Ok((
            "",
            Self::Div(Box::new(left_side_ast), Box::new(right_side_ast)),
        ))
    }

    fn addition(partial: &str) -> IResult<&str, Self, Error> {
        let (right_side_clause, left_side_clause) = take_until("+")(partial)?;
        let left_side_ast = Self::new(left_side_clause).map_err(nom::Err::Error)?;
        let right_side_ast = Self::new(&right_side_clause[1..]).map_err(nom::Err::Error)?;
        Ok((
            "",
            Self::Add(Box::new(left_side_ast), Box::new(right_side_ast)),
        ))
    }

    fn subtraction(partial: &str) -> IResult<&str, Self, Error> {
        let (right_side_clause, left_side_clause) = take_until("-")(partial)?;
        let left_side_ast = Self::new(left_side_clause).map_err(nom::Err::Error)?;
        let right_side_ast = Self::new(&right_side_clause[1..]).map_err(nom::Err::Error)?;
        Ok((
            "",
            Self::Sub(Box::new(left_side_ast), Box::new(right_side_ast)),
        ))
    }

    fn parenthesis(partial: &str) -> IResult<&str, Self, Error> {
        let (remaining, left_or_inside_parens) = take_till(|c| c == '(' || c == ')')(partial)?;
        if remaining.is_empty() && left_or_inside_parens == partial {
            return Err(nom::Err::Error(Error::ParseError(
                "Non-fatal error this is not a parenthetical expression".to_string(),
            )));
        }
        let result = match remaining.as_bytes() {
            [b'(', ..] => {
                // More parenthesis inside!
                let left_side_phrase = left_or_inside_parens.trim();
                // Determine op for left side phrase
                let left_side_op = Self::get_leftside_op(left_side_phrase);
                let (right_side_phrase, inner) = Self::parenthesis(&remaining[1..])?;
                let right_side_phrase = right_side_phrase.trim();
                let right_side_op = Self::get_rightside_op(right_side_phrase);
                // Determine which side `inner` (the nested parenthesis) ought to be applied to based off of order of operations
                match (left_side_op, right_side_op) {
                    (Ok(left_op), Ok(right_op)) => Self::generate_result(
                        left_op,
                        left_side_phrase,
                        inner,
                        right_op,
                        right_side_phrase,
                    ),
                    (Err(e), _) | (_, Err(e)) => Err(e),
                }
            }
            [b')', ..] => {
                // We finished this parenthesis pair
                let inside_parens = left_or_inside_parens.trim();
                let res = Self::new(inside_parens);
                match res {
                    Ok(r) => Ok((&remaining[1..], r)),
                    Err(e) => Err(nom::Err::Error(e)),
                }
                // return right as the remainder
            }
            _ => {
                // Imbalanced parenthesis
                Err(nom::Err::Error(Error::ParseError(
                    "parenthesis are not properly balanced".to_string(),
                )))
            }
        };
        result
    }

    fn generate_result<'lifetime>(
        left_op: Option<Op>,
        left_side_phrase: &'lifetime str,
        inner: Ast,
        right_op: Option<Op>,
        right_side_phrase: &'lifetime str,
    ) -> Result<(&'lifetime str, Self), nom::Err<Error>> {
        match (left_op, right_op) {
            (Some(left_op), Some(right_op)) if left_op >= right_op => {
                let left_ast = Self::get_left_ast(
                    left_op,
                    left_side_phrase,
                    inner,
                    Some(right_op),
                    right_side_phrase,
                )?;
                Self::get_right_ast(None, "", left_ast.1, right_op, right_side_phrase)
            }
            (Some(left_op), None) => Self::get_left_ast(
                left_op,
                left_side_phrase,
                inner,
                right_op,
                right_side_phrase,
            ),
            (Some(left_op), Some(right_op)) => {
                // Right op takes precendence
                let right_ast = Self::get_right_ast(
                    Some(left_op),
                    left_side_phrase,
                    inner,
                    right_op,
                    right_side_phrase,
                )?;
                Self::get_left_ast(left_op, left_side_phrase, right_ast.1, None, "")
            }
            (None, Some(right_op)) => Self::get_right_ast(
                left_op,
                left_side_phrase,
                inner,
                right_op,
                right_side_phrase,
            ),
            (None, None) => Ok(("", inner)),
        }
    }

    fn get_right_ast<'lifetime>(
        left_op: Option<Op>,
        left_side_phrase: &'lifetime str,
        inner: Ast,
        right_op: Op,
        right_side_phrase: &'lifetime str,
    ) -> Result<(&'lifetime str, Self), nom::Err<Error>> {
        match right_op {
            Op::Exp => {
                let right_ast = Self::new(&right_side_phrase[1..]).unwrap();
                Ok((
                    left_side_phrase,
                    Self::Exp(Box::new(inner), Box::new(right_ast)),
                ))
            }
            Op::Mul => {
                let right_ast = Self::new(&right_side_phrase[1..]).unwrap();
                Ok((
                    left_side_phrase,
                    Self::Mul(Box::new(inner), Box::new(right_ast)),
                ))
            }
            Op::Div => {
                let right_ast = Self::new(&right_side_phrase[1..]).unwrap();
                Ok((
                    left_side_phrase,
                    Self::Div(Box::new(inner), Box::new(right_ast)),
                ))
            }
            Op::Plus => {
                let right_ast = Self::new(&right_side_phrase[1..]).unwrap();
                Ok((
                    left_side_phrase,
                    Self::Add(Box::new(inner), Box::new(right_ast)),
                ))
            }
            Op::Minus => {
                let right_ast = Self::new(&right_side_phrase[1..]).unwrap();
                Ok((
                    left_side_phrase,
                    Self::Sub(Box::new(inner), Box::new(right_ast)),
                ))
            }
        }
    }

    fn get_left_ast<'lifetime>(
        left_op: Op,
        left_side_phrase: &'lifetime str,
        inner: Ast,
        right_op: Option<Op>,
        right_side_phrase: &'lifetime str,
    ) -> Result<(&'lifetime str, Self), nom::Err<Error>> {
        match left_op {
            Op::Exp => {
                let left_ast = Self::new(&left_side_phrase[..left_side_phrase.len() - 1]).unwrap();
                Ok((
                    right_side_phrase,
                    Self::Exp(Box::new(left_ast), Box::new(inner)),
                ))
            }
            Op::Mul => {
                let left_ast = Self::new(&left_side_phrase[..left_side_phrase.len() - 1]).unwrap();
                Ok((
                    right_side_phrase,
                    Self::Mul(Box::new(left_ast), Box::new(inner)),
                ))
            }
            Op::Div => {
                let left_ast = Self::new(&left_side_phrase[..left_side_phrase.len() - 1]).unwrap();
                Ok((
                    right_side_phrase,
                    Self::Div(Box::new(left_ast), Box::new(inner)),
                ))
            }
            Op::Plus => {
                let left_ast = Self::new(&left_side_phrase[..left_side_phrase.len() - 1]).unwrap();
                Ok((
                    right_side_phrase,
                    Self::Add(Box::new(left_ast), Box::new(inner)),
                ))
            }
            Op::Minus => {
                let left_ast = Self::new(&left_side_phrase[..left_side_phrase.len() - 1]).unwrap();
                Ok((
                    right_side_phrase,
                    Self::Sub(Box::new(left_ast), Box::new(inner)),
                ))
            }
        }
    }

    fn get_leftside_op(left_side_phrase: &str) -> Result<Option<Op>, nom::Err<Error>> {
        match left_side_phrase.as_bytes() {
            [left_expr @ .., b'^'] => Ok(Some(Op::Exp)),
            [left_expr @ .., b'*'] => Ok(Some(Op::Mul)),
            [left_expr @ .., b'/'] => Ok(Some(Op::Div)),
            [left_expr @ .., b'+'] => Ok(Some(Op::Plus)),
            [left_expr @ .., b'-'] => Ok(Some(Op::Minus)),
            [] => Ok(None),
            _ => Err(nom::Err::Error(Error::ParseError(
                "malformed expression within parethesis".to_string(),
            ))),
        }
    }

    fn get_rightside_op(right_side_phrase: &str) -> Result<Option<Op>, nom::Err<Error>> {
        match right_side_phrase.as_bytes() {
            [b'^', right_expr @ ..] => Ok(Some(Op::Exp)),
            [b'*', right_expr @ ..] => Ok(Some(Op::Mul)),
            [b'/', right_expr @ ..] => Ok(Some(Op::Div)),
            [b'+', right_expr @ ..] => Ok(Some(Op::Plus)),
            [b'-', right_expr @ ..] => Ok(Some(Op::Minus)),
            [b')', right_expr @ ..] => Ok(None),
            [] => Ok(None),
            _ => Err(nom::Err::Error(Error::ParseError(
                "malformed expression within parethesis".to_string(),
            ))),
        }
    }

    pub fn new(equation: &str) -> Result<Self, Error> {
        let mut result = Err(Error::ParseError("Unknown Parse Error".into()));
        let mut current_remaining = equation;
        loop {
            let parse_result = alt((
                Self::parenthesis,
                Self::subtraction,
                Self::addition,
                Self::division,
                Self::multiplication,
                Self::exponent,
                Self::value,
                Self::variable,
            ))(current_remaining);

            if parse_result.is_ok() {
                let (mut remaining, ast) = parse_result.unwrap();
                remaining = remaining.trim();
                if remaining.is_empty() {
                    result = Ok(ast);
                    break;
                } else {
                    current_remaining = remaining;
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
    use super::{Ast, Error};
    use super::{
        Ast::Add, Ast::Div, Ast::Exp, Ast::Function, Ast::Mul, Ast::Sub, Ast::Value, Ast::Variable,
    };
    use crate::lexer::Lexer;
    use test_case::test_case;

    #[test_case("2" => Ok(Value(2.0)) ; "single-digit integers are parsed properly")]
    #[test_case("(2)" => Ok(Value(2.0)) ; "values encased by parenthesis are parsed properly")]
    #[test_case("((2))" => Ok(Value(2.0)) ; "values encased in nested parenthesis are parsed properly")]
    #[test_case("((2) + 3)" => Ok(Add(Box::new(Value(2.0)), Box::new(Value(3.0)))) ; "expressions encased in nested parenthesis are parsed properly")]
    #[test_case("(5+ (2)/ 3)" => Ok(Add(Box::new(Value(5.0)), Box::new(Div(Box::new(Value(2.0)), Box::new(Value(3.0)))))) ; "left-hand expressions encased in nested parenthesis are parsed properly")]
    #[test_case("256" => Ok(Value(256.0)) ; "multi-digit integers are parsed properly")]
    #[test_case("   256     " => Ok(Value(256.0)) ; "multi-digit integers surrounded with whitespace are parsed properly")]
    #[test_case("2.56" => Ok(Value(2.56)) ; "floats are parsed properly")]
    #[test_case("    2.56   " => Ok(Value(2.56)) ; "floats surrounded with whitespace are parsed properly")]
    #[test_case("x" => Ok(Variable("x".to_string())) ; "single-character variables are parsed properly")]
    #[test_case("xyz" => Ok(Variable("xyz".to_string())) ; "multi-character variables are parsed properly")]
    #[test_case("   xyz     " => Ok(Variable("xyz".to_string())) ; "multi-character variables with whitespace are parsed properly")]
    #[test_case("2 + 2" => Ok(Add(Box::new(Value(2.0)), Box::new(Value(2.0)))) ; "addition is parsed properly")]
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
        assert_eq!(ast.extract_value(), Some(19.0));

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
