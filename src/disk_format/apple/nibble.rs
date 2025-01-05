//! Encoding and Decoding Nibble-based disk formats
use std::collections::BTreeMap;
use std::convert::TryInto;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

use crate::config::Config;
use log::{debug, error};

use nom::{
    bytes::streaming::{take, take_until},
    character::complete::one_of,
    multi::many0,
    number::complete::le_u8,
    IResult,
};

use crate::disk_format::image::DiskImageSaver;

/// The different nibble encoding formats used for Apple disk images.
/// These are required because of hardware requirements with Apple
/// disk drives.  Not all 256 possible byte values could be written to
/// disk.
///
/// The first requirement for hardware required that the high bit
/// always be set.
/// Second, at least two adjacent bits need to be set.
/// Third, at most one pair of consecutive zero bits.
///
/// This leaves 34 valid disk bytes, in the range from AA to FF.
/// Two of these bytes are reserved: 0xAA and 0xD5
///
/// This led to the "4 and 4" encoding format.  This
/// splits the byte into two bytes, one containing the odd bytes and
/// one containing the even bytes.
/// Other encoding formats satisify these properties while allowing
/// more efficient data usage.
pub enum Format {
    /// Four and four splits each data byte into two disk bytes,
    /// containing the odd and even bits.

    /// b_7 b_6 b_5 b_4 b_3 b_2 b_1 b_0 -> 1 . . . b_7 b_5 b_3 b_1,
    ///                                    1 . . . b_6 b_4 b_2 b_0
    /// Used in earlier versions of DOS before DOS 3
    FourAndFour,

    /// 5 and 3 uses 5 bits of the data in the disk byte
    /// Used in DOS versions from DOS 3 to DOS 3.2.1
    FiveAndThree,

    /// 6 and 2 uses 6 bits of the data in the disk byte
    /// This was enabled by changes in the disk ROM that allowed two
    /// consecutive zero bits.
    /// Used in DOS 3.3
    SixAndTwo,
}

/// The converstion table for writing nibble data
#[allow(dead_code)]
const NIBBLE_WRITE_TABLE_6_AND_2: [u8; 64] = [
    0x96, 0x97, 0x9A, 0x9B, 0x9D, 0x9E, 0x9F, 0xA6, 0xA7, 0xAB, 0xAC, 0xAD, 0xAE, 0xAF, 0xB2, 0xB3,
    0xB4, 0xB5, 0xB6, 0xB7, 0xB9, 0xBA, 0xBB, 0xBC, 0xBD, 0xBE, 0xBF, 0xCB, 0xCD, 0xCE, 0xCF, 0xD3,
    0xD6, 0xD7, 0xD9, 0xDA, 0xDB, 0xDC, 0xDD, 0xDE, 0xDF, 0xE5, 0xE6, 0xE7, 0xE9, 0xEA, 0xEB, 0xEC,
    0xED, 0xEE, 0xEF, 0xF2, 0xF3, 0xF4, 0xF5, 0xF6, 0xF7, 0xF9, 0xFA, 0xFB, 0xFC, 0xFD, 0xFE, 0xFF,
];

/// The converstion table for reading nibble data
/// It's the write table inverted
const NIBBLE_READ_TABLE_6_AND_2: [u8; 256] = [
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x02, 0x03, 0x00, 0x04, 0x05, 0x06,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x07, 0x08, 0x00, 0x00, 0x00, 0x09, 0x0A, 0x0B, 0x0C, 0x0D,
    0x00, 0x00, 0x0E, 0x0F, 0x10, 0x11, 0x12, 0x13, 0x00, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1A,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x1B, 0x00, 0x1C, 0x1D, 0x1E,
    0x00, 0x00, 0x00, 0x1F, 0x00, 0x00, 0x20, 0x21, 0x00, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x28,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x29, 0x2A, 0x2B, 0x00, 0x2C, 0x2D, 0x2E, 0x2F, 0x30, 0x31, 0x32,
    0x00, 0x00, 0x33, 0x34, 0x35, 0x36, 0x37, 0x38, 0x00, 0x39, 0x3A, 0x3B, 0x3C, 0x3D, 0x3E, 0x3F,
];

/// Parse a single byte encoded in 4 and 4 nibble format
/// This is used to encode the volume, track, sector and checksum fields
/// in the address field
pub fn parse_nibble_byte_4_and_4(i: &[u8]) -> IResult<&[u8], u8> {
    let (i, bytes) = take(2_usize)(i)?;

    let byte = ((bytes[0] << 1) | 0x01) & bytes[1];

    Ok((i, byte))
}

/// An address field identifies the data field that follows it
pub struct AddressField {
    /// The volume of the track
    pub volume: u8,
    /// The track of the track
    pub track: u8,
    /// The sector of the track
    pub sector: u8,
    /// The checksum of the address field
    pub checksum: u8,
}

/// Finds the first sector address prologue in the data
/// Returns the prologue as a slice of three bytes
pub fn parse_prologue(i: &[u8]) -> IResult<&[u8], &[u8]> {
    // Find 0xD5 0xAA, the start of the address prologue, then either 0x96 or 0xB5
    // If another ending byte is found, keep searching from that point
    let mut new_i = i;

    loop {
        let (i, _result) = take_until(&[0xD5, 0xAA][..])(new_i)?;

        // Start the search at the character after DFAA
        new_i = &i[2..];

        let result = one_of::<&[u8], [u8; 2], crate::error::Error>([0x96_u8, 0xB5_u8])(new_i);
        match result {
            Ok(r) => {
                debug!("Found an address field prologue");
                return Ok((r.0, i));
            }
            // Should check for EOF error
            Err(_e) => {}
        }

        // Next search includes the character after DFAA
    }
}

/// Searches for an address prologue in the image
/// If it's found, it returns the byte indicating the type of the prologue
pub fn recognize_prologue(i: &[u8]) -> Option<u8> {
    let result: IResult<&[u8], &[u8]> = parse_prologue(i);

    match result {
        Ok(r) => Some(r.1[2]),
        Err(_) => None,
    }
}

/// Find and parse an address field in the nibblized file
pub fn find_and_parse_address_field(
    config: &Config,
) -> impl Fn(&[u8]) -> IResult<&[u8], AddressField> + '_ {
    // Find the first field
    // Read in the address field
    // 3 byte prologue (D5 AA 96)
    // 2 byte odd-even encoded volume:
    //   odd (D_7 D_5 D_3 D_1) followed by even (D_6 D_4 D_2 D_0)
    // 2 byte odd-even encoded track
    // 2 byte odd-even encoded sector
    // 2 byte odd-even encoded checksum
    // Epilogue DE AA EB
    // debug!("Searching 1");
    move |i| {
        let (i, _data) = take_until(&[0xD5, 0xAA, 0x96][..])(i)?;
        let (i, _prologue) = take(3_usize)(i)?;
        let (i, volume) = parse_nibble_byte_4_and_4(i)?;
        let (i, track) = parse_nibble_byte_4_and_4(i)?;
        let (i, sector) = parse_nibble_byte_4_and_4(i)?;
        let (i, checksum) = parse_nibble_byte_4_and_4(i)?;
        let (i, _epilogue) = take(3_usize)(i)?;

        debug!(
            "Found address field: volume: {}, track: {}, sector: {}, checksum: {}",
            volume, track, sector, checksum
        );

        let computed_checksum = volume ^ track ^ sector;

        let address_field = AddressField {
            volume,
            track,
            sector,
            checksum,
        };

        if computed_checksum != checksum {
            error!(
                "Address field computed checksum not equal to disk checksum: {} {}",
                computed_checksum, checksum
            );
            if !config
                .settings
                .get_bool("ignore-checksums")
                .unwrap_or(false)
            {
                panic!(
                    "Address field computed checksum not equal to disk checksum: {} {}",
                    computed_checksum, checksum
                );
            }
        }

        Ok((i, address_field))
    }
}

/// A 6 and 2 encoded data field that follows an address field in a nibblized image
pub struct DataField {
    /// The DataField prologue, three bytes
    _prologue: [u8; 3],
    /// 342 bytes of data, encoded as 6 and 2
    pub data: Vec<u8>,
    /// The checksum of the data
    pub checksum: u8,
    /// The DataField epilogue, three bytes
    _epilogue: [u8; 3],
}

/// Parse the data component of a data field
pub fn parse_6_and_2_nibblized_data(i: &[u8]) -> Vec<u8> {
    let data: Vec<u8> = i
        .iter()
        .map(|b| NIBBLE_READ_TABLE_6_AND_2[(*b & 0x7F) as usize])
        .collect();

    data
}

/// Find and parse a data field in the nibblized file
pub fn find_and_parse_data_field(i: &[u8]) -> IResult<&[u8], DataField> {
    // Find the next sequence of 0xD5 0xAA 0xAD that identifies a field
    // let (i, find_tag) = tag([0xD5, 0xAA, 0xAD])(i)?;
    // Find the first field
    let (i, _data) = take_until(&[0xD5, 0xAA, 0xAD][..])(i)?;

    // Read in the data field
    // 3 byte prologue (D5 AA AD)
    // 342 bytes data, 6 and 2 encoded
    // 1 byte checksum
    // Epilogue DE AA EB
    let (i, prologue) = take(3_usize)(i)?;
    let (i, data) = take(342_usize)(i)?;
    let (i, checksum) = le_u8(i)?;
    // let (i, _epilogue) = tag(&[0xDE, 0xAA, 0xEB][..])(i)?;
    let (i, epilogue) = take(3_usize)(i)?;

    Ok((
        i,
        DataField {
            _prologue: prologue.try_into().unwrap(),
            data: data.to_vec(),
            checksum,
            _epilogue: epilogue.try_into().unwrap(),
        },
    ))
}

/// A 256-byte 8-bit data structure computed from 6 and 2 data
#[derive(Clone)]
pub struct Sector {
    /// The data
    pub data: Vec<u8>,
}

/// Compute the checksum and transformed buffer for the data field
pub fn data_field_build_buffer(data_field: &DataField) -> ([u8; 342], u8) {
    // The data is split up into several different sections
    // The first 0x56 bytes are the "auxiliary data buffer"
    // Starting at offset 0x56 the 6 bit bytes are stored
    let mut computed_checksum: u8 = 0;
    let data_field_data_size = data_field.data.len();
    let mut buffer = [0; 342];

    for (index, byte) in data_field.data.iter().enumerate() {
        computed_checksum ^= NIBBLE_READ_TABLE_6_AND_2[(*byte) as usize];
        if index < 0x56 {
            buffer[data_field_data_size - index - 1] = computed_checksum;
        } else {
            buffer[index - 0x56] = computed_checksum;
        }
    }
    computed_checksum ^= NIBBLE_READ_TABLE_6_AND_2[data_field.checksum as usize];

    (buffer, computed_checksum)
}

/// Transform a 6 and 2 data field to a 256-byte sector
pub fn transform_data_field(config: &Config, data_field: &DataField) -> Sector {
    // The data is split up into several different sections
    // The first 0x56 bytes are the "auxiliary data buffer"
    // Starting at offset 0x56 the 6 bit bytes are stored
    // Two references that explain the encoding and decoding:
    // Beneath Apple DOS and Beneath Apple ProDOS
    // The source code for AppleCommander and apple2emu was invaluable
    // in writing this code
    let mut data = [0; 256];

    let (buffer, computed_checksum) = data_field_build_buffer(data_field);

    if computed_checksum != 0 {
        error!(
            "Invalid checksum on data: calculated: {}, disk: {}",
            computed_checksum, data_field.checksum
        );
        if !config
            .settings
            .get_bool("ignore-checksums")
            .unwrap_or(false)
        {
            panic!(
                "Invalid checksum on data: calculated: {}, disk: {}",
                computed_checksum, data_field.checksum
            );
        }
    }

    let reverse_values = [0x00, 0x02, 0x01, 0x03];
    for i in 0..=255 {
        let byte_1 = buffer[i];
        let nibble_low = buffer.len() - (i % 0x56) - 1;
        let byte_2 = buffer[nibble_low];
        let shift_pairs = (i / 0x56) * 2;
        let byte: u8 = (byte_1 << 2) | reverse_values[((byte_2 >> shift_pairs) & 0x03) as usize];
        data[i] = byte;
    }

    Sector {
        data: data.to_vec(),
    }
}

// pub fn build_nibble_sector_5_and_3(data: &[u8]) -> DataField {
// }

/// Nibblize a sector
/// This nibblizes a sector using the 6 and 2 algorithm
/// This encodes data in the lower six bits.
/// There are two reserved bytes, 0xAA and 0xD5
///
/// There are several ways this can be done.  The clearest is to split
/// up the data into blocks that are multiples of six, since the
/// encoding format uses the lower six bits.
/// For u8 blocks of size six or 256 (the standard sector size) work.
pub fn build_nibble_sector(data: &[u8]) -> DataField {
    // The nibble data plus two bytes for the checksum
    let mut nibble_data: [u8; 344] = [0; 344];

    // The following was recommended from cargo-clippy, it's the equivalent of a
    // loop over the entire data array:
    // for i in 0..=255 {
    //     nibble_data[i + 86] = data[i];
    // }
    // Copy a slice instead
    nibble_data[86..(0xFF + 0x56 + 1)].copy_from_slice(&data[..(0xFF + 1)]);

    let mut val: u8;

    for (i, nibble_item) in nibble_data.iter_mut().enumerate().take(0x56) {
        let ac_index: usize = (i + 0xAC) % 0x100;
        let index_56: usize = (i + 0x56) % 0x100;
        let index: usize = i % 0x100;
        val = (((data[ac_index] & 0x1) << 1) | ((data[ac_index] & 0x2) >> 1)) << 6;
        val |= (((data[index_56] & 0x1) << 1) | ((data[index_56] & 0x2) >> 1)) << 4;
        val |= (((data[index] & 0x1) << 1) | ((data[index] & 0x2) >> 1)) << 2;
        *nibble_item = val;
    }

    nibble_data[84] &= 0x3F;
    nibble_data[85] &= 0x3F;

    let mut checksum: u8 = 0;
    let mut saved_data: u8;
    for item in &mut nibble_data {
        saved_data = *item;
        *item ^= checksum;
        checksum = saved_data;
    }

    let final_data: [u8; 342] = nibble_data[0..=341]
        .iter()
        .map(|d| NIBBLE_WRITE_TABLE_6_AND_2[(d >> 2) as usize])
        .collect::<Vec<u8>>()
        .try_into()
        .unwrap();

    DataField {
        _prologue: [0xD5, 0xAA, 0xAD],
        data: final_data.to_vec(),
        checksum,
        _epilogue: [0xDE, 0xAA, 0xEB],
    }
}

/// Nibblize a slice of u8 data
pub fn nibblize_data(data: &[u8]) -> Vec<u8> {
    let mut output_data: Vec<u8> = Vec::new();

    let mut i = 0;
    debug!("Data length: {}", data.len());
    while (i + 256) < data.len() {
        let block = &data[i..=(i + 255)];
        output_data.append(&mut build_nibble_sector(block).data);
        i += 256;
    }

    if i > 0 {
        i -= 256;
    }
    let block = &data[i..data.len()];
    output_data.append(&mut build_nibble_sector(block).data);

    output_data
}

/// A single track on the disk
#[derive(Default)]
pub struct Track {
    /// The sectors on the disk
    pub sectors: BTreeMap<u8, Sector>,
}

/// A single volume on the disk
#[derive(Default)]
pub struct Volume {
    /// The tracks on the disk
    pub tracks: BTreeMap<u8, Track>,
}

/// A Nibble encoded disk
/// (although this is generic enough a module-wide data structure
/// could be used)
#[derive(Default)]
pub struct NibbleDisk {
    /// The sectors on the disk
    pub volumes: BTreeMap<u8, Volume>,
}

// impl DiskImageParser for NibbleDisk {
//     fn parse_disk_image<'a>(
//         &self,
//         config: &Config,
//         filename: &str,
//         // data: &'a [u8],
//     ) -> IResult<&'a [u8], DiskImage<'a>> {
//         let guess_option = format_from_filename_and_data(filename, data);

//         match guess_option {
//             Some(guess) => {
//                 let (i, disk) = parse_nib_disk(config)(data)?;
//                 Ok((
//                     i,
//                     DiskImage::Apple(AppleDisk {
//                         encoding: guess.encoding,
//                         format: guess.format,
//                         data: AppleDiskData::Nibble(disk),
//                     }),
//                 ))
//             }
//             None => {
//                 panic!("Invalid format");
//             }
//         }
//     }
// }

impl DiskImageSaver for NibbleDisk {
    fn save_disk_image(
        &self,
        _config: &Config,
        _selected_filename: Option<&str>,
        filename: &str,
    ) -> std::result::Result<(), crate::error::Error> {
        let filename = PathBuf::from(filename);
        let file_result = File::create(filename);
        match file_result {
            Ok(mut file) => {
                for volume in self.volumes.values() {
                    for track in volume.tracks.values() {
                        for sector in track.sectors.values() {
                            file.write_all(&sector.data).unwrap();
                        }
                    }
                }
            }
            Err(e) => error!("Error opening file: {}", e),
        }
        Ok(())
    }
}

/// A field, containing both the data field and address field
pub struct Field {
    /// The address field, which contains volume, track and sector info indicating
    /// where this sector is located on the disk
    /// It also contains a checksum
    pub address_field: AddressField,
    /// The data field, which contains the data and checksum
    pub data_field: DataField,
}

/// Parse an address field, data field and build a Sector
pub fn parse_nib_sector(config: &Config) -> impl Fn(&[u8]) -> IResult<&[u8], Field> + '_ {
    move |i| {
        let (i, header) = find_and_parse_address_field(config)(i)?;
        let (i, data_field) = find_and_parse_data_field(i)?;

        Ok((
            i,
            Field {
                address_field: header,
                data_field,
            },
        ))
    }
}

/// Parse an entire nibble encoded disk
pub fn parse_nib_disk(config: &Config) -> impl Fn(&[u8]) -> IResult<&[u8], NibbleDisk> + '_ {
    move |i| {
        let (i, fields) = many0(parse_nib_sector(config))(i)?;

        debug!("Found {} fields", fields.len());
        let mut disk = NibbleDisk::default();

        for field in &fields {
            debug!("Parsing another field");
            let volume = disk.volumes.entry(field.address_field.volume);
            let track = volume.or_default().tracks.entry(field.address_field.track);
            let sector = track.or_default().sectors.entry(field.address_field.sector);
            sector.or_insert_with(|| transform_data_field(config, &field.data_field));
        }

        Ok((i, disk))
    }
}

#[cfg(test)]
mod tests {
    use super::{
        build_nibble_sector, data_field_build_buffer, find_and_parse_address_field,
        parse_nibble_byte_4_and_4, parse_prologue, transform_data_field, DataField,
        NIBBLE_WRITE_TABLE_6_AND_2,
    };
    use crate::config::{Config, Configuration};
    use pretty_assertions::assert_eq;

    /// Test nibble byte 4 and 4 parsing works
    #[test]
    fn parse_nibble_byte_4_and_4_works() {
        let volume_data: [u8; 2] = [0xFF, 0xFE];
        let track_data: [u8; 2] = [0xAB, 0xBF];
        let sector_data: [u8; 2] = [0xAA, 0xAF];
        let checksum_data: [u8; 2] = [0xFE, 0xEE];

        let zero_data: [u8; 2] = [0x00, 0x00];
        let one_data: [u8; 2] = [0x00, 0x01];

        let volume_result = parse_nibble_byte_4_and_4(&volume_data);
        match volume_result {
            Ok(volume) => {
                assert_eq!(volume.1, 0xFE);
            }
            Err(e) => {
                panic!("Parser failed: {}", e);
            }
        }

        let track_result = parse_nibble_byte_4_and_4(&track_data);
        match track_result {
            Ok(track) => {
                assert_eq!(track.1, 0x17);
            }
            Err(e) => {
                panic!("Parser failed: {}", e);
            }
        }

        let sector_result = parse_nibble_byte_4_and_4(&sector_data);
        match sector_result {
            Ok(sector) => {
                assert_eq!(sector.1, 0x05);
            }
            Err(e) => {
                panic!("Parser failed: {}", e);
            }
        }

        let checksum_result = parse_nibble_byte_4_and_4(&checksum_data);
        match checksum_result {
            Ok(checksum) => {
                assert_eq!(checksum.1, 0xEC);
            }
            Err(e) => {
                panic!("Parser failed: {}", e);
            }
        }

        let zero_result = parse_nibble_byte_4_and_4(&zero_data);
        match zero_result {
            Ok(zero) => {
                assert_eq!(zero.1, 0x00);
            }
            Err(e) => {
                panic!("Parser failed: {}", e);
            }
        }

        let one_result = parse_nibble_byte_4_and_4(&one_data);
        match one_result {
            Ok(one) => {
                assert_eq!(one.1, 0x01);
            }
            Err(e) => {
                panic!("Parser failed: {}", e);
            }
        }
    }

    /// Test find_and_parse_address_field with valid data
    #[test]
    fn find_and_parse_address_field_works() {
        // volume: 254, track: 23, sector: 5
        let address_field_data: [u8; 14] = [
            0xD5, 0xAA, 0x96, 0xFF, 0xFE, 0xAB, 0xBF, 0xAA, 0xAF, 0xFE, 0xEE, 0xDE, 0xAA, 0xEB,
        ];

        let config = Config::load(config::Config::default()).unwrap();
        let address_field_result = find_and_parse_address_field(&config)(&address_field_data);

        match address_field_result {
            Ok(address_field) => {
                assert_eq!(address_field.1.volume, 0xFE);
                assert_eq!(address_field.1.track, 0x17);
                assert_eq!(address_field.1.sector, 0x05);
                assert_eq!(address_field.1.checksum, 0xEC);
            }
            Err(e) => {
                panic!("Parsing error: {}", e);
            }
        }
    }

    /// Test that transform_data_field works
    /// TODO: Build some known checksum values
    #[test]
    pub fn transform_data_field_works() {
        let mut data: [u8; 342] = [0; 342];

        for i in 0..=341 {
            data[i] = NIBBLE_WRITE_TABLE_6_AND_2[(i % 0x40) as usize];
        }

        let data_field = DataField {
            _prologue: [0xD5, 0xAA, 0xAD],
            data: data.to_vec(),
            checksum: 0x96,
            _epilogue: [0xDE, 0xAA, 0xEB],
        };

        let (_buffer, checksum) = data_field_build_buffer(&data_field);

        assert_eq!(checksum, 1);
    }

    /// Do a round-trip test of nibblizing a sector and denibblizing
    /// it.  More work needs to be done on this.
    ///
    /// A full disk round-trip test may not be byte-for-byte equal.
    /// The nibblized data may be out of order, which is why data
    /// address headers are included.
    #[test]
    pub fn data_field_round_trip() {
        let mut original_data: [u8; 256] = [0; 256];

        for i in 0_u8..=255_u8 {
            original_data[i as usize] = i;
        }
        original_data[255] = 1;

        let data_field = build_nibble_sector(&original_data);

        let config = Config::load(config::Config::default()).unwrap();
        let sector = transform_data_field(&config, &data_field);

        assert_eq!(sector.data, original_data);
    }

    /// Test find_and_parse_address_field with invalid checksum
    #[test]
    #[should_panic(expected = "Address field computed checksum not equal to disk checksum: 236 0")]
    fn find_and_parse_address_field_panics_with_invalid_checksum() {
        // volume: 254, track: 23, sector: 5
        let address_field_data: [u8; 14] = [
            0xD5, 0xAA, 0x96, 0xFF, 0xFE, 0xAB, 0xBF, 0xAA, 0xAF, 0x00, 0x00, 0xDE, 0xAA, 0xEB,
        ];

        let config = Config::load(config::Config::default()).unwrap();
        let address_field_result = find_and_parse_address_field(&config)(&address_field_data);

        match address_field_result {
            Ok(_address_field) => {
                panic!("Should fail with checksum error");
            }
            Err(_e) => {
                panic!("Should fail with checksum error");
            }
        }
    }

    // Test address field prologue parsing and identification

    /// Test parsing a simple address field for a DOS 3.3 disk
    #[test]
    fn parse_prologue_dos_33_works() {
        let data = [0xD5, 0xAA, 0x96];

        let result = parse_prologue(&data);

        match result {
            Ok(r) => {
                assert_eq!(r.1, &[0xD5, 0xAA, 0x96]);
            }
            Err(_) => {
                panic!("Shouldn't fail parsing data");
            }
        }
    }

    /// Test parsing a simple address field for a DOS 3.3 disk
    /// Where the first header is several bytes in the data
    #[test]
    fn parse_prologue_dos_33_skip_works() {
        let data = [0x00, 0x00, 0xD5, 0xAA, 0x96];

        let result = parse_prologue(&data);

        match result {
            Ok(r) => {
                assert_eq!(r.1, &[0xD5, 0xAA, 0x96]);
            }
            Err(_) => {
                panic!("Shouldn't fail parsing data");
            }
        }
    }

    /// Test parsing a simple address field for a DOS 3.2 disk
    #[test]
    fn parse_prologue_dos_32_works() {
        let data = [0xD5, 0xAA, 0xB5];

        let result = parse_prologue(&data);

        match result {
            Ok(r) => {
                assert_eq!(r.1, &[0xD5, 0xAA, 0xB5]);
            }
            Err(_) => {
                panic!("Shouldn't fail parsing data");
            }
        }
    }

    /// Test parsing a prologe on data without one
    #[test]
    fn parse_no_prologue_fails() {
        let data = [0x00, 0x00, 0x00];

        let result = parse_prologue(&data);

        match result {
            Ok(_) => {
                panic!("Should fail parsing data");
            }
            Err(e) => match e {
                nom::Err::Incomplete(nom::Needed::Unknown) => {
                    assert_eq!("Parsing requires more data", e.to_string());
                }
                _ => {
                    panic!("Wrong parsing error");
                }
            },
        }
    }

    /// Test parsing a prologue on data that matches the first two
    /// prologue bytes but not the last.
    #[test]
    fn parse_incomplete_prologue_fails() {
        let data = [0xD5, 0xAA, 0x00];

        let result = parse_prologue(&data);

        match result {
            Ok(_) => {
                panic!("Should fail parsing data");
            }
            Err(e) => match e {
                nom::Err::Incomplete(nom::Needed::Unknown) => {
                    assert_eq!("Parsing requires more data", e.to_string());
                }
                _ => {
                    panic!("Wrong parsing error");
                }
            },
        }
    }

    /// Test parsing a prologue on data that matches the first two
    /// prologue bytes with no data left fails.
    #[test]
    fn parse_incomplete_prologue_two_bytes_fails() {
        let data = [0xD5, 0xAA];

        let result = parse_prologue(&data);

        match result {
            Ok(_) => {
                panic!("Should fail parsing data");
            }
            Err(e) => match e {
                nom::Err::Incomplete(nom::Needed::Unknown) => {
                    assert_eq!("Parsing requires more data", e.to_string());
                }
                _ => {
                    panic!("Wrong parsing error");
                }
            },
        }
    }
}
