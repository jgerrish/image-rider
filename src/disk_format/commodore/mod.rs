//! Parse a Commodore disk image
//!
//! This parses Commodore disk images.
//!
//! Currently this includes support for parsing D64 disk images.
//!
#[warn(missing_docs)]
#[warn(unsafe_code)]

/// Disk-level functions and data structures for D64 disks.
pub mod d64;

/// DiskImageGuess trait implementation heuristics for guessing the
/// format of a disk and DiskImage trait implementations
pub mod disk;
