#![warn(missing_docs)]
#![warn(unsafe_code)]
//!
//! Disk format parsers
//!

/// Sanity checking trait
pub mod sanity_check;

/// image parser, parses disk images and ROM images
pub mod image;

/// Commodore disk images
pub mod commodore;

/// STX disk images
pub mod stx;

/// Apple disk images
pub mod apple;
