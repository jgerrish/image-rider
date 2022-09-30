use config::Config;
use log::debug;
use nom::bytes::complete::{tag, take};
use nom::combinator::{map, verify};
use nom::multi::count;
use nom::number::complete::{le_u16, le_u8};
use nom::IResult;
/// Parse a Commodore D64 disk image
use std::fmt::{Display, Formatter, Result};

use crate::disk_format::image::DiskImageSaver;
use crate::disk_format::sanity_check::SanityCheck;

/// A Commodore D64 disk
pub struct D64Disk<'a> {
    /// The D64 Block Availability Map
    pub bam: D64BlockAvailabilityMap<'a>,
}

/// Display a Commodore D64 disk
impl Display for D64Disk<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        write!(f, "{}", self.bam)
    }
}

/// The different DOS types
#[derive(Debug)]
pub enum DOSType {
    /// Original CBM DOS
    CBM,
}

/// Heuristic guesses for what kind of disk this is
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct D64DiskGuess<'a> {
    /// The raw image data
    pub data: &'a [u8],
}

/// The Block Availability Map (BAM) lives at track 18, which is at offset 0x16500
pub struct D64BlockAvailabilityMap<'a> {
    /// The first byte is the track of the first directory sector
    pub first_directory_sector_track: u8,
    /// The second byte is the sector of the first directory sector
    pub first_directory_sector_sector: u8,
    /// The third byte is the DOS version type
    pub disk_dos_version: u8,

    /// reserved byte
    pub reserved: u8,

    /// 140 bytes of BAM entries for each track (offset 0x04 to 0x8f)
    /// four bytes per track, starting at track one
    pub bam_entries: Vec<D64BAMEntry<'a>>,

    /// The disk name, 16 bytes, padded with 0xA0
    pub disk_name: &'a [u8],

    /// two reserved bytes
    pub second_reserved: &'a [u8],

    /// disk id
    pub disk_id: u16,

    /// reserved byte, usually 0xA0
    pub third_reserved: u8,

    /// DOS type, usually "2A" aka CBM DOS
    pub dos_type: DOSType,
}

/// A single Block Availability Map entry
pub struct D64BAMEntry<'a> {
    /// Number of free sectors on track
    pub free_sectors_on_track: u8,

    /// The sector use bitmap, 3 bytes or 24 bits
    pub sector_use_bitmap: &'a [u8],
}

/// Parse an entry in the Block Availability Map table
pub fn bam_entry_parser(i: &[u8]) -> IResult<&[u8], D64BAMEntry> {
    let (i, free_sectors_on_track) = le_u8(i)?;

    let (i, sector_use_bitmap) = take(3_usize)(i)?;

    Ok((
        i,
        D64BAMEntry {
            free_sectors_on_track,
            sector_use_bitmap,
        },
    ))
}

impl Display for D64BlockAvailabilityMap<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        write!(f, "D64 Block Availability Map: ")?;
        write!(
            f,
            "first_directory_sector_track: 0x{:02X}, ",
            self.first_directory_sector_track
        )?;
        write!(
            f,
            "first_directory_sector_sector: 0x{:02X}, ",
            self.first_directory_sector_sector
        )?;
        write!(
            f,
            "first_directory_sector_track: 0x{:02X}, ",
            self.disk_dos_version
        )?;
        write!(
            f,
            "disk_name: {}, ",
            String::from_utf8_lossy(self.disk_name)
        )?;
        write!(f, "disk_id: {}, ", self.disk_id)?;
        write!(f, "dos_type: {:?}", self.dos_type)
    }
}

/// Perform sanity checks for DOS 2.x boot sectors
impl SanityCheck for D64BlockAvailabilityMap<'_> {
    fn check(&self) -> bool {
        if self.disk_dos_version != 0x41 {
            debug!(
                "disk dos version should be 0x41: 0x{:02X}",
                self.disk_dos_version
            );
            false
        } else {
            true
        }
    }
}

/// TODO: Get this parser working as it should
/// e.g. it should fail if there is no NOP for a DOS 3.x
pub fn d64_block_availability_map_parser(i: &[u8]) -> IResult<&[u8], D64BlockAvailabilityMap> {
    // Jump to the BAM
    let (i, _) = take(0x16500_usize)(i)?;

    // Should be 0x12, fail on a parse error if it isn't
    let (i, first_directory_sector_track) = verify(le_u8, |val: &u8| *val == 0x12)(i)?;
    // Should be 0x01, fail on a parse error if it isn't
    let (i, first_directory_sector_sector) = verify(le_u8, |val: &u8| *val == 0x01)(i)?;
    // Should be 0x41, otherwise it uses "soft write protection"
    // For now, parse fail because we don't support soft write protection
    let (i, disk_dos_version) = verify(le_u8, |val: &u8| *val == 0x41)(i)?;
    let (i, reserved) = le_u8(i)?;
    let (i, bam_entries) = count(bam_entry_parser, 35_usize)(i)?;
    let (i, disk_name) = take(16_usize)(i)?;
    let (i, second_reserved) = take(2_usize)(i)?;
    let (i, disk_id) = le_u16(i)?;
    let (i, third_reserved) = le_u8(i)?;
    let (i, dos_type) = map(tag("2A"), |_| DOSType::CBM)(i)?;

    let d64_bam = D64BlockAvailabilityMap {
        first_directory_sector_track,
        first_directory_sector_sector,
        disk_dos_version,
        reserved,
        bam_entries,
        disk_name,
        second_reserved,
        disk_id,
        third_reserved,
        dos_type,
    };
    Ok((i, d64_bam))
}

/// Parse a D64 disk image
pub fn d64_disk_parser(i: &[u8]) -> IResult<&[u8], D64Disk> {
    let (i, bam) = d64_block_availability_map_parser(i)?;

    Ok((i, D64Disk { bam }))
}

// impl DiskImageParser for D64Disk<'_> {
//     fn parse_disk_image<'a>(
//         &self,
//         _config: &Config,
//         _filename: &str,
//         data: &'a [u8],
//     ) -> IResult<&'a [u8], DiskImage<'a>> {
//         let (i, parse_result) = d64_disk_parser(data)?;

//         Ok((i, DiskImage::D64(parse_result)))
//     }
// }

impl DiskImageSaver for D64Disk<'_> {
    /// This saves the underlying image on this disk.
    /// This can be a FAT disk image, an ST disk, or a custom disk image
    /// that may or may not be copy-protected.
    fn save_disk_image(
        &self,
        _config: &Config,
        _selected_filename: Option<&str>,
        _filename: &str,
    ) -> std::result::Result<(), crate::error::Error> {
        Err(crate::error::Error::new(
            crate::error::ErrorKind::Unimplemented(String::from(
                "Saving D64 disk images not implemented\n",
            )),
        ))
    }
}
