//! Heuristics for identifying a Commodore disk image
//!
//! Currently this includes support for identifying D64 disk images.
//!
#![warn(missing_docs)]
#![warn(unsafe_code)]
use log::error;
use std::{
    fmt::{Display, Formatter, Result},
    fs,
    path::Path,
};

use crate::{
    config::Config,
    disk_format::commodore::d64::d64_disk_parser,
    disk_format::image::{DiskImage, DiskImageGuess, DiskImageGuesser},
    error::{Error, ErrorKind, InvalidErrorKind},
};

/// The format of the disk image
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Format {
    /// A D64 disk image
    D64(u64),
}

impl Display for Format {
    fn fmt(&self, f: &mut Formatter) -> Result {
        write!(f, "{:?}", self)
    }
}

/// Heuristic guesses for what kind of disk this is
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CommodoreDiskGuess<'a> {
    /// The disk format
    pub format: Format,
    /// The raw image data
    pub data: &'a [u8],
}

impl<'a> CommodoreDiskGuess<'a> {
    /// Return a new CommodoreDiskGuess with some default parameters
    /// that can't be easily guessed from basic heuristics like
    /// filename
    pub fn new(format: Format, data: &'a [u8]) -> CommodoreDiskGuess {
        CommodoreDiskGuess { format, data }
    }
}

/// Format a CommodoreDiskGuess for display
impl Display for CommodoreDiskGuess<'_> {
    fn fmt(&self, f: &mut Formatter) -> Result {
        write!(f, "format: {}", self.format)
    }
}

/// Try to guess a file format from a filename
impl<'a, 'b> DiskImageGuesser<'a, 'b> for CommodoreDiskGuess<'a> {
    fn guess(_config: &Config, filename: &str, data: &'a [u8]) -> Option<DiskImageGuess<'a>> {
        // TODO: Abstract this to a common higher-level module
        let filename_extension: Vec<_> = filename.split('.').collect();
        let path = Path::new(&filename);

        let filesize = match fs::metadata(path) {
            Ok(metadata) => metadata.len(),
            Err(e) => {
                error!("Couldn't get file metadata: {}", e);
                panic!("Couldn't get file metadata");
            }
        };

        match filename_extension[filename_extension.len() - 1]
            .to_lowercase()
            .as_str()
        {
            "d64" => Some(DiskImageGuess::Commodore(CommodoreDiskGuess::new(
                Format::D64(filesize),
                data,
            ))),
            &_ => None,
        }
    }

    fn parse(
        &'b self,
        config: &'a crate::config::Config,
    ) -> std::result::Result<DiskImage<'a>, Error> {
        let result = d64_disk_parser(config)(self.data);
        match result {
            Ok(res) => Ok(DiskImage::D64(res.1)),
            Err(e) => Err(Error::new(ErrorKind::Invalid(InvalidErrorKind::Invalid(
                nom::Err::Error(e).to_string(),
            )))),
        }
    }
}
