#![warn(missing_docs)]
#![warn(unsafe_code)]
//! image_rider is a library crate to parse disk and ROM images.  It
//! primarily focuses on older 8-bit and 16-bit systems.
//!
//! The primary API for this library are a set of traits in the
//! [image_rider::disk_format::image](crate::disk_format::image) module.
//!
//! The disk_format module contains everything to parse disk formats
//!
use log::error;

pub mod config;
pub mod disk_format;
pub mod error;
pub mod serialize;

/// Initialize the module.
/// This should be called before any parsing is performed.
/// Panics on failure or if there are any incompatibilities.
pub fn init() {
    // If we're on a system with a usize < 32 bits then fail.  This
    // crate is geared towards parsing file formats for 8-bit systems,
    // but the code currently does not run on 8-bit systems.  For
    // example, we read the entire file into a single image data array
    // and access the data array with usize indexes for several of the
    // file formats.
    if usize::BITS < 32 {
        error!(
            "Architecture usize {} is too small for this library",
            usize::BITS
        );
        panic!(
            "Architecture usize {} is too small for this library",
            usize::BITS
        );
    }
}
