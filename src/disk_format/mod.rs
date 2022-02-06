///
/// Disk format parsers
///
#[warn(missing_docs)]
#[warn(unsafe_code)]

/// Sanity checking trait
pub mod sanity_check;

/// image parser, parses disk images and ROM images
pub mod image;

/// Commodore D64 disk images
pub mod d64;

/// STX disk images
pub mod stx;
