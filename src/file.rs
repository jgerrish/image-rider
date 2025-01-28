//! Functions for dealing with files
#![warn(missing_docs)]
#![warn(unsafe_code)]

use std::{fs, result::Result};

use crate::error::Error;

/// Open up a file and read in the data
///
/// # Arguments
///
/// * `filename` - A string reference to a filename to open and read
///
/// # Returns
///   Returns all the data as a u8 vector
///      Returns an Err result if there was an error reading the file.
///      The Err type is an image_rider::error::Error
///      Returns an Ok result with a u8 vector if reading the file was
///      successful.
///
/// # Examples
///
/// // Start of setup code
/// use std::path::Path;
/// use std::io::{Read, Write};
/// use std::fs::{File, OpenOptions};
/// use image_rider::config::{Config, Configuration};
/// use image_rider::disk_format::image::DiskImageParser;
/// use image_rider::file::read_file;
///
/// let filename = "parse_disk_image-tmpfile-file-read_file.img";
/// let path = Path::new(&filename);
///
/// // Create a new scope for the file new operation
/// {
///     let mut file = OpenOptions::new()
///         .create(true)
///         .write(true)
///         .open(path)
///         .unwrap_or_else(|e| {
///             panic!("Couldn't open file: {}", e);
///         });
///     file.write(&[1, 2, 3, 4]);
/// }
/// // End of the setup code
///
/// let data = read_file(filename);
/// assert!(data.is_ok());
/// assert_eq!(data.unwrap(), vec![1, 2, 3, 4]);
///
/// // Teardown code
/// std::fs::remove_file(filename).unwrap_or_else(|e| {
///         panic!("Error removing test file: {}", e);
/// });
///
/// ```
pub fn read_file(filename: &str) -> Result<Vec<u8>, Error> {
    Ok(fs::read(filename)?)
}
