/// Parse an Apple ][ 8-bit disk image
///
/// There are several methods used to determine the disk image format
/// for Apple disks.
/// First, there are several common file extensions and file sizes.
///
/// If a file has a dsk extension, it may be an Apple DOS image
/// If the file size is 143360 bytes, it's a 140K floppy.
///
/// If the file has a nib extension, it's likely a Nibble format disk
///
#[warn(missing_docs)]
#[warn(unsafe_code)]

/// Disk-level functions and data structures for Apple disks.
pub mod disk;

/// Catalog parsing functions and strutures
pub mod catalog;

/// Nibble decoding and encoding routines
pub mod nibble;
