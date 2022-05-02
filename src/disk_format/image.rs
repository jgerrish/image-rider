use config::Config;
use log::info;

use nom::branch::alt;
use nom::combinator::map;
use nom::IResult;
/// Generic image formater parser
/// Parses a variety of disk image, ROM and other binary formats
use std::fmt::{Display, Formatter, Result};

use crate::disk_format::apple::{
    self,
    disk::{apple_disk_parser, AppleDisk, AppleDiskGuess},
};
use crate::disk_format::d64::{d64_disk_parser, D64Disk};
use crate::disk_format::stx::disk::{stx_disk_parser, STXDisk};

/// The different kinds of disk images
pub enum DiskImage<'a, 'b> {
    /// A Commodore 64 D64 Disk Image
    D64(D64Disk<'b>),
    /// An Atari ST STX Disk Image
    STX(STXDisk<'a>),
    /// An Apple ][ Disk Image
    Apple(AppleDisk<'a>),
}

/// Display a DiskImage
impl Display for DiskImage<'_, '_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        match self {
            DiskImage::D64(d) => write!(f, "D64 Disk: {}", d),
            DiskImage::STX(d) => write!(f, "STX Disk: {}", d),
            DiskImage::Apple(d) => write!(f, "Apple Disk: {}", d),
        }
    }
}

/// The result of heuristics to guess a disk image
/// Certain disk images can be guessed accurately based on filenames
/// This returns a guess that can be used to guide the parsing process
/// Later versions can include a parser generator trait that returns the recommended
/// parser
pub enum DiskImageGuess {
    /// A Commodore D64 Disk Image
    D64,
    /// An Atari ST STX Disk Image
    STX,
    /// An Apple ][ Disk Image
    Apple(AppleDiskGuess),
}

/// Display a DiskImageGuess
impl Display for DiskImageGuess {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        match self {
            DiskImageGuess::D64 => write!(f, "D64 Disk"),
            DiskImageGuess::STX => write!(f, "STX Disk"),
            DiskImageGuess::Apple(d) => write!(f, "Apple Disk: {}", d),
        }
    }
}

/// Parses a file given a filename, returning a DiskImage
pub fn file_parser<'a>(
    filename: &str,
    data: &'a [u8],
    config: &Config,
) -> IResult<&'a [u8], DiskImage<'a, 'a>> {
    let guess_image_type = format_from_filename(filename);

    info!(
        "config ignore-checksums: {:?}",
        config.get_bool("ignore-checksums")
    );

    match guess_image_type {
        Some(i) => match i {
            DiskImageGuess::Apple(guess) => {
                let result = apple_disk_parser(Some(guess), config)(data)?;
                Ok((result.0, DiskImage::Apple(result.1)))
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

/// Guess an image format from a filename
pub fn format_from_filename(filename: &str) -> Option<DiskImageGuess> {
    // TODO: format_from_filename should be defined by a trait, and
    // each module should expose a type that implements that trait
    let apple_res = apple::disk::format_from_filename(filename);
    apple_res.map(DiskImageGuess::Apple)
    // match apple_res {
    //     None => None,
    //     Some(res) => Some(DiskImageGuess::Apple(res)),
    // }
}

/// Function to collect the actual disk image data from a disk image and return
/// it as an Option<Vec<u8>>
/// It should have more tests around the different disk types
pub fn disk_image_data(disk_image: DiskImage) -> Option<Vec<u8>> {
    match disk_image {
        DiskImage::STX(image_data) => {
            // It may be more efficient to return sector-size &[u8] iterators
            Some(
                image_data
                    .stx_tracks
                    .iter()
                    .filter(|s| s.sector_data.is_some())
                    .flat_map(|s| (*s).sector_data.as_ref().unwrap().iter())
                    .flat_map(|bytes| (*bytes).iter())
                    .copied()
                    .collect(),
            )
            // For readability comparison:
            // for track in image_data.stx_tracks {
            //     if let Some(sector_data) = track.sector_data {
            //         for sector in sector_data {
            //             for byte in sector {
            //                 disk_image_data.push(*byte);
            //             }
            //         }
            //     }
            // }
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
    use super::{format_from_filename, DiskImageGuess};

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

        let guess = format_from_filename(filename).unwrap_or_else(|| {
            panic!("Invalid filename guess");
        });

        match guess {
            DiskImageGuess::Apple(g) => {
                assert_eq!(
                    g,
                    AppleDiskGuess {
                        encoding: Encoding::Plain,
                        format: Format::DOS(143360),
                    }
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
