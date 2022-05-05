use config::Config;

use log::{debug, error, info};

use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

use nom::bytes::complete::{tag, take};
use nom::number::complete::{le_u16, le_u8};
use nom::IResult;

use std::fmt::{Display, Formatter, Result};

use crate::disk_format::image::{DiskImage, DiskImageParser};
use crate::disk_format::stx::track::{stx_tracks_parser, STXTrack};
use crate::disk_format::stx::SanityCheck;

/// A STX disk image
#[derive(Debug)]
pub struct STXDisk<'a> {
    /// The disk header
    pub stx_disk_header: STXDiskHeader<'a>,

    /// The disk tracks
    pub stx_tracks: Vec<STXTrack<'a>>,
}

/// Format a STXDisk for display
impl Display for STXDisk<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        write!(f, "header: {}", self.stx_disk_header)
    }
}

impl DiskImageParser for STXDisk<'_> {
    fn parse_disk_image<'a>(
        _config: &Config,
        _filename: &str,
        data: &'a [u8],
    ) -> IResult<&'a [u8], DiskImage<'a>> {
        let (i, parse_result) = stx_disk_parser(data)?;

        Ok((i, DiskImage::STX(parse_result)))
    }

    fn save_disk_image(&self, _config: &Config, filename: &str) {
        // It may be more efficient to return sector-size &[u8] iterators
        let disk_image_data: Vec<u8> = self
            .stx_tracks
            .iter()
            .filter(|s| s.sector_data.is_some())
            .flat_map(|s| (*s).sector_data.as_ref().unwrap().iter())
            .flat_map(|bytes| (*bytes).iter())
            .copied()
            .collect();
        info!("Found image data, writing data");
        let filename = PathBuf::from(filename);
        let file_result = File::create(filename);
        match file_result {
            Ok(mut file) => {
                let _res = file.write_all(&disk_image_data);
            }
            Err(e) => error!("Error opening file: {}", e),
        }
    }
}

/// STXDiskHeader contains information about an Atari ST STX floppy disk image header
/// 16 bytes
#[derive(Debug)]
pub struct STXDiskHeader<'a> {
    /// The disk identifier, "RSY\0"
    pub disk_id: &'a [u8],
    /// The version of the disk image
    /// Usually version 3
    pub version: u16,
    /// A 16-bit integer that indicates the tool used to create the image
    pub tool_used: u16,
    /// First reserved area, two reserved bytes
    pub reserved_area_1: &'a [u8],
    /// The number of tracks on the disk
    pub track_count: u8,
    /// Whether the disk is in the new format
    pub new_format: u8,
    /// Second reserved area, four reserved bytes
    pub reserved_area_2: &'a [u8],
}

/// Perform sanity checks for a disk header
/// For now, these are done post-parsing of the section
/// These are generally less strict than things like magic number identification
/// but are good indicators the data may be corrupted
impl SanityCheck for STXDiskHeader<'_> {
    fn check(&self) -> bool {
        if self.track_count > 164 {
            debug!("Disk track count is greater than 164: {}", self.track_count);
            false
        } else {
            true
        }
    }
}

/// Format a STXDiskHeader for display
impl Display for STXDiskHeader<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        write!(
            f,
            "version: {}, tool_used: {}, reserved_area_1: {:?}, track_count: {}, ",
            self.version, self.tool_used, self.reserved_area_1, self.track_count
        )?;
        write!(
            f,
            "new_format: {}, reserved_area_2: {:?}",
            self.new_format, self.reserved_area_2
        )
    }
}

/// The track or sector image data can be located in several places, depending on the
/// fuzzy masks and track flags
pub fn stx_disk_parser(i: &[u8]) -> IResult<&[u8], STXDisk> {
    let (i, stx_disk_header) = stx_disk_header_parser(i)?;

    if !stx_disk_header.check() {
        error!("Invalid data");
        panic!("Invalid data");
    }

    info!("Disk header: {}", stx_disk_header);

    let (i, tracks) = stx_tracks_parser(stx_disk_header.track_count as usize)(i)?;

    let stx_disk = STXDisk {
        stx_disk_header,
        stx_tracks: tracks,
    };

    Ok((i, stx_disk))
}

// TODO: Verify that this is reading correctly
/// Parse STX disks
pub fn stx_disk_header_parser(i: &[u8]) -> IResult<&[u8], STXDiskHeader> {
    // will consume bytes if the input begins with "RSY" + 0
    // magic number
    let (i, disk_id) = tag("RSY\0")(i)?;

    // version
    let (i, version) = le_u16(i)?;

    // tool used
    let (i, tool_used) = le_u16(i)?;

    // 2 reserved bytes
    let (i, reserved_area_1) = take(2_usize)(i)?;

    let (i, track_count) = le_u8(i)?;
    let (i, new_format) = le_u8(i)?;

    // 4 reserved bytes
    let (i, reserved_area_2) = take(4_usize)(i)?;

    let stx_disk_header = STXDiskHeader {
        disk_id,
        version,
        tool_used,
        reserved_area_1,
        track_count,
        new_format,
        reserved_area_2,
    };

    Ok((i, stx_disk_header))
}

#[cfg(test)]
mod tests {
    use super::stx_disk_header_parser;

    /// Test parsing a STX disk header
    #[test]
    fn stx_disk_valid_header_parser_works() {
        // Standard "public tool" / stock Atari ST image
        // version 3, tool 1, 82 tracks, new format 2
        let stx_disk_header: [u8; 16] = [
            0x52, 0x53, 0x59, 0x00, 0x03, 0x00, 0x01, 0x00, 0x00, 0x00, 0x52, 0x02, 0x00, 0x00,
            0x00, 0x00,
        ];

        let stx_disk_header_parser_result = stx_disk_header_parser(&stx_disk_header);

        match stx_disk_header_parser_result {
            Ok((_, res)) => {
                assert_eq!(res.disk_id, [0x52, 0x53, 0x59, 0x00]);
                assert_eq!(res.version, 0x03);
                assert_eq!(res.tool_used, 0x01);
                assert_eq!(res.reserved_area_1, [0x00, 0x00]);
                assert_eq!(res.track_count, 0x52);
                assert_eq!(res.new_format, 0x02);
                assert_eq!(res.reserved_area_2, [0x00, 0x00, 0x00, 0x00]);
            }
            Err(e) => panic!("Parsing failed on the STX disk header: {}", e),
        }
    }

    /// Test parsing an invalid STX disk header
    #[test]
    #[should_panic(
        expected = "Parsing failed on the STX disk header: Parsing Error: Error { input: [82, 83, 96, 0, 3, 0, 1, 0, 0, 0, 82, 2, 0, 0, 0, 0], code: Tag }"
    )]
    fn stx_disk_invalid_header_parser_fails() {
        // Standard "public tool" / stock Atari ST image
        // version 3, tool 1, 82 tracks, new format 2
        // invalid magic number
        let stx_disk_header: [u8; 16] = [
            0x52, 0x53, 0x60, 0x00, 0x03, 0x00, 0x01, 0x00, 0x00, 0x00, 0x52, 0x02, 0x00, 0x00,
            0x00, 0x00,
        ];

        let stx_disk_header_parser_result = stx_disk_header_parser(&stx_disk_header);

        match stx_disk_header_parser_result {
            Ok((_, _)) => panic!("Should fail parsing"),
            Err(e) => panic!("Parsing failed on the STX disk header: {}", e),
        }
    }
}
