//! Error results that can occur working with images
use std::{
    fmt::{Debug, Display, Formatter, Result},
    io,
};

/// An error that can occur when processing an image, ROM or other
/// file.
pub struct Error {
    kind: ErrorKind,
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

impl std::error::Error for Error {}

impl Error {
    /// Create a new Error with a given ErrorKind variant
    pub fn new(kind: ErrorKind) -> Error {
        Error { kind }
    }
}

/// The kinds of errors that can occur when processing an image, ROM
/// or other file.
pub enum ErrorKind {
    /// Generic error type
    Message(String),

    /// An error that occurs while reading or writing image data.
    Io(io::Error),

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
}

impl Display for ErrorKind {
    fn fmt(&self, f: &mut Formatter) -> Result {
        match self {
            ErrorKind::Message(message) => write!(f, "An error occurred: {}", message),
            ErrorKind::Io(e) => write!(f, "{}", e),
            ErrorKind::Invalid(e) => write!(f, "{}", e),
            ErrorKind::Unimplemented(message) => {
                write!(f, "Unimplemented feature: {}", message)
            }
            ErrorKind::NotFound(message) => {
                write!(f, "Data not found: {}", message)
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
