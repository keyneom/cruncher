use nom::error::ErrorKind;
use std::error;
use std::fmt::{self, Display, Formatter};

/// Error types for cruncher
#[derive(Debug, Clone, PartialEq)]
pub enum Error {
    /// Error while parsing an expression
    ParseError(String),
    /// Unknown variable during evaluation
    NameError(String),
}

impl Display for Error {
    fn fmt(&self, fmt: &mut Formatter) -> fmt::Result {
        match *self {
            Self::ParseError(ref message) => write!(fmt, "ParseError: {}", message),
            Self::NameError(ref message) => write!(fmt, "NameError: {}", message),
        }
    }
}

impl error::Error for Error {
    #[must_use]
    fn description(&self) -> &str {
        match *self {
            Self::ParseError(ref message) | Self::NameError(ref message) => message,
        }
    }

    #[must_use]
    fn cause(&self) -> Option<&dyn error::Error> {
        match *self {
            Self::ParseError(_) | Self::NameError(_) => None,
        }
    }
}

impl nom::error::ParseError<&str> for Error {
    fn from_error_kind(input: &str, kind: ErrorKind) -> Error {
        Error::ParseError(format!(
            "Error during parsing with ErrorKind: {} and equation: {}",
            kind.description(),
            input
        ))
    }

    fn append(input: &str, kind: ErrorKind, other: Self) -> Self {
        Error::ParseError(format!(
            "Error during parsing with ErrorKind: {} and equation: {}
{}",
            kind.description(),
            input,
            other
        ))
    }
}
