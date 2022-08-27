//! Serializer trait and functions to help serialize images to vectors
//! of bytes and other data types.
use crate::error::Error;
use std::result::Result;

/// Serializer is a trait that lets you build custom serializers for
/// structures.
pub trait Serializer<'a> {
    /// Serialize a structure to a vector of bytes
    fn as_vec(&'a self) -> Result<Vec<u8>, Error>;
}

impl<'a> Serializer<'a> for Vec<u8> {
    fn as_vec(&'a self) -> Result<Vec<u8>, Error> {
        Ok(self.to_vec())
    }
}

/// Convert a 16-bit word to a little-endian pair of bytes
pub fn little_endian_word_to_bytes(word: u16) -> Vec<u8> {
    let bytes: Vec<u8> = vec![(word & 0xFF) as u8, ((word >> 8) & 0xFF) as u8];

    bytes
}
