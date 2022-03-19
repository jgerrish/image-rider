///
/// STX Disk sector functions
///
use std::fmt::{Display, Formatter, Result};

use log::{debug, error, info};

use nom::bytes::complete::take;
use nom::multi::count;
use nom::number::complete::{be_u16, le_u16, le_u32, le_u8};
use nom::IResult;

use crate::disk_format::sanity_check::SanityCheck;
use crate::disk_format::stx::crc16_add_byte;

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

/// Parse all the data after the sector headers, fuzzy mask and track image header.
pub fn stx_sector_data_parser<'a>(
    stx_sector_headers: &'a [STXSectorHeader],
) -> impl Fn(&[u8]) -> IResult<&[u8], Vec<&[u8]>> + '_ {
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
        // Sector 0 (sector 1 in Atari ST docs) output appears to be a good Atari ST
        // boot sector compatible with MS-DOS 2.x
        // Pass this data to the FAT code

        Ok((i, all_sector_data))
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

/// Get the 512 bytes of the boot sector as big-endian words (two bytes)
pub fn parse_boot_sector_as_words(sector_data: &[u8]) -> IResult<&[u8], Vec<u16>> {
    count(be_u16, 0x100_usize)(sector_data)
}

/// Return true if this is a boot sector
/// Calculate the sector sum to see if it's a valid boot sector
/// The checksum is calculated over the 256 words of the boot sector
/// These words are in big-endian format
/// STX disks may not have valid boot sectors
/// There are a couple signs a STX disk isn't a boot sector
///   If the boot sector checksum isn't 0x1234
///   If there is no jump in the first byte of the boot sector
/// This is the checksum for FAT disks, not STX disks
/// TODO: Double check this code is in the right crate
pub fn calculate_boot_sector_sum_from_words(sector_data: &[u8]) -> bool {
    let mut sum: u32 = 0;

    let words_result = parse_boot_sector_as_words(sector_data);

    match words_result {
        Ok((_, words)) => {
            for word in words {
                sum = (sum + (word as u32)) % 0xFFFF;
            }
        }
        Err(_) => panic!("Parsing failed for boot sector checksum"),
    }

    sum == 0x1234
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

#[cfg(test)]
mod tests {
    use super::{calculate_boot_sector_sum_from_words, parse_boot_sector_as_words};

    /// Test that converting the boot sector to words works
    #[test]
    fn parse_boot_sector_as_words_works() {
        let mut boot_sector = [0_u8; 512];

        // equivalent to: for i in 0..256 { ... sector_data[i] }
        // for item in sector_data.iter().take(256) {

        for i in 0..512 {
            boot_sector[i] = (i & 0x00FF) as u8;
        }

        let words_result = parse_boot_sector_as_words(&boot_sector);

        match words_result {
            Ok((_, words)) => {
                let mut cnt = 0;
                for word in words.iter() {
                    let byte1: u16 = cnt & 0xFF;
                    cnt += 1;
                    let byte2: u16 = cnt & 0xFF;
                    cnt += 1;

                    // first byte is most-significant byte in big endian
                    let i: u16 = (byte1 << 8) + byte2;
                    assert_eq!(*word, i);
                }
            }
            Err(_) => panic!("Parsing failed for boot sector checksum"),
        }
    }

    /// Test parsing STX boot sector checksum
    /// TODO: This may not be an Atari ST checksum, move it into FAT
    /// and maybe remove it from here
    #[test]
    fn stx_boot_sector_checksum_works() {
        let mut boot_sector = [0_u8; 512];

        boot_sector[0] = 0x12;
        boot_sector[1] = 0x34;

        let checksum = calculate_boot_sector_sum_from_words(&boot_sector);

        assert_eq!(checksum, true);
    }
}
