/// Parse an Atari ST STX (Pasti) disk image
/// The basic structure of a Pasti image is:
///
/// File Header
/// Track Header
///   Sector Header
///   ...
///   Sector Header
///   Fuzzy sector mask (if it exists)
///   Track data, may be track image itself, or image containing sector data, may be missing
///   Sector data x sector count, if not in track image
/// ...
/// Track Header
///   Sector Header
///    ...
/// etc.
///
/// Information from:
/// https://atari.8bitchip.info/STXdesc.html
///   (Good summary)
/// Hatari https://github.com/hatari/hatari.git
/// Thomas Bernard https://github.com/miniupnp/AtariST.git
///   (short Python script that gives a simple overview of reading in the metadata)
/// CLK https://github.com/TomHarte/CLK.git
///   (modern C++ code)
/// pce https://github.com/jsdf/pce.git
///   (easy to understand code)
use std::fmt::{Display, Formatter, Result};
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

use log::{debug, error, info};

use nom::bytes::complete::{tag, take};
use nom::combinator::cond;
use nom::multi::count;
use nom::number::complete::{be_u16, le_u16, le_u32, le_u8};
use nom::IResult;

use crate::disk_format::sanity_check::SanityCheck;

/// A STX disk image
#[derive(Debug)]
pub struct STXDisk<'a> {
    /// The disk header
    pub stx_disk_header: STXDiskHeader<'a>,

    /// The disk tracks
    pub stx_tracks: Vec<STXTrack>,
}

/// Format a STXDisk for display
impl Display for STXDisk<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        write!(f, "header: {}", self.stx_disk_header)
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

/// The STXTrackHeader structure contains information about a single track in a STX disk image
/// 16 bytes
#[derive(Debug)]
pub struct STXTrackHeader {
    /// The block size of this track, in bytes
    /// byte 0 in the track header
    pub block_size: u32,
    /// The fuzzy sector mask size, in bytes
    /// byte 3 in the track header
    /// The fuzzy sector mask is used for copy protection.
    /// It has bits set for every bit in the sector that are not random
    /// (the bits that are real data)
    pub fuzzy_size: u32,
    /// The number of sectors in this track
    /// THis includes sector address blocks and sector data blocks
    /// byte 7 in the header
    pub sectors_count: u16,
    /// Flags for this track
    /// byte 9 in the header
    /// bit 0: if bit 0 is set, the track contains sector blocks
    ///        the track is protected with a custom sector-size
    ///        One of the "custom" sizes is still 512-bytes long
    ///        if bit 0 is set, a standard sector size of 512 bytes is used
    ///        This is like a .ST disk image dump, data is just after the header
    /// bit 5: the track is protected
    /// bit 6: the track contains a track image
    /// bit 7: the track image has a sync position
    ///        the sync position is a word at the start of the track image
    pub flags: u16,
    /// The MFM size, also known as the track length
    /// byte 11 in the header
    pub mfm_size: u16,
    /// The track number
    /// byte 13 in the header
    /// bit 7 determines the side of the floppy (0 is side A, 1 is side B)
    pub track_number: u8,
    /// The record type or track type
    /// byte 14 in the header
    /// 0 == WDC track dump, 0xCC == DC type track dump
    /// bits 0-6 are
    /// bit 7 is
    pub record_type: u8,
}

/// Perform sanity checks for a track header
/// For now, these are done post-parsing of the section
/// These are generally less strict than things like magic number identification
/// but are good indicators the data may be corrupted
impl SanityCheck for STXTrackHeader {
    fn check(&self) -> bool {
        if (self.flags != 0x21) && (self.flags != 0x61) && (self.flags != 0xc1) {
            debug!("Disk flags are a nonstandard value: 0x{:X}", self.flags);
            return false;
        }

        if ((self.flags & 0x40) == 0) && (self.sectors_count > 0) {
            debug!("If flags bit 6 is not set, the sector count should be zero");
            return false;
        }

        true
    }
}

/// Format a STXTrackHeader for display
impl Display for STXTrackHeader {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        write!(
            f,
            "block_size: {}, fuzzy_size: {}, ",
            self.block_size, self.fuzzy_size
        )?;
        writeln!(f, "sectors_count: {}", self.sectors_count)?;
        writeln!(f, "       Flags: 0x{:X} {:b}", self.flags, self.flags)?;
        writeln!(
            f,
            "         bit0(custom-size-byte-sector): {}",
            // bit 0
            if (self.flags & 0x01) == 0x01 {
                "T"
            } else {
                "F"
            }
        )?;
        writeln!(
            f,
            "         bit5(has-track-image): {}",
            // bit 5
            if (self.flags & 0x20) == 0x20 {
                "T"
            } else {
                "F"
            }
        )?;
        writeln!(
            f,
            "         bit6(track-is-proteced): {}",
            // bit 6
            if (self.flags & 0x40) == 0x40 {
                "T"
            } else {
                "F"
            }
        )?;
        writeln!(
            f,
            "         bit7(track-image-has-sync-pos): {}",
            // bit 7
            if (self.flags & 0x80) == 0x80 {
                "T"
            } else {
                "F"
            }
        )?;
        write!(
            f,
            "       mfm_size: {}, track_number: {}, ",
            self.mfm_size, self.track_number
        )?;
        write!(f, "record_type: {}", self.record_type)
    }
}

/// A STXTrack contains a STXTrackHeader
#[derive(Debug)]
pub struct STXTrack {
    /// Thea header for this track
    pub header: STXTrackHeader,

    /// The sector headers in this track
    pub sector_headers: Option<Vec<STXSectorHeader>>,
}

/// Display a single track
impl Display for STXTrack {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        write!(f, "header: {}", self.header)
    }
}

/// Plain ST disk image style sector dump
pub struct STXPlainSector<'a> {
    /// The contents of the sector
    contents: Vec<&'a [u8]>,
}

/// Display a single disk plain sector image metadata
impl Display for STXPlainSector<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        write!(f, "size of contents: {}", self.contents.len())
    }
}

/// STXSector contains information about a single sector in a STX disk image
/// This is when we have a custom-size byte standard sector dump
/// 16 bytes
#[derive(Debug)]
pub struct STXSectorHeader {
    /// offset of the sector data in the sector block.
    /// This is relative to the sector header end, or to the end of the fuzzy mask if it
    /// exists.
    pub data_offset: u32,
    /// position in bits from the start of the track
    pub bit_position: u16,
    /// position of the start of the id field in ms
    /// Some copy protection schemes use the read time to prevent copying
    pub read_time: u16,
    /// contents of the address field
    /// track number of the address block identifying the sector
    /// may be zero because of copy-protection
    pub id_track: u8,
    /// side of the disk from the address block identifying the sector
    /// may be zero because of copy-protection
    pub id_head: u8,
    /// sector from the address block identifying the sector
    pub id_sector: u8,
    /// size of the sector from the address block identifying the sector
    /// 2 = 512b, 3 = 1024b
    pub id_size: u8,
    /// address block CRC
    pub id_crc: u16,
    /// Floppy Drive Controller (FDC) status register after reading the sector
    pub fdc_status: u8,
    /// reserved sector flags, always zero
    pub reserved: u8,
}

/// A single sector on the disk, including the header
pub struct STXSector {
    /// The sector header for this sector
    pub header: STXSectorHeader,
}

/// Format a STXSectorHeader for display
impl Display for STXSectorHeader {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        write!(
            f,
            "data_offset: {}, bit_position: {}, ",
            self.data_offset, self.bit_position
        )?;
        write!(
            f,
            "read_time: {}, id_track: {}, ",
            self.read_time, self.id_track
        )?;
        write!(
            f,
            "id_head: {}, id_sector: {}, ",
            self.id_head, self.id_sector
        )?;
        write!(
            f,
            "id_size: {}, ",
            match self.id_size {
                2 => "512b",
                3 => "1024b",
                _ => "unknown",
            }
        )?;
        write!(f, "id_crc: {}, ", self.id_crc)?;
        write!(
            f,
            "fdc_status: {}, reserved: {}, ",
            self.fdc_status, self.reserved
        )
        //write!(f, "sector_size: {}", self.sector_size)
    }
}

/// Convert the sector size field to number of bytes
pub fn sector_size_as_bytes(size: u8) -> u16 {
    match size {
        2 => 512,
        3 => 1024,
        _ => 0,
    }
}

/// Perform sanity checks for sector headers
/// Check the CRC for the sector header
impl SanityCheck for STXSectorHeader {
    fn check(&self) -> bool {
        let crc = calculate_crc16(self);
        if crc != self.id_crc {
            debug!(
                "Sector CRC is bad: expected: {}, calculated: {}",
                self.id_crc, crc
            );
            false
        } else {
            true
        }
    }
}

/// Display a single sector
impl Display for STXSector {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        write!(f, "header: {}", self.header)
    }
}

/// The track image data on the disk, appears in each track,
/// after the sector headers if they exist, or just after the track headers
pub struct STXTrackImageHeader {
    /// The first sync offset
    /// This field exists if the track flags bit 5 and 6 are set
    pub first_sync_offset: u16,
    /// The track image size
    /// This field exists if the track flags bit 5 is set
    pub track_image_size: u16,
    // The sector data offset base
    // This field exists if the track flags bit 5 is set
    //pub sector_data_offset_base: u8,
}

/// Display a single sector
impl Display for STXTrackImageHeader {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        write!(f, "track image header: ")?;
        write!(
            f,
            "first_sync_offset: {}, track_image_size: {}",
            self.first_sync_offset, self.track_image_size
        )
    }
}

/// Parse a STX track image header
pub fn stx_track_image_header_parser(
    flags: u16,
) -> impl Fn(&[u8]) -> IResult<&[u8], STXTrackImageHeader> {
    // Create and return a closure as the main result of this function
    // i is not a simple value, even though it may appear to operate as one
    // so it doesn't get copied by default
    // This is why we need the move to capture ownership of the values it uses in the
    // environment
    move |i| {
        // If flag bit 6 and 7 are set, get the first sync offset
        let (i, first_sync_offset) =
            cond(((flags & 0x40) != 0) && ((flags & 0x80) != 0), le_u16)(i)?;
        // If flag bit six is set, get the track image size
        let (i, track_image_size) = cond((flags & 0x40) != 0, le_u16)(i)?;

        let stx_track_image_header = STXTrackImageHeader {
            first_sync_offset: first_sync_offset.unwrap_or(0),
            track_image_size: track_image_size.unwrap_or(0),
        };

        Ok((i, stx_track_image_header))
    }
}

/// The actual track data
pub struct STXTrackData<'a> {
    /// The track image data
    data: &'a [u8],
}

/// Display metadata for the track data
impl Display for STXTrackData<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        write!(f, "size of contents: {}", self.data.len())
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

/// Parse a single track on the disk
/// Returns the remaining bytes an STXTrackHeader filled out with the track information
pub fn stx_track_header_parser(i: &[u8]) -> IResult<&[u8], STXTrackHeader> {
    // The track header is 16 bytes long
    let (i, block_size) = le_u32(i)?;
    let (i, fuzzy_size) = le_u32(i)?;
    let (i, sectors_count) = le_u16(i)?;
    let (i, flags) = le_u16(i)?;
    let (i, mfm_size) = le_u16(i)?;
    let (i, track_number) = le_u8(i)?;
    let (i, record_type) = le_u8(i)?;

    let stx_track_header = STXTrackHeader {
        block_size,
        fuzzy_size,
        sectors_count,
        flags,
        mfm_size,
        track_number,
        record_type,
    };

    Ok((i, stx_track_header))
}

/// Return true if this is a boot sector
pub fn calculate_boot_sector_sum(sector_data: &[u8]) -> bool {
    // Calculate the sector sum to see if it's a valid boot sector
    // STX disks may not have valid boot sectors
    let mut sum: u16 = 0;

    // equivalent to: for i in 0..256 { ... sector_data[i] }
    for item in sector_data.iter().take(256) {
        sum += *item as u16;
    }

    sum == 0x1234
}

/// Parse all the data after the sector headers, fuzzy mask and track image header.
pub fn stx_sector_data_parser<'a>(
    stx_track_header: &'a STXTrackHeader,
    stx_sector_headers: &'a [STXSectorHeader],
) -> impl Fn(&[u8]) -> IResult<&[u8], Vec<&[u8]>> + 'a {
    move |i| {
        let mut all_sector_data = Vec::new();
        for sector_header in stx_sector_headers {
            // Start at the same offset every loop, right after the sector headers
            // and fuzzy mask
            // TODO: Maybe after the track image header (2-4 bytes)
            // discard the skipped data
            let (i, _) = take(sector_header.data_offset)(i)?;

            // Discard the read bytes here
            // We're using id_size as the authoritative source for sector data size,
            // instead of relying on consecutive reads
            let (_i, sector_data) = take(sector_size_as_bytes(sector_header.id_size))(i)?;

            all_sector_data.push(sector_data);
        }

        // TODO: Verify this is the correct data and flags are interpreted correctly
        //       The track_image_size isn't being skipped here
        let filename = PathBuf::from(format!(
            "data/testdata-sector-{}.img",
            stx_track_header.track_number
        ));
        let file_result = File::create(filename);
        match file_result {
            Ok(mut file) => {
                for sector_data in &all_sector_data {
                    let _res = file.write_all(sector_data);
                }
            }
            Err(e) => error!("Error opening file: {}", e),
        }

        Ok((i, all_sector_data))
    }
}

/// Parse the track data, including sector headers in the track
/// TODO: Implement full parsing
/// This currently doesn't parse track data, just the headers
/// TODO: Simplify this parser
pub fn stx_track_parser(i: &[u8]) -> IResult<&[u8], STXTrack> {
    // Record the starting position so we can figure out how much was missed
    let starting_position = i;
    let stx_track_header_result = stx_track_header_parser(i)?;

    let stx_track_header = stx_track_header_result.1;
    let i = stx_track_header_result.0;

    if !stx_track_header.check() {
        error!("Invalid data");
        panic!("Invalid data");
    }

    let (_, sector_headers) = if (stx_track_header.flags & 0x01) != 0x01 {
        // Parse a plain data track
        if stx_track_header.sectors_count > 0 {
            let stx_sector = stx_sector_parser_plain(stx_track_header.sectors_count as usize)(i)?;
            (stx_sector.0, None)
        } else {
            (i, None)
        }
    } else {
        // Parse a set of sector headers

        // Fuzzy byte reading is not implemented
        if stx_track_header.fuzzy_size > 0 {
            error!("Fuzzy bytes reading not implemented");
            panic!("Fuzzy bytes reading not implemented");
        }
        // Find out how many sector headers to parse

        info!("Track header: {}", stx_track_header);
        // Parse the STX sector headers
        // The last track has issues parsing in some cases, we hit EOF
        // The last tracks are sometimes flag 0x21 and not 0x61, we need to
        // deal with each track image data separately
        let (i, sector_headers) = if stx_track_header.sectors_count > 0 {
            let stx_sector_headers_result = count(
                stx_sector_header_parser,
                stx_track_header.sectors_count as usize,
            )(stx_track_header_result.0)?;
            let stx_sector_headers = stx_sector_headers_result.1;
            let sector_header_iter = stx_sector_headers.iter();
            for header in sector_header_iter {
                info!("stx_sector_header: {}", header);
            }

            // Skip past the fuzzy mask record
            let (i, _) = take(stx_track_header.fuzzy_size)(stx_sector_headers_result.0)?;

            // The track image data
            // First the header, two or four bytes depending on the flags
            // If track flags bit three (starting from bit zero) is set
            //   Then also test bit seven.
            //     If bit seven is set, read in two bytes, the first sync offset
            //   Then read read in the track image size, two bytes
            // If bit seven is not set, the first sync offset is zero, size is
            // calculated from other data
            // just read in the track image data
            let stx_track_image_header_result =
                stx_track_image_header_parser(stx_track_header.flags)(i)?;
            info!(
                "stx_track_image_header: {}",
                stx_track_image_header_result.1
            );

            // Comment this out for now
            // let _stx_sector_data_parser_result =
            //     stx_sector_data_parser(&stx_track_header, &stx_sector_headers)(i)?;

            (stx_track_image_header_result.0, Some(stx_sector_headers))
        } else {
            (i, None)
        };

        (i, sector_headers)
    };

    // TODO: Fix up the other track image data parsing
    // We don't use the i returned from the sector headers parsing block above, because
    // currently the image track data parsing is unfinished.  So the parser is left in
    // an unfinished state after parsing track and sector headers.
    // But we know the total length of the tracks, so we can skip to the next block
    let (i, _) = take(stx_track_header.block_size)(starting_position)?;

    Ok((
        i,
        STXTrack {
            header: stx_track_header,
            sector_headers,
        },
    ))
}

/// Get n tracks from the disk
/// Returns a vector of the tracks
pub fn stx_tracks_parser(n: usize) -> impl Fn(&[u8]) -> IResult<&[u8], Vec<STXTrack>> {
    move |i| count(stx_track_parser, n)(i)
}

/// Read in a single sector of data
/// This appears after the track header in a plain-style track
/// The plain sector data is 512 bytes long
pub fn stx_sector_data_parser_plain(i: &[u8]) -> IResult<&[u8], &[u8]> {
    take(512_usize)(i)
}

/// Parse plain ST-style sector data
/// Read in n sectors of data
pub fn stx_sector_parser_plain(n: usize) -> impl Fn(&[u8]) -> IResult<&[u8], STXPlainSector> {
    move |i| {
        info!("Reading in plain sector data");
        let (i, data) = count(stx_sector_data_parser_plain, n)(i)?;

        Ok((i, STXPlainSector { contents: data }))
    }
}

/// Parse a custom STX sector
/// The sector parser needs the track flags and fuzzy sector mask settings
pub fn stx_sector_header_parser(i: &[u8]) -> IResult<&[u8], STXSectorHeader> {
    let (i, data_offset) = le_u32(i)?;
    let (i, bit_position) = le_u16(i)?;
    let (i, read_time) = le_u16(i)?;
    let (i, id_track) = le_u8(i)?;
    let (i, id_head) = le_u8(i)?;
    let (i, id_sector) = le_u8(i)?;
    let (i, id_size) = le_u8(i)?;
    // The CRC is in big-endian byte order
    let (i, id_crc) = be_u16(i)?;
    let (i, fdc_status) = le_u8(i)?;
    let (i, reserved) = le_u8(i)?;

    let sector_header = STXSectorHeader {
        data_offset,
        bit_position,
        read_time,
        id_track,
        id_head,
        id_sector,
        id_size,
        id_crc,
        fdc_status,
        reserved,
    };

    if !sector_header.check() {
        error!("Invalid sector header");
        panic!("Invalid sector header");
    }

    Ok((i, sector_header))
}

/// Parses the sector header and the sector data
// pub fn stx_sector_parser(
//     _fuzzy_size: u32,
//     _flags: u16,
// ) -> impl Fn(&[u8]) -> IResult<&[u8], STXSector> {
//     move |i| {
//         let stx_sector_header = stx_sector_header_parser(i)?;

//         Ok((
//             stx_sector_header.0,
//             STXSector {
//                 header: stx_sector_header.1,
//             },
//         ))
//     }
// }

// /// Get n sectors from the disk
// /// Returns a vector of the sectors
// pub fn stx_sectors_parser(
//     fuzzy_size: u32,
//     flags: u16,
//     n: usize,
// ) -> impl Fn(&[u8]) -> IResult<&[u8], Vec<STXSector>> {
//     move |i| count(stx_sector_parser(fuzzy_size, flags), n)(i)
// }

/// Four bytes of sync marker at the start of a track
/// Usually 0xA1, 0xA1, 0xA1, 0xFE
pub struct STXSyncMarker<'a> {
    /// Four bytes of sync marker at the start of a track
    pub sync_markers: &'a [u8],
}

/// Display the sync markers
impl Display for STXSyncMarker<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        write!(f, "sync markers: {:?}", self.sync_markers)
    }
}

/// Read in the four sync markers at the start of the track image data
pub fn stx_sync_markers_parser<'a>(i: &'a [u8]) -> IResult<&'a [u8], STXSyncMarker> {
    let (i, stx_sync_markers) = take(4_usize)(i)?;

    Ok((
        i,
        STXSyncMarker {
            sync_markers: stx_sync_markers,
        },
    ))
}

/// Calculate the CRC-16 value for the sector header
pub fn calculate_crc16(sector_header: &STXSectorHeader, /*, sync_markers: &STXSyncMarker*/) -> u16 {
    // Initialize to 0xFFFF
    let crc: u16 = 0xFFFF;

    // Add the sync marks
    let crc = crc16_add_byte(crc, 0xA1);
    let crc = crc16_add_byte(crc, 0xA1);
    let crc = crc16_add_byte(crc, 0xA1);
    let crc = crc16_add_byte(crc, 0xFE);

    // Add the sector header data
    let crc = crc16_add_byte(crc, sector_header.id_track);
    let crc = crc16_add_byte(crc, sector_header.id_head);
    let crc = crc16_add_byte(crc, sector_header.id_sector);
    crc16_add_byte(crc, sector_header.id_size)
}

const CCITT_CRC16_POLY: u16 = 0x1021;

/// Add a byte to the CRC
/// TODO: Double check this
pub fn crc16_add_byte(crc: u16, byte: u8) -> u16 {
    let mut new_crc = crc;

    // exclusive or the shifted byte and the current CRC
    new_crc ^= ((byte as u16) << 8) as u16;

    // Rust for-loop iteration is not inclusive on the end
    for _i in 0..8 {
        if (new_crc & 0x8000u16) != 0 {
            // exclusive or the shifted CRC and the CRC16 polynomial
            // assuming we are using the CCITT CRC16 polynomial: 0b0001000000100001
            new_crc = (new_crc << 1) ^ CCITT_CRC16_POLY;
        } else {
            new_crc <<= 1;
        }
    }

    new_crc
}

#[cfg(test)]
mod tests {
    use super::SanityCheck;
    use super::CCITT_CRC16_POLY;
    use super::{crc16_add_byte, stx_disk_header_parser, stx_track_header_parser};

    /// Test calculating a CRC16
    #[test]
    fn crc16_add_one_works() {
        let test_byte = 0x01_u8;
        let crc = 0xFFFF_u16;

        // verify the CRC16 polynomial is a known value
        assert_eq!(CCITT_CRC16_POLY, 0x1021);

        let crc = crc16_add_byte(crc, test_byte);

        // Each shift:
        //   0xFFFF (start) -> 0xFEFF (XOR shited byte) ->
        //   0xEDDF -> 0xCB9F -> 0x871F -> 0x1E1F -> 0x3C3E -> 0x787C -> 0xF0F8 -> 0xF1D1
        //   (shift loop)
        assert_eq!(crc, 0xF1D1);
    }

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

    /// Test parsing a STX track header
    #[test]
    fn stx_valid_track_header_parser_works() {
        // image_rider::disk_format::stx] Track header: block_size: 11022, fuzzy_size: 0, sectors_count: 9
        let stx_track_header: [u8; 16] = [
            0x43, 0x2b, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x09, 0x00, 0x61, 0x00, 0x74, 0x18,
            0x00, 0x00,
        ];

        let stx_track_header_parser_result = stx_track_header_parser(&stx_track_header);

        match stx_track_header_parser_result {
            Ok((_, res)) => {
                assert_eq!(res.block_size, 0x2b43);
                assert_eq!(res.fuzzy_size, 0x00);
                assert_eq!(res.sectors_count, 0x09);
                assert_eq!(res.flags, 0x61);
                assert_eq!(res.mfm_size, 0x1874);
                assert_eq!(res.track_number, 0x00);
                assert_eq!(res.record_type, 0x00);
            }
            Err(e) => panic!("Parsing failed on the STX disk header: {}", e),
        }
    }

    /// Test parsing a STX track header with an unknown flags field
    #[test]
    fn stx_unknown_track_header_parser_works() {
        // image_rider::disk_format::stx] Track header: block_size: 11022, fuzzy_size: 0, sectors_count: 9
        let stx_track_header: [u8; 16] = [
            0x43, 0x2b, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x09, 0x00, 0x62, 0x00, 0x74, 0x18,
            0x00, 0x00,
        ];

        let stx_track_header_parser_result = stx_track_header_parser(&stx_track_header);

        match stx_track_header_parser_result {
            Ok((_, res)) => {
                assert_eq!(res.block_size, 0x2b43);
                assert_eq!(res.fuzzy_size, 0x00);
                assert_eq!(res.sectors_count, 0x09);
                assert_eq!(res.flags, 0x62);
                assert_eq!(res.mfm_size, 0x1874);
                assert_eq!(res.track_number, 0x00);
                assert_eq!(res.record_type, 0x00);

                // Should fail because of the flags
                assert_eq!(false, res.check());
            }
            Err(e) => panic!("Parsing failed on the STX disk header: {}", e),
        }
    }
}
