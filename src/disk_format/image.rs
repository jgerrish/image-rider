use log::info;
use nom::branch::alt;
use nom::combinator::map;
use nom::IResult;
/// Generic image formater parser
/// Parses a variety of disk image, ROM and other binary formats
use std::fmt::{Display, Formatter, Result};

use crate::disk_format::d64::{d64_disk_parser, D64Disk};
use crate::disk_format::stx::{stx_disk_parser, STXDisk};

/// The different kinds of disk images
pub enum DiskImage<'a, 'b> {
    /// A Commodore 64 D64 Disk Image
    D64(D64Disk<'b>),
    /// An Atari ST STX Disk Image
    STX(STXDisk<'a>),
}

// Display a DiskImage
impl Display for DiskImage<'_, '_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        match self {
            DiskImage::D64(d) => write!(f, "D64 Disk: {}", d),
            DiskImage::STX(d) => write!(f, "STX Disk: {}", d),
        }
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

/// Function to collect the actual disk image data from a disk image and return
/// it as a &[u8]
pub fn disk_image_data(disk_image: DiskImage) -> Option<Vec<u8>> {
    let mut disk_image_data = Vec::new();

    match disk_image {
        DiskImage::STX(image_data) => {
            for track in image_data.stx_tracks {
                if let Some(sector_data) = track.sector_data {
                    // TODO: this could be simplified with iterator/fold
                    for sector in sector_data {
                        for byte in sector {
                            disk_image_data.push(*byte);
                        }
                    }
                }
            }
            Some(disk_image_data)
        }
        _ => {
            info!("Unsupported image for file saving");
            None
        }
    }
}
