//! Pratt (TDOP) parser over infix tokens.
//!
//! Replaces the previous shunting-yard → RPN → AST pipeline with a single
//! recursive descent / Pratt pass that builds the AST directly.

use crate::ast::Ast;
use crate::error::Error;
use crate::lexer::is_variable;
use crate::token::{Op, SpannedToken, Token};
use crate::util::FUNCTIONS;

/// Parse infix `tokens` into an optimized AST.
pub fn parse_tokens(tokens: &[SpannedToken]) -> Result<Ast, Error> {
    let mut parser = Parser {
        tokens,
        pos: 0,
    };
    if parser.tokens.is_empty() {
        return Err(Error::parse("empty expression"));
    }
    let ast = parser.parse_expr(0)?;
    if parser.pos < parser.tokens.len() {
        let span = parser.tokens[parser.pos].span;
        return Err(Error::parse_at(
            format!("unexpected token {:?}", parser.tokens[parser.pos].token),
            span,
        ));
    }
    Ok(Ast::optimize(ast))
}

struct Parser<'a> {
    tokens: &'a [SpannedToken],
    pos: usize,
}

impl<'a> Parser<'a> {
    fn peek(&self) -> Option<&'a SpannedToken> {
        self.tokens.get(self.pos)
    }

    fn bump(&mut self) -> Option<&'a SpannedToken> {
        let tok = self.tokens.get(self.pos)?;
        self.pos += 1;
        Some(tok)
    }

    fn parse_expr(&mut self, min_bp: u8) -> Result<Ast, Error> {
        let mut lhs = self.parse_prefix()?;

        loop {
            let Some(st) = self.peek() else { break };
            let Token::Op(op) = &st.token else { break };
            let (l_bp, r_bp) = op.binding_power();
            if l_bp < min_bp {
                break;
            }
            let op = *op;
            self.bump();
            let rhs = self.parse_expr(r_bp)?;
            lhs = match op {
                Op::Plus => Ast::Add(Box::new(lhs), Box::new(rhs)),
                Op::Minus => Ast::Sub(Box::new(lhs), Box::new(rhs)),
                Op::Mul => Ast::Mul(Box::new(lhs), Box::new(rhs)),
                Op::Div => Ast::Div(Box::new(lhs), Box::new(rhs)),
                Op::Exp => Ast::Exp(Box::new(lhs), Box::new(rhs)),
            };
        }
        Ok(lhs)
    }

    fn parse_prefix(&mut self) -> Result<Ast, Error> {
        let Some(st) = self.bump() else {
            return Err(Error::parse("empty expression"));
        };
        match &st.token {
            Token::Value(value) => {
                if FUNCTIONS.contains_key(value) {
                    // function call: name ( expr )
                    let open = self.bump().ok_or_else(|| {
                        Error::parse_at(
                            format!("expected '(' after function '{}'", value),
                            st.span,
                        )
                    })?;
                    if open.token != Token::LParen {
                        return Err(Error::parse_at(
                            format!("expected '(' after function '{}'", value),
                            open.span,
                        ));
                    }
                    let arg = self.parse_expr(0)?;
                    let close = self.bump().ok_or_else(|| {
                        Error::parse_at("expected ')' after function argument", st.span)
                    })?;
                    if close.token != Token::RParen {
                        return Err(Error::parse_at(
                            "expected ')' after function argument",
                            close.span,
                        ));
                    }
                    Ok(Ast::Function(value.clone(), Box::new(arg)))
                } else if let Ok(number) = value.parse::<f64>() {
                    Ok(Ast::Value(number))
                } else if is_variable(value) {
                    Ok(Ast::Variable(value.clone()))
                } else {
                    Err(Error::parse_at(
                        format!("invalid value {}", value),
                        st.span,
                    ))
                }
            }
            Token::LParen => {
                let expr = self.parse_expr(0)?;
                let close = self.bump().ok_or_else(|| {
                    Error::parse_at("mismatched parenthesis", st.span)
                })?;
                if close.token != Token::RParen {
                    return Err(Error::parse_at("mismatched parenthesis", close.span));
                }
                Ok(expr)
            }
            Token::Op(Op::Minus) => {
                // Unary minus: bind tighter than exponentiation's left side.
                const UNARY_BP: u8 = 7;
                let rhs = self.parse_expr(UNARY_BP)?;
                Ok(Ast::Mul(Box::new(Ast::Value(-1.0)), Box::new(rhs)))
            }
            Token::Op(Op::Plus) => {
                const UNARY_BP: u8 = 7;
                self.parse_expr(UNARY_BP)
            }
            other => Err(Error::parse_at(
                format!("unexpected token {:?}", other),
                st.span,
            )),
        }
    }
}

/// Convenience: tokenize + Pratt-parse a source string.
pub fn parse(source: &str) -> Result<Ast, Error> {
    let tokens = crate::lexer::Lexer::new(source).tokenize()?;
    parse_tokens(&tokens)
}

#[cfg(test)]
mod tests {
    use super::parse;
    use crate::ast::Ast;

    #[test]
    fn precedence_and_assoc() {
        let ast = parse("3 + 5 * 2").unwrap();
        assert_eq!(ast, Ast::Value(13.0)); // constant folded

        let ast = parse("2 ^ 3 ^ 2").unwrap();
        // right-assoc: 2^(3^2) = 512
        assert_eq!(ast, Ast::Value(512.0));
    }

    #[test]
    fn function_and_parens() {
        let ast = parse("sin(0)").unwrap();
        assert_eq!(ast, Ast::Value(0.0));

        let ast = parse("(1+2)*3").unwrap();
        assert_eq!(ast, Ast::Value(9.0));
    }

    #[test]
    fn span_on_bad_token() {
        let err = parse("3 + $").unwrap_err();
        assert!(err.span().is_some());
        assert!(err.to_string().contains("ParseError"));
    }
}
