//! The image_rider::disk_format::image module provides a set of common functions
//! and trait definitions for reading disks and cartridges.
use config::Config;
use log::info;

use nom::branch::alt;
use nom::combinator::map;
use nom::IResult;
use std::fmt::{Display, Formatter, Result};

use crate::{
    disk_format::{
        apple::{
            self,
            disk::{apple_disk_parser, AppleDisk, AppleDiskData, AppleDiskGuess},
        },
        commodore::d64::{d64_disk_parser, D64Disk, D64DiskGuess},
        stx::disk::{stx_disk_parser, STXDisk, STXDiskGuess},
    },
    error::{Error, ErrorKind, InvalidErrorKind},
    init,
};

/// DiskImage is the primary enumeration for holding disk images.
///
/// The DiskImageParser and DiskImageSaver trait functions return and
/// operate on this enumeration.
///
/// Because the Disk data structures are more intelligent than simple
/// byte-oriented C structures, copying them isn't as easy as copying
/// a block of bytes.
/// rust-clippy recommends boxing the large fields to reduce the total size of the enum.
/// This is a new recommendation, we'll ignore it for now and investigate other solution.
/// DiskImage construction is usually done once at the beginning of the program,
/// and total variant size is still around ~512 bytes
/// On normal invocations in the current codebase we only have one
/// instance of this enum.  Future versions may have more, but for now
/// the cost is not an issue.
/// If this code is adapted to process a large number of images and
/// thrashing is a concern, feel free to fix it.
#[allow(clippy::large_enum_variant)]
pub enum DiskImage<'a> {
    /// A Commodore 64 D64 Disk Image
    D64(D64Disk<'a>),
    /// An Atari ST STX Disk Image.
    /// Usually the raw data in a STX disk image is a FAT12 filesystem.
    STX(STXDisk<'a>),
    /// An Apple ][ Disk Image There are several different encodings,
    /// formats, and filesystems for Apple2 disks.  This includes
    /// nibble encoding and DOS 3.x and ProDOS filesystems.
    Apple(AppleDisk<'a>),
}

/// Display a DiskImage
impl Display for DiskImage<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        match self {
            DiskImage::D64(_) => write!(f, "D64 Disk"),
            DiskImage::STX(_) => write!(f, "STX Disk"),
            DiskImage::Apple(d) => write!(f, "Apple Disk: {}", d),
        }
    }
}

/// A trait for disk or ROM image parsers
/// New image guessers should implement this trait
/// It's also implemented for &[u8]
///
/// The disk parsing and the disk image access (data, saving, etc.)
/// functions have been moved into separate traits.  This supports the
/// new data flow in the library.
/// It's assumed that image type is guessed from raw image data and a
/// DiskImageGuess structure is created.
/// From this structure, an image can be parsed.
///
/// It allows easy guiding of the parsing from the command line,
/// just specify the file type on the command line, along with guesses on
/// things like directory table locations and an DiskImageGuess can be generated.
///
/// The DiskImageParser trait and DiskImageSaver trait are the primary
/// API access points to the image-rider crate.  Application
/// developers should use these two traits to load and store data in
/// their application.
///
/// The individual DiskImage structures provide additional fields for
/// users familiar with a specific image type.
pub trait DiskImageParser<'a, 'b> {
    /// This function parses an entire disk, returning a DiskImage
    ///
    /// # Arguments
    ///
    /// - `config` - A Config object that contains information to guide parsing.
    /// - `filename` - The name of the file to parse.
    ///
    /// # Returns
    ///
    /// A Result containing the DiskImage or an Error.
    ///
    /// # Examples
    /// ```no_run
    /// // Start of setup code
    /// use std::path::Path;
    /// use std::io::Read;
    /// use std::fs::{File, OpenOptions};
    /// use config::Config;
    /// use image_rider::disk_format::image::DiskImageParser;
    /// let filename = "parse_disk_image-tmpfile-1234.img";
    /// let path = Path::new(&filename);
    /// let mut file = OpenOptions::new()
    ///     .create(true)
    ///     .write(true)
    ///     .open(path)
    ///     .unwrap_or_else(|e| {
    ///         panic!("Couldn't open file: {}", e);
    ///     });
    /// let data: Vec<u8> = Vec::new();
    /// let settings = Config::builder().build().unwrap();
    /// // End of the setup code
    ///
    /// // The main method call
    /// let result = data.parse_disk_image(&settings, &filename);
    /// if let Ok(disk_image) = result {
    ///     println!("Successful parse");
    /// }
    ///
    /// // Teardown code
    /// std::fs::remove_file(filename).unwrap_or_else(|e| {
    ///         panic!("Error removing test file: {}", e);
    /// });
    ///
    /// ```
    fn parse_disk_image(
        &'a self,
        config: &'b Config,
        filename: &str,
    ) -> std::result::Result<DiskImage<'a>, Error>;
}

/// Test trait for getting parsing and ownership transferral working
/// with DiskImageGuess
pub trait TestParser<'a, 'b> {
    /// Parse an entire disk, returning a DiskImage.
    ///
    /// # Arguments
    ///
    /// - `config` - A Config object that contains information to guide parsing.
    /// - `filename` - The name of the file to parse.
    ///
    /// # Returns
    ///
    /// A Result containing the DiskImage or an error message.
    ///
    fn parse_disk_image(
        self,
        config: &'b Config,
        filename: &str,
    ) -> std::result::Result<DiskImage<'a>, Error>;
}

/// This trait provides convenient functions for getting and saving
/// data for the parsed disk image data in a DiskImage
pub trait DiskImageSaver {
    /// Return the primary data contents of a disk image
    /// The meaning of the data contents will differ between image formats, but
    /// it's usually all the volume, track, and sector data, or the enclosed file format
    /// if the outer image is a wrapper
    // fn disk_image_data(&self, config: &Config) -> Vec<&[u8]>;

    /// Save the primary data contents of a disk image to disk
    /// The meaning of the data contents will differ between image formats, but
    /// it's usually all the volume, track, and sector data, or the enclosed file format
    /// if the outer image is a wrapper.
    /// This function parses an entire disk, returning a DiskImage.
    ///
    /// # Arguments
    ///
    /// - `config` - A Config object that contains information to guide parsing.
    /// - `filename` - The name of the file to parse.
    ///
    /// # Examples
    /// ```no_run
    /// // Start of setup code
    /// use std::path::Path;
    /// use std::io::Read;
    /// use std::fs::{File, OpenOptions};
    /// use config::Config;
    /// use image_rider::disk_format::image::{DiskImageParser, DiskImageSaver};
    /// let filename = "parse_disk_image-tmpfile-1234.img";
    /// let path = Path::new(&filename);
    /// let mut file = OpenOptions::new()
    ///     .create(true)
    ///     .write(true)
    ///     .open(path)
    ///     .unwrap_or_else(|e| {
    ///         panic!("Couldn't open file: {}", e);
    ///     });
    /// let data: Vec<u8> = Vec::new();
    /// let settings = Config::builder().build().unwrap();
    /// // End of the setup code
    ///
    /// // The main method call
    /// let result = data.parse_disk_image(&settings, &filename);
    /// let tmp_out_filename = "parse_disk_image-tmpfile-out-1234.img";
    /// if let Ok(disk_image) = result {
    ///     println!("Successful parse");
    ///     // Save the data
    ///     disk_image.save_disk_image(&settings, None, tmp_out_filename);
    /// }
    ///
    /// // Teardown code
    /// std::fs::remove_file(tmp_out_filename).unwrap_or_else(|e| {
    ///         println!("Error removing test file: {}", e);
    /// });
    /// std::fs::remove_file(filename).unwrap_or_else(|e| {
    ///         println!("Error removing test file: {}", e);
    /// });
    ///
    /// ```
    fn save_disk_image(
        &self,
        config: &Config,
        selected_filename: Option<&str>,
        filename: &str,
    ) -> std::result::Result<(), crate::error::Error>;
}

/// The result of heuristics to guess a disk image
/// Certain disk images can be guessed accurately based on filenames
/// This returns a guess that can be used to guide the parsing process
/// Later versions can include a parser generator trait that returns the recommended
/// parser
/// The DiskImageGuess structures should have a field that contains
/// the raw image data When A DiskImageGuess is created, it becomes
/// the new owner of the image data
pub enum DiskImageGuess<'a> {
    /// A Commodore D64 Disk Image
    D64(D64DiskGuess<'a>),
    /// An Atari ST STX Disk Image
    STX(STXDiskGuess<'a>),
    /// An Apple ][ Disk Image
    Apple(AppleDiskGuess<'a>),
}

/// Display a DiskImageGuess
impl<'a> Display for DiskImageGuess<'a> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        match self {
            DiskImageGuess::D64(_) => write!(f, "D64 Disk"),
            DiskImageGuess::STX(_) => write!(f, "STX Disk"),
            DiskImageGuess::Apple(d) => write!(f, "Apple Disk: {}", d),
        }
    }
}

/// Implement a parser for a DiskImageGuess
/// The intention is that the DiskImage owns the raw data afterwards
impl<'a, 'b> TestParser<'a, 'b> for DiskImageGuess<'a> {
    fn parse_disk_image(
        self,
        config: &'b Config,
        _filename: &str,
    ) -> std::result::Result<DiskImage<'a>, crate::error::Error> {
        // Initialize the image-rider module
        init();

        match self {
            DiskImageGuess::D64(_) => Err(Error::new(ErrorKind::Unimplemented(String::from(
                "Error parsing image from guess",
            )))),
            DiskImageGuess::STX(_) => Err(Error::new(ErrorKind::Unimplemented(String::from(
                "Error parsing image from guess",
            )))),
            DiskImageGuess::Apple(guess) => {
                let parser_result = apple_disk_parser(guess, config);
                match parser_result {
                    Ok(res) => Ok(DiskImage::Apple(res.1)),
                    Err(e) => Err(Error::new(ErrorKind::Invalid(InvalidErrorKind::Invalid(
                        nom::Err::Error(e).to_string(),
                    )))),
                }
            }
        }
    }
}

impl DiskImageSaver for DiskImage<'_> {
    fn save_disk_image(
        &self,
        config: &Config,
        selected_filename: Option<&str>,
        filename: &str,
    ) -> std::result::Result<(), crate::error::Error> {
        match self {
            DiskImage::STX(image_data) => {
                image_data.save_disk_image(config, None, filename)?;
                Ok(())
            }
            DiskImage::Apple(apple_image) => match &apple_image.data {
                AppleDiskData::Nibble(nibble_image) => {
                    nibble_image.save_disk_image(config, None, filename)?;
                    Ok(())
                }
                AppleDiskData::DOS(dos_image) => {
                    info!("Saving DOS 3.3 file");
                    dos_image.save_disk_image(config, selected_filename, filename)?;
                    Ok(())
                }
                _ => {
                    info!("Unsupported image for file saving");
                    Err(crate::error::Error::new(
                        crate::error::ErrorKind::Unimplemented(String::from(
                            "Saving unknown Apple disk images not implemented\n",
                        )),
                    ))
                }
            },
            _ => {
                info!("Unsupported image for file saving");
                Err(crate::error::Error::new(
                    crate::error::ErrorKind::Unimplemented(String::from(
                        "Saving unknown disk images not implemented\n",
                    )),
                ))
            }
        }
    }
}

/// Parses a file given a filename, returning a DiskImage
pub fn file_parser<'a, 'b>(
    filename: &str,
    data: &'a [u8],
    config: &'b Config,
) -> IResult<&'a [u8], DiskImage<'a>> {
    let guess_image_type = format_from_filename_and_data(filename, data);

    info!(
        "config ignore-checksums: {:?}",
        config.get_bool("ignore-checksums")
    );

    match guess_image_type {
        Some(i) => match i {
            DiskImageGuess::Apple(guess) => {
                // Before this can be refactored to the
                // DiskImageParser trait, the code needs to be
                // rewritten to transfer ownership from
                // the DiskImageGuess to the DiskImage
                info!("Attempting to parse Apple disk");
                let res = apple_disk_parser(guess, config)?;
                Ok((res.0, DiskImage::Apple(res.1)))
            }
            _ => panic!("Exiting"),
        },
        None => disk_image_parser(data),
    }
}

/// Parse a disk image
/// This attempts to parse the different file types supported by this library
/// It returns the remaining input and a DiskImage
pub fn disk_image_parser(i: &[u8]) -> IResult<&[u8], DiskImage> {
    // Assume the alt parser is greedy and checks the next parser on the first error
    alt((
        map(d64_disk_parser, DiskImage::D64),
        map(stx_disk_parser, DiskImage::STX),
    ))(i)
}

/// Implementation of DiskImageParser for references to 8-bit integer arrays
// impl<'a, 'b> DiskImageParser<'a, 'b> for &[u8] {
//     fn parse_disk_image(
//         self,
//         config: &'b Config,
//         filename: &str,
//     ) -> IResult<&'a [u8], DiskImage<'a>> {
//         file_parser(filename, self, config)
//     }
// }

/// Implementation of DiskImageParser for 8-bit integer vectors
impl<'a, 'b> DiskImageParser<'a, 'b> for Vec<u8> {
    fn parse_disk_image(
        &'a self,
        config: &'b Config,
        filename: &str,
    ) -> std::result::Result<DiskImage<'a>, Error> {
        // Initialize the image-rider module
        init();

        let result = file_parser(filename, self, config);
        match result {
            Ok(res) => Ok(res.1),
            Err(e) => Err(Error::new(ErrorKind::Invalid(InvalidErrorKind::Invalid(
                nom::Err::Error(e).to_string(),
            )))),
        }
    }
}

/// Guess an image format from a filename.  Builds and returns a
/// DiskImageGuess for a given filename and file data.
///
/// # Arguments
///
/// - `filename` - The name of the file to generate a guess for.
/// - `data` - The disk image data as a reference to a byte array.
///
/// # Returns
///
/// An Option containing the DiskImageGuess
pub fn format_from_filename_and_data<'a>(
    filename: &str,
    data: &'a [u8],
) -> Option<DiskImageGuess<'a>> {
    // TODO: format_from_filename should be defined by a trait, and
    // each module should expose a type that implements that trait
    let apple_res = apple::disk::format_from_filename_and_data(filename, data);
    apple_res.map(DiskImageGuess::Apple)
    // match apple_res {
    //     None => None,
    //     Some(res) => Some(DiskImageGuess::Apple(res)),
    // }
}

/// Function to collect the actual disk image data from a disk image and return
/// it as an Option<Vec<u8>>
/// It should have more tests around the different disk types
pub fn disk_image_data(disk_image: &DiskImage) -> Option<Vec<u8>> {
    match disk_image {
        DiskImage::STX(image_data) => {
            // It may be more efficient to return sector-size &[u8] iterators
            Some(
                image_data
                    .stx_tracks
                    .iter()
                    .filter(|s| s.sector_data.is_some())
                    .flat_map(|s| s.sector_data.as_ref().unwrap().iter())
                    .flat_map(|bytes| (*bytes).iter())
                    .copied()
                    .collect(),
            )
        }
        _ => {
            info!("Unsupported image for file saving");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs::OpenOptions;
    use std::io::Write;
    use std::path::Path;

    use super::apple::disk::{Encoding, Format};
    use super::AppleDiskGuess;
    use super::{format_from_filename_and_data, DiskImageGuess};

    /// Test collecting heuristics on disk image type
    #[test]
    fn format_from_filename_works() {
        let filename = "testdata/test-image_format_from_filename_works.dsk";

        /* Version where we build the file in the test instead of
         * saving it to version control */
        let path = Path::new(&filename);
        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .open(path)
            .unwrap_or_else(|e| {
                panic!("Couldn't open file: {}", e);
            });
        let data: [u8; 143360] = [0; 143360];

        file.write_all(&data).unwrap_or_else(|e| {
            panic!("Error writing test file: {}", e);
        });
        file.flush().unwrap_or_else(|e| {
            panic!("Couldn't flush file stream: {}", e);
        });

        let guess = format_from_filename_and_data(filename, &data).unwrap_or_else(|| {
            panic!("Invalid filename guess");
        });

        match guess {
            DiskImageGuess::Apple(g) => {
                assert_eq!(
                    g,
                    AppleDiskGuess::new(Encoding::Plain, Format::DOS33(143360), &data)
                );
            }
            _ => {
                panic!("Invalid filename guess");
            }
        }

        std::fs::remove_file(filename).unwrap_or_else(|e| {
            panic!("Error removing test file: {}", e);
        });
    }
}
