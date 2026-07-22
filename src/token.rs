use crate::error::Span;

/// Possible tokens to find in the input string
#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    /// Any literal: value or variables
    Value(String),
    /// A boolean operator
    Op(Op),
    /// Left parenthesis
    LParen,
    /// Right parenthesis
    RParen,
}

/// A token with its byte span in the source expression.
#[derive(Debug, Clone, PartialEq)]
pub struct SpannedToken {
    pub token: Token,
    pub span: Span,
}

/// Allowed operators in the algorithm
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Op {
    Plus,
    Minus,
    Mul,
    Div,
    Exp,
}

impl Op {
    /// Get the operator precedence. Operators with higher precedence should be
    /// evaluated first.
    pub fn precedence(self) -> u8 {
        match self {
            Self::Plus | Self::Minus => 1,
            Self::Mul | Self::Div => 2,
            Self::Exp => 3,
        }
    }

    /// Check if the operator is left associative
    pub fn is_left_associative(self) -> bool {
        match self {
            Self::Plus | Self::Minus | Self::Mul | Self::Div => true,
            Self::Exp => false,
        }
    }

    /// Check if the operator is right associative
    pub fn is_right_associative(self) -> bool {
        !self.is_left_associative()
    }

    /// Pratt binding powers `(left_bp, right_bp)`.
    pub fn binding_power(self) -> (u8, u8) {
        match self {
            Self::Plus | Self::Minus => (1, 2),
            Self::Mul | Self::Div => (3, 4),
            // Right-associative: right bp lower than left.
            Self::Exp => (6, 5),
        }
    }
}
