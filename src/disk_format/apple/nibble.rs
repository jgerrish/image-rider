/// Encoding and Decoding Nibble-based disk formats
use std::collections::BTreeMap;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

use config::Config;
use log::error;

use nom::bytes::complete::{take, take_until};
use nom::multi::many0;
use nom::number::complete::le_u8;
use nom::IResult;

use crate::disk_format::apple::disk::{format_from_filename, AppleDisk, AppleDiskData};
use crate::disk_format::image::{DiskImage, DiskImageParser};

// const NIBBLE_WRITE_TABLE_6_AND_2: [u8; 64] = [
//     0x96,0x97,0x9A,0x9B,0x9D,0x9E,0x9F,0xA6,
//     0xA7,0xAB,0xAC,0xAD,0xAE,0xAF,0xB2,0xB3,
//     0xB4,0xB5,0xB6,0xB7,0xB9,0xBA,0xBB,0xBC,
//     0xBD,0xBE,0xBF,0xCB,0xCD,0xCE,0xCF,0xD3,
//     0xD6,0xD7,0xD9,0xDA,0xDB,0xDC,0xDD,0xDE,
//     0xDF,0xE5,0xE6,0xE7,0xE9,0xEA,0xEB,0xEC,
//     0xED,0xEE,0xEF,0xF2,0xF3,0xF4,0xF5,0xF6,
//     0xF7,0xF9,0xFA,0xFB,0xFC,0xFD,0xFE,0xFF
// ];

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
        // let (_i, volume_data) = take(2_usize)(i)?;
        let (i, volume) = parse_nibble_byte_4_and_4(i)?;
        // let (_i, track_data) = take(2_usize)(i)?;
        let (i, track) = parse_nibble_byte_4_and_4(i)?;
        // let (_i, sector_data) = take(2_usize)(i)?;
        let (i, sector) = parse_nibble_byte_4_and_4(i)?;
        // let (_i, checksum_data) = take(2_usize)(i)?;
        let (i, checksum) = parse_nibble_byte_4_and_4(i)?;
        let (i, _epilogue) = take(3_usize)(i)?;

        // debug!(
        //     "Found address field: volume: {:?}, {}, track: {:?} {}, sector: {:?} {}, checksum: {:?} {}",
        //     volume_data, volume, track_data, track, sector_data, sector, checksum_data, checksum
        // );

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
            if !config.get_bool("ignore-checksums").unwrap_or(false) {
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
    /// 342 bytes of data, encoded as 6 and 2
    pub data: Vec<u8>,
    /// The checksum of the data
    pub checksum: u8,
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
    let (i, _prologue) = take(3_usize)(i)?;
    let (i, data) = take(342_usize)(i)?;
    let (i, checksum) = le_u8(i)?;
    // let (i, _epilogue) = tag(&[0xDE, 0xAA, 0xEB][..])(i)?;
    let (i, _epilogue) = take(3_usize)(i)?;

    Ok((
        i,
        DataField {
            data: data.to_vec(),
            checksum,
        },
    ))
}

/// A 256-byte 8-bit data structure computed from 6 and 2 data
#[derive(Clone)]
pub struct Sector {
    /// The data
    pub data: Vec<u8>,
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
    // let mut data = parse_6_and_2_nibblized_data(data);

    let mut computed_checksum: u8 = 0;
    // let mut xor_value = 0;
    let data_field_data_size = data_field.data.len();
    let mut buffer = [0; 342];
    let mut data = [0; 256];

    // Optimize later
    // First, understand the algorithm and disk data structure
    for (index, byte) in data_field.data.iter().enumerate() {
        computed_checksum ^= NIBBLE_READ_TABLE_6_AND_2[(*byte) as usize];
        if index < 0x56 {
            // The - 1 is probably optimized away by the compiler
            buffer[data_field_data_size - index - 1] = computed_checksum;
        } else {
            buffer[index - 0x56] = computed_checksum;
        }
    }

    if computed_checksum != data_field.checksum {
        error!(
            "Invalid checksum on data: calculated: {}, disk: {}",
            computed_checksum, data_field.checksum
        );
        if !config.get_bool("ignore-checksums").unwrap_or(false) {
            panic!("Invalid checksum on data");
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

impl DiskImageParser for NibbleDisk {
    fn parse_disk_image<'a>(
        config: &Config,
        filename: &str,
        data: &'a [u8],
    ) -> IResult<&'a [u8], DiskImage<'a>> {
        let guess_option = format_from_filename(filename);

        match guess_option {
            Some(guess) => {
                let (i, disk) = parse_nib_disk(config)(data)?;
                Ok((
                    i,
                    DiskImage::Apple(AppleDisk {
                        encoding: guess.encoding,
                        format: guess.format,
                        data: AppleDiskData::Nibble(disk),
                    }),
                ))
            }
            None => {
                panic!("Invalid format");
            }
        }
    }

    fn save_disk_image(&self, _config: &Config, filename: &str) {
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
/// TODO: Maybe use one of the fold combinators
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

        let mut disk = NibbleDisk::default();

        for field in &fields {
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
    use super::{find_and_parse_address_field, parse_nibble_byte_4_and_4};
    use config::Config;

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

        let config = Config::default();
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

    /// Test find_and_parse_address_field with invalid checksum
    #[test]
    #[should_panic(expected = "Address field computed checksum not equal to disk checksum: 236 0")]
    fn find_and_parse_address_field_panics_with_invalid_checksum() {
        // volume: 254, track: 23, sector: 5
        let address_field_data: [u8; 14] = [
            0xD5, 0xAA, 0x96, 0xFF, 0xFE, 0xAB, 0xBF, 0xAA, 0xAF, 0x00, 0x00, 0xDE, 0xAA, 0xEB,
        ];

        let config = Config::default();
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
}
