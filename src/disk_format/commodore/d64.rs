//! Disk-level functions and data structures for Commodore D64 disk images
//!
//! Most of the data comes from The Commodore 1541 Disk Drive Users Guide
//! September, 1982
//!
//! Other data comes from Inside Commodore DOS
use crate::config::Config;
use forbidden_bands::petscii::PetsciiString;
use log::{debug, info};
use nom::bytes::complete::{tag, take};
use nom::combinator::{map, verify};
use nom::multi::count;
use nom::number::complete::{le_u16, le_u8};
use nom::IResult;
/// Parse a Commodore D64 disk image
use std::fmt::{Debug, Display, Formatter, Result};

use crate::disk_format::image::{DiskImage, DiskImageOps, DiskImageParser, DiskImageSaver};
use crate::disk_format::sanity_check::SanityCheck;
use crate::error::{Error, ErrorKind, InvalidErrorKind};

/// A Commodore D64 disk
pub struct D64Disk<'a> {
    /// The D64 Block Availability Map
    pub bam: D64BlockAvailabilityMap<'a>,

    /// The D64 disk directory
    pub directory: D64DirectoryEntry<'a>,
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

/// Specifications for a VIC 1540/1541 Single Drive Floppy Disk
///
/// These are from the Commodore 1541 Disk Drive User Guide, September
/// 1982.
///
pub struct VIC1540SingleDriveFloppyDisk {
    /// Total capacity of the disk
    pub total_capacity: u32,
    /// Number of records per file
    pub records_per_file: u32,
    /// Number of directory entries
    pub directory_entries: u16,
    /// Number of sectors per track
    pub sectors_per_track: u8,
    /// Number of bytes per sector, usually 256
    pub bytes_per_sector: u16,
    /// Number of tracks on the disk, usually 35
    pub tracks: u8,
    /// Total number of blocks
    pub blocks: u16,
}

/// Normal max specificiations for a VIC 1540/1541 Single Drive Floppy
/// Disk
///
/// These are from the Commodore 1541 Disk Drive User Guide, September
/// 1982.
pub static VIC_1540_SINGLE_DRIVE_FLOPPY_DISK: VIC1540SingleDriveFloppyDisk =
    VIC1540SingleDriveFloppyDisk {
        total_capacity: 174848,
        records_per_file: 65536,
        directory_entries: 144,
        sectors_per_track: 17,
        bytes_per_sector: 256,
        tracks: 35,
        blocks: 683,
    };

/// Mapping from track number to block range
/// The number of blocks differs depending on whether the tracks are inner or
/// outer tracks.  The outer tracks contain 21 blocks, the inner tracks contain 17.
///
/// TODO: Use zero-based for this and other track/block code
/// TODO: Test inclusive/exclusive ranges
pub fn track_to_block_length(track: u8) -> core::result::Result<u8, String> {
    match track {
        1..17 => Ok(21),
        17..24 => Ok(19),
        24..30 => Ok(18),
        30..35 => Ok(17),
        _ => Err("Invalid track number: {track}".to_string()),
    }
}

/// Mapping from track number to block range
/// The number of blocks differs depending on whether the tracks are inner or
/// outer tracks.  The outer tracks contain 21 blocks, the inner tracks contain 17.
///
/// Track numbers are 0-based
/// TODO: Use zero-based for this and other track/block code
/// TODO: Test inclusive/exclusive ranges
/// TODO: Add tests, including test for track 18, sector 0
pub fn block_to_track(block: u16) -> core::result::Result<u16, String> {
    match block {
        0..356 => Ok(block / 21),
        356..489 => Ok((block / 19) + 17),
        489..597 => Ok((block / 18) + 24),
        597..682 => Ok((block / 17) + 30),
        _ => Err("Invalid track number: {track}".to_string()),
    }
}

/// The disk name
pub struct DiskName<'a>(PetsciiString<'a, 16>);

impl Display for DiskName<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        write!(f, "{}", self.0)
    }
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
    pub bam_entries: Vec<D64BAMEntry>,

    /// The disk name, 16 bytes, padded with 0xA0
    pub disk_name: DiskName<'a>,

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
pub struct D64BAMEntry {
    /// Number of free sectors on track
    pub free_sectors_on_track: u8,

    /// The sector use bitmap, 3 bytes or 24 bits
    pub sector_use_bitmap: [u8; 3],
}

/// BooleanVector is from the Rust source code: core::bit library
/// crate
#[derive(Debug, PartialEq)]
struct BooleanVector(Vec<bool>);

fn bam_entry_to_boolean_vector(bam: &D64BAMEntry) -> BooleanVector {
    let mut bitvec: BooleanVector = BooleanVector(Vec::new());

    let mut tmp: u8 = bam.sector_use_bitmap[0];

    for _i in 0..8 {
        let bit = (tmp & 0x01) == 0x01;
        bitvec.0.push(bit);
        tmp >>= 1;
    }
    tmp = bam.sector_use_bitmap[1];
    for _i in 0..8 {
        let bit = (tmp & 0x01) == 0x01;
        bitvec.0.push(bit);
        tmp >>= 1;
    }
    tmp = bam.sector_use_bitmap[2];
    for _i in 0..5 {
        let bit = (tmp & 0x01) == 0x01;
        bitvec.0.push(bit);
        tmp >>= 1;
    }

    bitvec
}

fn bitmap_to_chars(bitmap: &BooleanVector, used_char: char, unused_char: char) -> String {
    let s: String = bitmap
        .0
        .iter()
        .map(|x| if *x { used_char } else { unused_char })
        .collect();
    s
}

/// Display a Commodore D64 disk
impl Debug for D64BAMEntry {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        let bitvec = bam_entry_to_boolean_vector(self);
        let chars = bitmap_to_chars(&bitvec, 'X', '-');
        write!(f, "free sectors on track: {}, ", self.free_sectors_on_track)?;
        write!(f, "sector use bitmap: {}", chars)
    }
}

/// Display a Commodore D64 disk
impl Display for D64BAMEntry {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        let bitvec = bam_entry_to_boolean_vector(self);
        let chars = bitmap_to_chars(&bitvec, 'X', '-');
        write!(f, "free sectors on track: {}, ", self.free_sectors_on_track)?;
        write!(f, "sector use bitmap: {}", chars)
    }
}

/// A directory entry
/// These are direct mappings of the directory structure,
/// that logic maybe shouldn't be in the data structure
pub struct D64DirectoryEntry<'a> {
    /// The track of the next directory block
    /// Tracks start at track number one (they are 1-based)
    /// This is different from the sector numbering
    /// If there is no next directory block, this should be 0x00
    pub track_of_next_directory_block: Option<u16>,
    /// The sector of the next directory block
    /// Sectors start at sector number zero (they are 0-based)
    /// This is different from the track numbering
    /// If there is no next directory block, this should be 0xFF
    pub sector_of_next_directory_block: Option<u16>,

    /// Eight file entries
    pub file_entries: Vec<FileEntry<'a>>,
}

/// The full byte of the file-type This includes the lower-four bits
/// of the file type: (DEL, SEQ, PRG, USR, REL) along with the
/// upper-bits of the file flag (closed, unclosed, @ replacement,
/// locked)
pub struct ExtendedFileType {
    /// The status of the file
    file_status: FileStatus,

    /// The type of the file
    file_type: FileType,
}

/// Display an ExtendedFileType
/// This Display implementation uses the same format
/// as would appear on a Commodore DOS listing
///
/// Directory format is described on page 43 of Inside Commodore DOS
impl Display for ExtendedFileType {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        let ft = self.file_type;

        let t: String = format!("{:?}", ft);

        let mode = self.file_status;
        let tmp_show: String = match mode {
            FileStatus::Normal => t,
            FileStatus::Unclosed => {
                let mut s = String::from("*");
                s.push_str(&t);
                s
            }
            FileStatus::AtReplacement => t,
            FileStatus::Locked => {
                let mut s = t;
                s.push_str(" <");
                s
            }
            _ => String::from(""),
        };

        let final_show = if ((self.file_type == FileType::Relative)
            && ((self.file_status == FileStatus::Unclosed)
                || (self.file_status == FileStatus::AtReplacement)))
            || ((self.file_type == FileType::Deleted) && (self.file_status == FileStatus::Unclosed))
        {
            String::from("")
        } else {
            tmp_show
        };
        write!(f, "{}", final_show)
    }
}

impl Debug for ExtendedFileType {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        write!(f, "{}", self)
    }
}

/// The upper five bits of the file represent the status of the file,
/// for example whether it is unclosed, an @ replacement, or locked.
#[derive(Clone, Copy, Eq, PartialEq)]
pub enum FileStatus {
    /// The file is a normal file
    Normal = 0x80,
    /// The file is unclosed
    Unclosed = 0x00,
    /// The file is an @ replacement
    AtReplacement = 0xA0,
    /// The file is locked
    Locked = 0xC0,
    /// An invalid file status
    Invalid,
}

/// The lower three bits of the file type represent the type of the
/// file: Deleted, Sequential, Program, User or Relative.
#[derive(Clone, Copy, Eq, PartialEq)]
pub enum FileType {
    /// The file is scratched or deleted.
    Deleted = 0,
    /// The file is a sequential file.
    Sequential = 1,
    /// The file is a program
    Program = 2,
    /// The file is a user file.
    User = 3,
    /// The file is a relative file
    Relative = 4,
    /// Special flag for invalid files
    /// TODO: This maybe shouldn't be here
    Invalid,
}

impl Debug for FileType {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        let s = match self {
            FileType::Deleted => String::from(""),
            FileType::Sequential => String::from("SEQ"),
            FileType::Program => String::from("PRG"),
            FileType::User => String::from("USR"),
            FileType::Relative => String::from("REL"),
            FileType::Invalid => String::from("INV"),
        };
        write!(f, "{}", s)
    }
}

impl From<u8> for FileStatus {
    fn from(num: u8) -> FileStatus {
        match num & 0xF0 {
            0x80 => FileStatus::Normal,
            0x00 => FileStatus::Unclosed,
            0xA0 => FileStatus::AtReplacement,
            0xC0 => FileStatus::Locked,
            _ => {
                info!("File type is invalid: {}", num);
                FileStatus::Invalid
            }
        }
    }
}

impl From<u8> for FileType {
    fn from(num: u8) -> FileType {
        match num & 0x0F {
            0 => FileType::Deleted,
            1 => FileType::Sequential,
            2 => FileType::Program,
            3 => FileType::User,
            4 => FileType::Relative,
            _ => {
                info!("File type is invalid: {}", num);
                FileType::Invalid
            }
        }
    }
}

/// A Commodore DOS file name
/// 16 bytes for file name
/// The name is padded out to 16 characters with shifted spaces
/// (0xA0).  These spaces are not displayed normally.
pub struct FileName<'a>(PetsciiString<'a, 16>);

impl Display for FileName<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        write!(f, "{}", self.0)
    }
}

/// Each file entry is 30 bytes on disk
pub struct FileEntry<'a> {
    /// The full byte of the file type
    /// If it is zero, the file is scratched
    pub file_type: ExtendedFileType,
    /// The track of the first data block
    pub track_of_first_data_block: u8,
    /// The sector of the first data block
    pub sector_of_first_data_block: u8,
    /// 16 bytes for file name
    /// The name is padded out to 16 characters with shifted spaces
    /// (0xA0).  These spaces are not displayed normally.
    pub file_name: FileName<'a>,

    /// Relative file entry fields

    /// The track of the first set of side sector blocks
    pub track_of_first_side_sector_block: u8,
    /// The sector of the first set of side sector blocks
    pub sector_of_first_side_sector_block: u8,

    /// The record size the relative file entry was created with
    pub record_size: u8,

    /// 4 unused bytes
    pub unused: &'a [u8],

    /// The next two bytes are used by the drive software during save
    /// and replace operations.  They shouldn't be used by normal user
    /// operations.
    /// Track of the start of the new replacement file
    pub track_of_replacement_file: u8,
    /// Sector of the start of the new replacement file
    pub sector_of_replacement_file: u8,

    /// number of blocks in file
    /// low-byte first, then high-byte
    pub number_of_blocks_in_file: u16,
}

/// Parse an entry in the Block Availability Map table
pub fn bam_entry_parser(i: &[u8]) -> IResult<&[u8], D64BAMEntry> {
    let (i, free_sectors_on_track) = le_u8(i)?;

    let (i, bm) = take(3_usize)(i)?;
    let sector_use_bitmap: [u8; 3] = [bm[0], bm[1], bm[2]];

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
        write!(f, "disk_name: {}, ", String::from(self.disk_name.0))?;
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
pub fn d64_block_availability_map_parser<'a>(
    config: &'a crate::config::Config,
) -> impl Fn(&'a [u8]) -> IResult<&[u8], D64BlockAvailabilityMap> + 'a {
    move |i| {
        // Jump to the BAM
        // 0x16500 is 91392 which is:
        // 17 (number of tracks before BAM) * 21 (blocks per track for first 17 tracks)
        // * 256 (bytes per sector)
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
        let (i, _shifted_spaces) = take(2_usize)(i)?;
        let (i, _nulls) = take(84_usize)(i)?;

        let (i, _todofix) = take(3_usize)(i)?;

        let disk_name_ps: PetsciiString<'_, 16> =
            PetsciiString::from_byte_slice_strip_shifted_space_with_config(
                disk_name,
                &config.forbidden_bands_config.petscii,
            );

        let d64_bam = D64BlockAvailabilityMap {
            first_directory_sector_track,
            first_directory_sector_sector,
            disk_dos_version,
            reserved,
            bam_entries,
            disk_name: DiskName(disk_name_ps),
            second_reserved,
            disk_id,
            third_reserved,
            dos_type,
        };
        Ok((i, d64_bam))
    }
}

/// Parse a D64 file entry
pub fn d64_file_entry_parser<'a>(
    //i: &[u8]) -> IResult<&[u8], FileEntry> {
    config: &'a crate::config::Config,
) -> impl Fn(&'a [u8]) -> IResult<&[u8], FileEntry> + 'a {
    move |i| {
        let (i, ft) = le_u8(i)?;
        let file_type = ExtendedFileType {
            file_status: FileStatus::from(ft),
            file_type: FileType::from(ft),
        };

        let (i, track_of_first_data_block) = le_u8(i)?;
        let (i, sector_of_first_data_block) = le_u8(i)?;
        let (i, file_name) = take(16_usize)(i)?;

        let (i, track_of_first_side_sector_block) = le_u8(i)?;
        let (i, sector_of_first_side_sector_block) = le_u8(i)?;
        let (i, record_size) = le_u8(i)?;
        let (i, unused) = take(4_usize)(i)?;

        let (i, track_of_replacement_file) = le_u8(i)?;
        let (i, sector_of_replacement_file) = le_u8(i)?;
        let (i, number_of_blocks_in_file) = le_u16(i)?;

        // Strip any trailing "shifted space" (0xA0) characters from
        // the string
        let ps: PetsciiString<'_, 16> =
            PetsciiString::from_byte_slice_strip_shifted_space_with_config(
                file_name,
                &config.forbidden_bands_config.petscii,
            );

        info!("Name: {}, File type: {:?}", ps, ft);

        Ok((
            i,
            FileEntry {
                file_type,
                track_of_first_data_block,
                sector_of_first_data_block,
                file_name: FileName(ps),
                track_of_first_side_sector_block,
                sector_of_first_side_sector_block,
                record_size,
                unused,
                track_of_replacement_file,
                sector_of_replacement_file,
                number_of_blocks_in_file,
            },
        ))
    }
}

/// TODO: Get this parser working as it should
pub fn d64_directory_parser<'a>(
    config: &'a crate::config::Config,
) -> impl Fn(&'a [u8]) -> IResult<&[u8], D64DirectoryEntry<'a>> {
    move |i| {
        let (i, track_of_next_directory_block) = le_u8(i)?;
        let (i, sector_of_next_directory_block) = le_u8(i)?;

        let (i, file_entries) = count(d64_file_entry_parser(config), 8)(i)?;

        Ok((
            i,
            D64DirectoryEntry {
                track_of_next_directory_block: Some(track_of_next_directory_block.into()),
                sector_of_next_directory_block: Some(sector_of_next_directory_block.into()),
                file_entries,
            },
        ))
    }
}

/// Parse a D64 disk image
pub fn d64_disk_parser<'a>(
    config: &'a crate::config::Config,
) -> impl Fn(&'a [u8]) -> IResult<&[u8], D64Disk<'a>> + 'a {
    move |i| {
        let (i, bam) = d64_block_availability_map_parser(config)(i)?;

        let (i, directory) = d64_directory_parser(config)(i)?;

        Ok((i, D64Disk { bam, directory }))
    }
}

// impl DiskImageParser for D64Disk<'_> {
//     fn parse_disk_image<'a>(
//         &self,
//         _config: &crate::config::Config,
//         _filename: &str,
//         data: &'a [u8],
//     ) -> IResult<&'a [u8], DiskImage<'a>> {
//         let (i, parse_result) = d64_disk_parser(data)?;

//         Ok((i, DiskImage::D64(parse_result)))
//     }
// }

// TODO: This should be on D64DiskGuess or DiskGuess
impl<'a, 'b> DiskImageParser<'a, 'b> for D64Disk<'a> {
    fn parse_disk_image(
        &'a self,
        _config: &'a crate::config::Config,
        _filename: &str,
    ) -> std::result::Result<DiskImage<'a>, Error> {
        Err(Error::new(ErrorKind::Invalid(InvalidErrorKind::Invalid(
            String::from("DiskImageParser parse_disk_image unimplemented for Commodore D64 disk"),
        ))))
    }
}

impl<'a, 'b> DiskImageOps<'a, 'b> for D64Disk<'a> {
    fn catalog(&'a self, _config: &'b Config) -> std::result::Result<String, Error> {
        let mut result = String::new();
        result.push_str(String::from(self.bam.disk_name.0).as_ref());
        result.push('\n');

        for file in &self.directory.file_entries {
            let s = file.file_name.to_string();
            result.push_str(&s);
            result.push_str(file.file_type.to_string().as_str());
            result.push('\n');
        }
        Ok(result)
    }
}

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

#[cfg(test)]
mod tests {
    use super::{
        bam_entry_parser, bam_entry_to_boolean_vector, bitmap_to_chars, d64_file_entry_parser,
        FileType,
    };
    use crate::config::Configuration;

    /// Test parsing STX boot sector checksum
    /// TODO: This may not be an Atari ST checksum, move it into FAT
    /// and maybe remove it from here
    #[test]
    fn bam_entry_to_boolean_vector_works() {
        let bam_entry_data: [u8; 4] = [0x11, 0xd7, 0x5f, 0x1f];

        let bam_entry = bam_entry_parser(&bam_entry_data).unwrap_or_else({
            |e| {
                panic!("Parsing failed on the BAM entry: {}", e);
            }
        });

        let bv = bam_entry_to_boolean_vector(&bam_entry.1);

        assert_eq!(bv.0.len(), 21);
        assert_eq!(bv.0[0], true);
        assert_eq!(bv.0[1], true);
        assert_eq!(bv.0[2], true);
        assert_eq!(bv.0[3], false);
        assert_eq!(bv.0[4], true);
        assert_eq!(bv.0[5], false);
        assert_eq!(bv.0[6], true);
        assert_eq!(bv.0[7], true);

        assert_eq!(bv.0[8], true);
        assert_eq!(bv.0[9], true);
        assert_eq!(bv.0[10], true);
        assert_eq!(bv.0[11], true);
        assert_eq!(bv.0[12], true);
        assert_eq!(bv.0[13], false);
        assert_eq!(bv.0[14], true);
        assert_eq!(bv.0[15], false);

        assert_eq!(bv.0[16], true);
        assert_eq!(bv.0[17], true);
        assert_eq!(bv.0[18], true);
        assert_eq!(bv.0[19], true);
        assert_eq!(bv.0[20], true);
    }

    /// Test parsing STX boot sector checksum
    /// TODO: This may not be an Atari ST checksum, move it into FAT
    /// and maybe remove it from here
    #[test]
    fn boolean_vector_to_chars_works() {
        let bam_entry_data: [u8; 4] = [0x11, 0xd7, 0x5f, 0x1f];

        let bam_entry = bam_entry_parser(&bam_entry_data).unwrap_or_else({
            |e| {
                panic!("Parsing failed on the BAM entry: {}", e);
            }
        });

        let bv = bam_entry_to_boolean_vector(&bam_entry.1);

        let bc = bitmap_to_chars(&bv, '1', '0');
        assert_eq!(bc, "111010111111101011111");

        let bc = bitmap_to_chars(&bv, 'X', '-');
        assert_eq!(bc, "XXX-X-XXXXXXX-X-XXXXX");
    }

    #[test]
    fn file_entry_parser_works() {
        let data_disk: [u8; 30] = [
            0x82, 0x11, 0x00, 0x48, 0x4f, 0x57, 0x20, 0x54, 0x4f, 0x20, 0x55, 0x53, 0x45, 0xa0,
            0xa0, 0xa0, 0xa0, 0xa0, 0xa0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0xd0, 0x00,
        ];

        let config = crate::config::Config::load(config::Config::default())
            .expect("Error loading image rider config");

        let file_entry = d64_file_entry_parser(&config)(&data_disk)
            .unwrap_or_else({
                |e| {
                    panic!("Parsing failed on the file entry: {}", e);
                }
            })
            .1;

        assert_eq!(
            FileType::from(file_entry.file_type.file_type),
            FileType::Program
        );

        // assert_eq!(file_entry_program.file_type, 0x82);
    }
}
