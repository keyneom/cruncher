use crate::error::{Error, Span};
use crate::token::{Op, SpannedToken, Token};
use crate::util::FUNCTIONS;
use std::iter::Peekable;
use std::str::Chars;

/// Check if `ident` is a valid variable name
///
/// # Examples
///
/// ```
/// # use cruncher::is_variable;
///
/// assert_eq!(is_variable("__abc3"), true);
/// assert_eq!(is_variable("34zb"), false);
/// ```
#[must_use]
pub fn is_variable(ident: &str) -> bool {
    let mut chars = ident.chars();
    // Check first char
    if !chars.next().map_or(false, is_variable_start) {
        return false;
    }
    // Check all others
    for c in chars {
        if !is_variable_part(c) {
            return false;
        }
    }
    true
}

/// An helper struct for lexing the input
pub struct Lexer<'a> {
    input: Peekable<Chars<'a>>,
    /// Byte offset of the next character to be consumed.
    offset: usize,
    /// True when the previous emitted token was a value or `)`.
    /// Used to distinguish infix `+`/`-` from signed literals.
    after_value: bool,
}

impl<'a> Lexer<'a> {
    #[must_use]
    pub fn new(string: &str) -> Lexer<'_> {
        Lexer {
            input: string.chars().peekable(),
            offset: 0,
            after_value: false,
        }
    }

    /// Tokenize into an infix stream with spans (for the Pratt parser).
    pub fn tokenize(&mut self) -> Result<Vec<SpannedToken>, Error> {
        let mut output = Vec::new();
        while let Some(st) = self.next_token()? {
            output.push(st);
        }
        Ok(output)
    }

    /// Legacy shunting-yard parse producing reverse-polish `Token`s.
    ///
    /// Prefer [`Self::tokenize`] + [`crate::parser::parse_tokens`].
    pub fn parse(&mut self) -> Result<Vec<Token>, Error> {
        let mut output = Vec::new();
        let mut operators = Vec::new();

        'tokens: while let Some(st) = self.next_token()? {
            let token = st.token;
            match token {
                Token::Value(ref name) if FUNCTIONS.contains_key(name) => {
                    operators.push(token.clone());
                }
                Token::Value(_) => output.push(token),
                Token::Op(o1) => {
                    'operators: while let Some(token) = operators.last().cloned() {
                        match token {
                            Token::Op(o2) => {
                                let pop_me =
                                    o1.is_left_associative() && o1.precedence() <= o2.precedence();
                                let pop_me = pop_me
                                    || o1.is_right_associative()
                                        && o1.precedence() < o2.precedence();
                                if pop_me {
                                    operators.pop();
                                    output.push(Token::Op(o2));
                                } else {
                                    break 'operators;
                                }
                            }
                            _ => break 'operators,
                        }
                    }
                    operators.push(token)
                }
                Token::LParen => operators.push(token),
                Token::RParen => {
                    while let Some(token) = operators.pop() {
                        match token {
                            Token::LParen => {
                                let next_is_fn =
                                    if let Some(&Token::Value(ref name)) = operators.last() {
                                        FUNCTIONS.contains_key(name)
                                    } else {
                                        false
                                    };

                                if next_is_fn {
                                    output.push(operators.pop().expect("emtpy operator stack"));
                                }
                                continue 'tokens;
                            }
                            Token::Op(_) => output.push(token),
                            other => panic!("Internal bug: found {:?} in operators stack", other),
                        }
                    }
                    return Err(Error::parse_at("mismatched parenthesis", st.span));
                }
            }
        }

        while let Some(token) = operators.pop() {
            match token {
                Token::LParen => return Err(Error::parse("mismatched parenthesis")),
                Token::Op(_) => output.push(token),
                other => panic!("Internal bug: found {:?} in operators stack", other),
            }
        }
        Ok(output)
    }

    fn bump_char(&mut self) -> Option<char> {
        let c = self.input.next()?;
        self.offset += c.len_utf8();
        Some(c)
    }

    fn next_token(&mut self) -> Result<Option<SpannedToken>, Error> {
        // Skip whitespace
        while let Some(&c) = self.input.peek() {
            if c == ' ' || c == '\t' || c == '\n' || c == '\r' {
                self.bump_char();
            } else {
                break;
            }
        }

        let start = self.offset;
        let Some(c) = self.bump_char() else {
            return Ok(None);
        };

        let token = match c {
            '+' | '-' => {
                // After a value or `)`, `+`/`-` are infix operators.
                // Otherwise they introduce a signed numeric literal when followed by a digit/`.`.
                if self.after_value {
                    if c == '+' {
                        Token::Op(Op::Plus)
                    } else {
                        Token::Op(Op::Minus)
                    }
                } else if self
                    .input
                    .peek()
                    .map_or(false, |p| p.is_ascii_digit() || *p == '.')
                {
                    let mut ident = String::new();
                    ident.push(c);
                    'signed: while let Some(&ch) = self.input.peek() {
                        if is_value_part(ch)
                            || ((ch == '+' || ch == '-') && is_exponent_sign_context(&ident))
                        {
                            self.bump_char();
                            ident.push(ch);
                        } else {
                            break 'signed;
                        }
                    }
                    Token::Value(ident)
                } else if c == '+' {
                    Token::Op(Op::Plus)
                } else {
                    Token::Op(Op::Minus)
                }
            }
            c if is_value_start(c) => {
                let mut ident = String::new();
                ident.push(c);
                'value: while let Some(&ch) = self.input.peek() {
                    if is_value_part(ch)
                        || ((ch == '+' || ch == '-') && is_exponent_sign_context(&ident))
                    {
                        self.bump_char();
                        ident.push(ch);
                    } else {
                        break 'value;
                    }
                }
                Token::Value(ident)
            }
            '*' => Token::Op(Op::Mul),
            '/' => Token::Op(Op::Div),
            '^' => Token::Op(Op::Exp),
            '(' => Token::LParen,
            ')' => Token::RParen,
            other => {
                return Err(Error::parse_at(
                    format!("unexpected character in input: {}", other),
                    Span::new(start, self.offset),
                ));
            }
        };
        let span = Span::new(start, self.offset);
        self.after_value = matches!(token, Token::Value(_) | Token::RParen);
        Ok(Some(SpannedToken { token, span }))
    }
}

/// Check if `c` can appear at the first character of a value
fn is_value_start(c: char) -> bool {
    c.is_ascii_digit() || is_variable_start(c)
}

/// Check if `c` can appear inside a value (not including exponent signs).
fn is_value_part(c: char) -> bool {
    c == '.' || is_variable_part(c)
}

/// Allow `+`/`-` only as the sign of a scientific-notation exponent (`1e-10`).
fn is_exponent_sign_context(ident: &str) -> bool {
    let bytes = ident.as_bytes();
    if bytes.is_empty() {
        return false;
    }
    let last = bytes[bytes.len() - 1];
    if last != b'e' && last != b'E' {
        return false;
    }
    let start = if bytes[0] == b'+' || bytes[0] == b'-' {
        1
    } else {
        0
    };
    let body = &bytes[start..bytes.len() - 1];
    body.iter().any(|b| b.is_ascii_digit())
        && body.iter().all(|b| b.is_ascii_digit() || *b == b'.')
}

/// Check if `c` can appear at the first character of a variable
fn is_variable_start(c: char) -> bool {
    c == '_' || (c.is_ascii() && c.is_alphabetic())
}

/// Check if `c` can appear inside a variable
fn is_variable_part(c: char) -> bool {
    c == '.' || c == '_' || c == '[' || c == ']' || (c.is_ascii() && c.is_alphanumeric())
}

#[cfg(test)]
mod tests {
    use super::{
        is_value_part, is_value_start, is_variable, is_variable_part, is_variable_start, Error,
        Lexer,
    };
    use crate::token::{Op, Token};
    use test_case::test_case;

    #[test_case("2 + 2" => Ok(vec![Token::Value("2".to_string()), Token::Op(Op::Plus), Token::Value("2".to_string())]) ; "addition with spaces")]
    #[test_case("2+2" => Ok(vec![Token::Value("2".to_string()), Token::Op(Op::Plus), Token::Value("2".to_string())]) ; "addition without spaces")]
    #[test_case("10-3" => Ok(vec![Token::Value("10".to_string()), Token::Op(Op::Minus), Token::Value("3".to_string())]) ; "subtraction without spaces")]
    #[test_case("1e-3" => Ok(vec![Token::Value("1e-3".to_string())]) ; "scientific notation with negative exponent")]
    #[test_case("2.5E+2" => Ok(vec![Token::Value("2.5E+2".to_string())]) ; "scientific notation with positive exponent")]
    fn tokenize_infix(equation: &str) -> Result<Vec<Token>, Error> {
        Ok(Lexer::new(equation)
            .tokenize()?
            .into_iter()
            .map(|st| st.token)
            .collect())
    }

    #[test]
    fn tokenize_spans_cover_source() {
        let tokens = Lexer::new("a+b").tokenize().unwrap();
        assert_eq!(tokens[0].span.start, 0);
        assert_eq!(tokens[0].span.end, 1);
        assert_eq!(tokens[1].token, Token::Op(Op::Plus));
        assert_eq!(tokens[2].span.start, 2);
    }

    #[test]
    fn idents() {
        let variable_starts = ['c', 'Z', '_', 'f'];
        for c in &variable_starts {
            assert!(is_variable_start(*c));
        }

        let non_variable_starts = ['3', '\u{e0}', '@', ']', '[', '.'];
        for c in &non_variable_starts {
            assert!(!is_variable_start(*c));
        }

        let variable_parts = ['c', 'Z', '_', 'f', '3', '[', ']', '.'];
        for c in &variable_parts {
            assert!(is_variable_part(*c));
        }

        let non_variable_parts = ['\u{e0}', '@', '{', '}', '(', ')', '?', '&', '$'];
        for c in &non_variable_parts {
            assert!(!is_variable_part(*c));
        }

        let variables = [
            "_______",
            "abc",
            "ab.gd_hc",
            "abc[2as]",
            "abc[",
            "abc]",
            "abc[90]",
            "ab.c[90]",
            "a__45__bc",
        ];
        for v in &variables {
            assert!(is_variable(v));
        }

        let non_variables = ["a-bc", "@bc", "6bc"];
        for nv in &non_variables {
            assert!(!is_variable(nv));
        }
    }

    #[test]
    fn values() {
        let value_starts = ['c', 'Z', '_', 'f', '3'];
        for c in &value_starts {
            assert!(is_value_start(*c));
        }

        let non_value_starts = ['\u{e0}', '@', '+', '-'];
        for c in &non_value_starts {
            assert!(!is_value_start(*c));
        }

        let value_parts = ['c', 'Z', '_', 'f', '3', '.'];
        for c in &value_parts {
            assert!(is_value_part(*c));
        }

        let non_value_parts = ['\u{e0}', '@', '+', '-'];
        for c in &non_value_parts {
            assert!(!is_value_part(*c));
        }
    }
}
