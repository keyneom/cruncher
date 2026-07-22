use std::error;
use std::fmt::{self, Display, Formatter};

/// Byte span into the original expression source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Span {
    /// Inclusive start byte offset.
    pub start: usize,
    /// Exclusive end byte offset.
    pub end: usize,
}

impl Span {
    #[must_use]
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    #[must_use]
    pub fn merge(self, other: Self) -> Self {
        Self {
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }
}

/// Error types for cruncher
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// Error while parsing an expression
    ParseError {
        message: String,
        span: Option<Span>,
    },
    /// Unknown variable or function during evaluation
    NameError {
        message: String,
        span: Option<Span>,
    },
}

impl Error {
    #[must_use]
    pub fn parse(message: impl Into<String>) -> Self {
        Self::ParseError {
            message: message.into(),
            span: None,
        }
    }

    #[must_use]
    pub fn parse_at(message: impl Into<String>, span: Span) -> Self {
        Self::ParseError {
            message: message.into(),
            span: Some(span),
        }
    }

    #[must_use]
    pub fn name(message: impl Into<String>) -> Self {
        Self::NameError {
            message: message.into(),
            span: None,
        }
    }

    #[must_use]
    pub fn name_at(message: impl Into<String>, span: Span) -> Self {
        Self::NameError {
            message: message.into(),
            span: Some(span),
        }
    }

    #[must_use]
    pub fn message(&self) -> &str {
        match self {
            Self::ParseError { message, .. } | Self::NameError { message, .. } => message,
        }
    }

    #[must_use]
    pub fn span(&self) -> Option<Span> {
        match self {
            Self::ParseError { span, .. } | Self::NameError { span, .. } => *span,
        }
    }
}

impl Display for Error {
    fn fmt(&self, fmt: &mut Formatter) -> fmt::Result {
        match self {
            Self::ParseError { message, span } => {
                write!(fmt, "ParseError: {}", message)?;
                if let Some(span) = span {
                    write!(fmt, " [bytes {}..{}]", span.start, span.end)?;
                }
                Ok(())
            }
            Self::NameError { message, span } => {
                write!(fmt, "NameError: {}", message)?;
                if let Some(span) = span {
                    write!(fmt, " [bytes {}..{}]", span.start, span.end)?;
                }
                Ok(())
            }
        }
    }
}

impl error::Error for Error {}
