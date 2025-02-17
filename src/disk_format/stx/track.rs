use log::{debug, error};
use nom::bytes::complete::take;
use nom::combinator::cond;
use nom::multi::count;
use nom::number::complete::{le_u16, le_u32, le_u8};
use nom::IResult;

use std::fmt::{Display, Formatter, Result};

use crate::disk_format::stx::sector::{
    stx_sector_data_parser, stx_sector_header_parser, stx_sector_parser_plain, STXSectorHeader,
};
use crate::disk_format::stx::SanityCheck;

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
            "         bit5(track-is-proteced): {}",
            // bit 5
            if (self.flags & 0x20) == 0x20 {
                "T"
            } else {
                "F"
            }
        )?;
        writeln!(
            f,
            "         bit6(has-track-image): {}",
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

/// A STXTrack contains a STXTrackHeader
#[derive(Debug)]
pub struct STXTrack<'a> {
    /// Thea header for this track
    pub header: STXTrackHeader,

    /// The sector headers in this track
    pub sector_headers: Option<Vec<STXSectorHeader>>,

    /// The sector data for this track
    pub sector_data: Option<Vec<&'a [u8]>>,
}

/// Display a single track
impl Display for STXTrack<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        write!(f, "header: {}", self.header)
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

    let (_, sector_headers, sector_data) = if (stx_track_header.flags & 0x01) != 0x01 {
        // Parse a plain data track
        if stx_track_header.sectors_count > 0 {
            let stx_sector = stx_sector_parser_plain(stx_track_header.sectors_count as usize)(i)?;
            (stx_sector.0, None, None)
        } else {
            (i, None, None)
        }
    } else {
        // Parse a set of sector headers

        // Fuzzy byte reading is not implemented
        if stx_track_header.fuzzy_size > 0 {
            error!("Fuzzy bytes reading not implemented");
            panic!("Fuzzy bytes reading not implemented");
        }
        // Find out how many sector headers to parse

        debug!("Track header: {}", stx_track_header);
        // Parse the STX sector headers
        // The last track has issues parsing in some cases, we hit EOF
        // The last tracks are sometimes flag 0x21 and not 0x61, we need to
        // deal with each track image data separately
        let (i, sector_headers, sector_data) = if stx_track_header.sectors_count > 0 {
            let stx_sector_headers_result = count(
                stx_sector_header_parser,
                stx_track_header.sectors_count as usize,
            )(stx_track_header_result.0)?;
            let stx_sector_headers = stx_sector_headers_result.1;
            let sector_header_iter = stx_sector_headers.iter();
            for header in sector_header_iter {
                debug!("stx_sector_header: {}", header);
            }

            // Skip past the fuzzy mask record
            let (i, _) = take(stx_track_header.fuzzy_size)(stx_sector_headers_result.0)?;

            // The track image data
            // First the header, two or four bytes depending on the flags
            // If track flags bit six (starting from bit zero) is set
            //   Then also test bit seven.
            //     If bit seven is set, read in two bytes, the first sync offset
            //   Then read read in the track image size, two bytes
            // If bit seven is not set, the first sync offset is zero, size is
            // calculated from other data
            // just read in the track image data
            let stx_track_image_header_result =
                stx_track_image_header_parser(stx_track_header.flags)(i)?;
            debug!(
                "stx_track_image_header: {}",
                stx_track_image_header_result.1
            );

            let stx_sector_data_parser_result =
                //stx_sector_data_parser(&stx_track_header, &stx_sector_headers)(stx_track_image_header_result.0)?;
                stx_sector_data_parser(&stx_sector_headers)(i)?;

            (
                stx_track_image_header_result.0,
                Some(stx_sector_headers),
                Some(stx_sector_data_parser_result.1),
            )
        } else {
            (i, None, None)
        };

        (i, sector_headers, sector_data)
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
            sector_data,
        },
    ))
}

/// Get n tracks from the disk
/// Returns a vector of the tracks
pub fn stx_tracks_parser(n: usize) -> impl Fn(&[u8]) -> IResult<&[u8], Vec<STXTrack>> {
    move |i| count(stx_track_parser, n)(i)
}

/// The track image data on the disk, appears in each track,
/// after the sector headers if they exist, or just after the track headers
pub struct STXTrackImageHeader {
    /// The first sync offset
    /// This field exists if the track flags bit 7
    pub first_sync_offset: u16,
    /// The track image size
    /// This field exists if the track flags bit 6 is set
    pub track_image_size: u16,
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
        // If flag bit 6 is set, get the track image size
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

#[cfg(test)]
mod tests {
    use super::SanityCheck;

    use super::stx_track_header_parser;

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
