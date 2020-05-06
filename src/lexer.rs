use crate::error::Error;
// use crate::token::{Op, Token};
use crate::util::FUNCTIONS;
use std::iter::Peekable;
use std::str::Chars;

enum S {
    Cons(char, Vec<S>),
}

struct Lexer {
    tokens: Vec<char>,
}

impl Lexer {
    fn new(input: &str) -> Lexer {
        let mut tokens = input
            .chars()
            .filter(|it| !it.is_ascii_whitespace())
            .collect::<Vec<_>>();
        tokens.reverse();
        Lexer { tokens }
    }

    fn next(&mut self) -> Option<char> {
        self.tokens.pop()
    }
}

fn expr(input: &str) -> S {
    let mut lexer = Lexer::new(input);
    let res = expr_bp(&mut lexer).unwrap();
    res
}

struct Frame {
    min_bp: u8,
    lhs: Option<S>,
    token: Option<char>,
}

fn expr_bp(lexer: &mut Lexer) -> Option<S> {
    let mut top = Frame {
        min_bp: 0,
        lhs: None,
        token: None,
    };
    let mut stack = Vec::new();

    loop {
        let token = lexer.next();
        let (token, r_bp) = loop {
            match binding_power(token, top.lhs.is_none()) {
                Some((t, (l_bp, r_bp))) if top.min_bp <= l_bp => break (t, r_bp),
                _ => {
                    let res = top;
                    top = match stack.pop() {
                        Some(it) => it,
                        None => return res.lhs,
                    };

                    let mut args = Vec::new();
                    args.extend(top.lhs);
                    args.extend(res.lhs);
                    let token = res.token.unwrap();
                    top.lhs = Some(S::Cons(token, args));
                }
            };
        };

        if token == ')' {
            assert_eq!(top.token, Some('('));
            let res = top;
            top = stack.pop().unwrap();
            top.lhs = res.lhs;
            continue;
        }

        stack.push(top);
        top = Frame {
            min_bp: r_bp,
            lhs: None,
            token: Some(token),
        };
    }
}

fn binding_power(op: Option<char>, prefix: bool) -> Option<(char, (u8, u8))> {
    let op = op?;
    let res = match op {
        '0'..='9' | 'a'..='z' | 'A'..='Z' => (99, 100),
        '(' => (99, 0),
        ')' => (0, 100),
        '=' => (2, 1),
        '+' | '-' if prefix => (99, 9),
        '+' | '-' => (5, 6),
        '*' | '/' => (7, 8),
        '!' => (11, 100),
        '.' => (14, 13),
        _ => return None,
    };
    Some((op, res))
}

#[cfg(test)]
mod tests {
    use super::*;
    // use crate::token::{Op, Token};
    use test_case::test_case;

    #[test_case("2 + 2" => Ok(vec![Token::Value("2".to_string()), Token::Value("2".to_string()), Token::Op(Op::Plus)]) ; "addition is parsed properly")]
    #[test_case("2+2" => Ok(vec![Token::Value("2".to_string()), Token::Value("2".to_string()), Token::Op(Op::Plus)]) ; "inconclusive -- spaces are parsed properly")]
    fn parse(equation: &str) -> Result<Vec<Token>, Error> {
        expr(equation)
    }
}
