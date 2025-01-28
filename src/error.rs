//! Error results that can occur working with images
#![warn(missing_docs)]
#![warn(unsafe_code)]
use std::{
    fmt::{Debug, Display, Formatter, Result},
    io,
};

/// An error that can occur when processing an image, ROM or other
/// file.
pub struct Error {
    kind: ErrorKind,
    // reqwest (0.12.12) has a fairly good Error implementation
    // showing how to do source with boxes.
    // source: String,
}

impl Debug for Error {
    fn fmt(&self, f: &mut Formatter) -> Result {
        write!(f, "{}", self.kind)
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter) -> Result {
        write!(f, "{}", self.kind)
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self.kind {
            ErrorKind::IO(ref io) => Some(io),
            _ => None,
        }
    }
}

impl Error {
    /// Create a new Error with a given ErrorKind variant
    pub fn new(kind: ErrorKind) -> Error {
        Error { kind }
    }
}

impl From<nom::Err<nom::error::Error<&[u8]>>> for Error {
    fn from(e: nom::Err<nom::error::Error<&[u8]>>) -> Self {
        Error::new(ErrorKind::new(&e.to_string()))
    }
}

// impl From<nom::Err<nom::error::ParseError<&[u8]>>> for Error {
//     fn from(e: nom::Err<nom::error::Error<&[u8]>>) -> Self {
//         Error::new(ErrorKind::new(&e.to_string()))
//     }
// }

impl<'a> nom::error::ParseError<&'a [u8]> for Error {
    fn from_error_kind(_input: &'a [u8], kind: nom::error::ErrorKind) -> Self {
        Error::new(ErrorKind::new(kind.description()))
    }

    fn append(_input: &'a [u8], _kind: nom::error::ErrorKind, other: Self) -> Self {
        other
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::new(ErrorKind::IO(e))
    }
}

/// The kinds of errors that can occur when processing an image, ROM
/// or other file.
#[derive(Debug)]
pub enum ErrorKind {
    /// Generic error type
    Message(String),

    /// An error that occurs when dealing with invalid or unexpected
    /// data.
    Invalid(InvalidErrorKind),

    /// The file was in a format that is unsupported or has
    /// unsupported features.
    Unimplemented(String),

    /// The data requested was not found in the image.  This can occur
    /// when attempting to extract a specific file from a file, or
    /// when attempting to extract a certain sector or other item.
    NotFound(String),

    /// An IO based error
    // goblin (0.8.2) has an implementation using Error enums to store
    // the source error
    IO(io::Error),
}

impl Display for ErrorKind {
    fn fmt(&self, f: &mut Formatter) -> Result {
        match self {
            ErrorKind::Message(message) => write!(f, "An error occurred: {}", message),
            ErrorKind::Invalid(e) => write!(f, "{}", e),
            ErrorKind::Unimplemented(message) => {
                write!(f, "Unimplemented feature: {}", message)
            }
            ErrorKind::NotFound(message) => {
                write!(f, "Data not found: {}", message)
            }
            ErrorKind::IO(io) => {
                write!(f, "IO error: {}", io)
            }
        }
    }
}

impl ErrorKind {
    /// Return a new generic ErrorKind::Message with a given string message.
    pub fn new(message: &str) -> ErrorKind {
        ErrorKind::Message(message.to_string())
    }
}

/// An InvalidErrorKind is returned when the data is invalid.
#[derive(Debug, Eq, PartialEq)]
pub enum InvalidErrorKind {
    /// The data was invalid
    Invalid(String),
    /// The data contains an invalid checksum
    Checksum,
}

impl Display for InvalidErrorKind {
    fn fmt(&self, f: &mut Formatter) -> Result {
        match self {
            InvalidErrorKind::Invalid(message) => write!(f, "Image is invalid: {}", message),
            InvalidErrorKind::Checksum => write!(f, "Image has an invalid checksum"),
        }
    }
}

/// Test the error code
#[allow(unused_imports)]
#[cfg(test)]
pub mod tests {
    use crate::error::ErrorKind;

    #[test]
    pub fn error_kind_new_works() {
        let ek1 = ErrorKind::new("Test1");
        let ek2 = ErrorKind::new("Test1");

        matches!(ek1, ErrorKind::Message(x) if x == "Test1");
        matches!(ek2, ErrorKind::Message(x) if x == "Test1");
    }

    #[test]
    pub fn error_kind_not_found_works() {
        let ek = ErrorKind::NotFound(String::from("Test1"));
        matches!(ek, ErrorKind::NotFound(x) if x == "Test1");
    }
}
