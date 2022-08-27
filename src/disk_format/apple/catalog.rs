use nom::error::ErrorKind;
use nom::{Err, IResult};

use nom::bytes::complete::take;
use nom::multi::count;
use nom::number::complete::{le_u16, le_u8};

use std::fmt::{Display, Formatter, Result};
use std::string::FromUtf8Error;

use crate::serialize::{little_endian_word_to_bytes, Serializer};

/// Different file types
#[derive(Clone, Copy, Debug)]
pub enum FileType {
    /// Text file
    Text = 0,
    /// Integer BASIC file
    IntegerBasic = 1,
    /// AppleSoft BASIC file
    AppleSoftBasic = 2,
    /// Binary file
    Binary = 4,
    /// S Type file
    SType = 8,
    /// Relocatable Object Module file
    RelocatableObjectModule = 10,
    /// A Type file
    AType = 20,
    /// B Type file
    BType = 40,
    /// Unknown file type
    Unknown,
}

/// Display a FileType as a single character
impl Display for FileType {
    fn fmt(&self, f: &mut Formatter) -> Result {
        match self {
            FileType::Text => write!(f, "T"),
            FileType::IntegerBasic => write!(f, "I"),
            FileType::AppleSoftBasic => write!(f, "A"),
            FileType::Binary => write!(f, "B"),
            FileType::SType => write!(f, "S"),
            FileType::RelocatableObjectModule => write!(f, "R"),
            FileType::AType => write!(f, "A"),
            FileType::BType => write!(f, "B"),
            FileType::Unknown => write!(f, "U"),
        }
    }
}

/// A file entry
#[derive(Clone, Copy, Debug)]
pub struct FileEntry<'a> {
    /// Track index  of the start of the file
    pub track_of_first_track_sector_list_sector: u8,
    /// Sector index of the start of the file
    pub sector_of_first_track_sector_list_sector: u8,

    /// The file type
    pub file_type: FileType,

    /// Whether the file is locked
    pub locked: bool,

    /// The file name, bytes 03-20
    pub file_name: &'a [u8],

    /// The file length in number of sectors
    pub file_length_in_sectors: u16,
}

/// Custom implmentations for the FileEntry structure
/// These patterns would be useful for the FAT parser and other
/// resource-constrained and performance-oriented target codebases,
/// as long as caching isn't required
/// A separate data structure could be added to cache the result of these
/// filename calculations
impl<'a> FileEntry<'a> {
    /// Create a new FileEntry with the given data
    ///
    /// # Examples
    ///
    /// ```
    /// use image_rider::disk_format::apple::catalog::{FileEntry, FileType};
    ///
    /// let fe = FileEntry::new(0x12, 0x0F, FileType::AppleSoftBasic, false, "HELLO", 0x0002);
    /// assert_eq!(fe.filename().unwrap(), "HELLO");
    /// ```
    pub fn new(
        track_of_first_track_sector_list_sector: u8,
        sector_of_first_track_sector_list_sector: u8,
        file_type: FileType,
        locked: bool,
        filename: &str,
        file_length_in_sectors: u16,
    ) -> FileEntry {
        FileEntry {
            track_of_first_track_sector_list_sector,
            sector_of_first_track_sector_list_sector,
            file_type,
            locked,
            file_name: filename.as_bytes(),
            file_length_in_sectors,
        }
    }

    /// Return the filename as a String
    pub fn filename(&self) -> std::result::Result<String, FromUtf8Error> {
        let filename_vector: Vec<u8> = self
            .file_name
            .iter()
            .map(|c| if *c > 0x80 { *c - 0x80 } else { *c })
            .collect();
        let file_name = String::from_utf8(filename_vector)?;

        // Apple DOS disks use spaces as padding at the end
        // Remove the spaces from the end
        let file_name = String::from(file_name.trim_end_matches(' '));
        Ok(file_name)
    }
}

impl<'a> Serializer<'a> for FileEntry<'a> {
    fn as_vec(&'a self) -> std::result::Result<Vec<u8>, crate::error::Error> {
        let mut bytes: Vec<u8> = Vec::new();

        bytes.push(self.track_of_first_track_sector_list_sector);
        bytes.push(self.sector_of_first_track_sector_list_sector);

        let file_type = if self.locked {
            self.file_type as u8 + 0x80
        } else {
            self.file_type as u8
        };

        bytes.push(file_type);

        let num_bytes = self.file_name.len();
        // This may be misusing the ErrorKind::Invalid type
        if (num_bytes == 0) || (num_bytes > 30) {
            return Err(crate::error::Error::new(crate::error::ErrorKind::Invalid(
                crate::error::InvalidErrorKind::Invalid(format!(
                    "Filename size is invalid: {}",
                    num_bytes
                )),
            )));
        }

        let mut padding: Vec<u8> = vec![0; 30 - num_bytes];

        padding.fill(0xA0);

        let mut converted_filename: Vec<u8> =
            self.file_name.to_vec().iter().map(|c| c + 0x80).collect();

        bytes.append(&mut converted_filename);
        bytes.append(&mut padding);
        bytes.append(&mut little_endian_word_to_bytes(
            self.file_length_in_sectors,
        ));

        Ok(bytes)
    }

    // /// Optimize this with something, trait of a trait or whatever
    // fn as_bytes(&'a self) -> &[u8] {
    //     (*self).as_bytes()
    // }
}

/// Format a FileEntry for display
impl Display for FileEntry<'_> {
    fn fmt(&self, f: &mut Formatter) -> Result {
        writeln!(
            f,
            "{:>3} {:>3} {} {:>3} {:<30}",
            self.track_of_first_track_sector_list_sector,
            self.sector_of_first_track_sector_list_sector,
            self.file_type,
            self.file_length_in_sectors,
            self.filename().unwrap_or_else(|_| String::from("")),
        )
    }
}

/// Parse a file entry
pub fn parse_file_entry(i: &[u8]) -> IResult<&[u8], FileEntry> {
    let (i, track_of_first_track_sector_list_sector) = le_u8(i)?;
    let (i, sector_of_first_track_sector_list_sector) = le_u8(i)?;

    let (i, file_type) = le_u8(i)?;

    // The file type code the disk contains information about the
    // file type and also whether the file is locked.  If the file is
    // locked, bit seven is set.
    let locked = (file_type & 0x80) != 0;

    let file_type = match file_type & 0x7F {
        0 => FileType::Text,
        1 => FileType::IntegerBasic,
        2 => FileType::AppleSoftBasic,
        4 => FileType::Binary,
        8 => FileType::SType,
        10 => FileType::RelocatableObjectModule,
        20 => FileType::AType,
        40 => FileType::BType,
        _ => FileType::Unknown,
    };
    let (i, filename) = take(30_usize)(i)?;
    let (i, file_length_in_sectors) = le_u16(i)?;

    Ok((
        i,
        FileEntry {
            track_of_first_track_sector_list_sector,
            sector_of_first_track_sector_list_sector,
            file_type,
            locked,
            file_name: filename,
            file_length_in_sectors,
        },
    ))
}

/// Returns a successful result if this is a valid file entry
/// Otherwise returns an error
pub fn valid_file_entry(i: &[u8]) -> IResult<&[u8], bool> {
    let (i, res1) = le_u8(i)?;
    let (i, res2) = le_u8(i)?;

    if (res1 != 0) && (res2 != 0) {
        Ok((i, true))
    } else {
        Err(Err::Error(nom::error_position!(i, ErrorKind::Fail)))
    }
}

/// The disk catalog
pub struct Catalog<'a> {
    /// One reserved byte
    pub reserved: u8,
    /// The track number of the next catalog sector
    pub track_number_of_next_sector: u8,
    /// The sector number of the next catalog sector
    pub sector_number_of_next_sector: u8,
    /// Eight reserved bytes
    pub reserved_2: &'a [u8],

    /// Up to seven file descriptive entries
    pub file_entries: Vec<FileEntry<'a>>,
}

/// Format a Catalog for display
impl Display for Catalog<'_> {
    fn fmt(&self, f: &mut Formatter) -> Result {
        writeln!(
            f,
            "track number of next sector: {}",
            self.track_number_of_next_sector
        )?;
        writeln!(
            f,
            "sector number of next sector: {}",
            self.sector_number_of_next_sector
        )?;
        for file_entry in &self.file_entries {
            write!(f, "file: {}", file_entry)?;
        }
        writeln!(f)
    }
}

/// Return true if this is a valid allocated undeleted file
pub fn valid_file(track_of_first_track_sector_list_sector: u8) -> bool {
    // Unallocated files are set to 0x00 for the location
    // Deleted files are set to 0xFF for the location
    (track_of_first_track_sector_list_sector != 0x00)
        && (track_of_first_track_sector_list_sector != 0xFF)
}

/// Parse an Apple ][ DOS disk catalog
pub fn parse_catalog(i: &[u8]) -> IResult<&[u8], Catalog> {
    let (i, reserved) = le_u8(i)?;
    let (i, track_number_of_next_sector) = le_u8(i)?;
    let (i, sector_number_of_next_sector) = le_u8(i)?;
    let (i, reserved_2) = take(8_usize)(i)?;
    // We can also use many_till here to parse out the entries until there is a
    // file entry with zero for track and sector list entries

    // let (i, file_entries) = many_till(parse_file_entry, valid_file_entry)(i)?;

    let (i, file_entries) = count(parse_file_entry, 7)(i)?;

    let file_entries = file_entries
        .iter()
        .filter(|fe| valid_file(fe.track_of_first_track_sector_list_sector))
        .copied()
        .collect();

    Ok((
        i,
        Catalog {
            reserved,
            track_number_of_next_sector,
            sector_number_of_next_sector,
            reserved_2,
            file_entries,
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::{parse_catalog, parse_file_entry, FileEntry, FileType};
    use crate::serialize::Serializer;
    use nom::AsBytes;

    /// Returns a 35-byte file entry with a given filename
    fn file_entry_as_bytes(
        file_entry: &FileEntry,
    ) -> std::result::Result<[u8; 35], crate::error::Error> {
        Ok(file_entry.as_vec()?.as_bytes().try_into().unwrap())
    }

    /// Test that parsing a file entry works
    #[test]
    fn parse_file_entry_works() {
        let data: [u8; 35] = [
            0x12, 0x0F, 0x02, 0xC8, 0xC5, 0xCC, 0xCC, 0xCF, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0,
            0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0,
            0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0x02, 0x00,
        ];

        let result = parse_file_entry(&data);

        match result {
            Ok(file_entry) => {
                assert_eq!(file_entry.1.track_of_first_track_sector_list_sector, 18);
                assert_eq!(file_entry.1.sector_of_first_track_sector_list_sector, 15);
                match file_entry.1.file_type {
                    FileType::AppleSoftBasic => {
                        assert_eq!(true, true);
                    }
                    _ => {
                        panic!("Invalid file type parsed");
                    }
                }
                assert!(!file_entry.1.locked);
                assert_eq!(
                    file_entry.1.file_name,
                    [
                        0xC8, 0xC5, 0xCC, 0xCC, 0xCF, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0,
                        0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0,
                        0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0,
                    ]
                );
            }
            Err(e) => {
                panic!("Error parsing: {}", e);
            }
        }
    }

    /// Test that parsing a locked file entry works
    #[test]
    fn parse_file_entry_locked_works() {
        let data: [u8; 35] = [
            0x12, 0x0F, 0x82, 0xC8, 0xC5, 0xCC, 0xCC, 0xCF, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0,
            0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0,
            0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0x02, 0x00,
        ];

        let result = parse_file_entry(&data);

        match result {
            Ok(file_entry) => {
                assert_eq!(file_entry.1.track_of_first_track_sector_list_sector, 18);
                assert_eq!(file_entry.1.sector_of_first_track_sector_list_sector, 15);
                match file_entry.1.file_type {
                    FileType::AppleSoftBasic => {
                        assert_eq!(true, true);
                    }
                    _ => {
                        panic!("Invalid file type parsed");
                    }
                }
                assert!(file_entry.1.locked);
                assert_eq!(
                    file_entry.1.file_name,
                    [
                        0xC8, 0xC5, 0xCC, 0xCC, 0xCF, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0,
                        0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0,
                        0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0,
                    ]
                );
            }
            Err(e) => {
                panic!("Error parsing: {}", e);
            }
        }
    }

    /// Test that serializing a file entry works
    #[test]
    fn serialize_file_entry_works() {
        let expected_data: [u8; 35] = [
            0x12, 0x0F, 0x02, 0xC8, 0xC5, 0xCC, 0xCC, 0xCF, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0,
            0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0,
            0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0x02, 0x00,
        ];

        let data = file_entry_as_bytes(&FileEntry::new(
            0x12,
            0x0F,
            FileType::AppleSoftBasic,
            false,
            "HELLO",
            0x0002,
        ));

        assert_eq!(data.unwrap(), expected_data);
    }

    /// Test that serializing a file entry works
    #[test]
    fn serialize_locked_file_entry_works() {
        let expected_data: [u8; 35] = [
            0x12, 0x0F, 0x82, 0xC8, 0xC5, 0xCC, 0xCC, 0xCF, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0,
            0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0,
            0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0x02, 0x00,
        ];

        let data = file_entry_as_bytes(&FileEntry::new(
            0x12,
            0x0F,
            FileType::AppleSoftBasic,
            true,
            "HELLO",
            0x0002,
        ));

        assert_eq!(data.unwrap(), expected_data);
    }

    /// Test that serializing a file entry with a zero length filename
    /// works.
    /// Decide whether this should be a type constraint
    #[test]
    fn serialize_file_name_len_0_file_entry_fails() {
        let file_entry = FileEntry::new(0x12, 0x0F, FileType::AppleSoftBasic, false, "", 0x0002);

        let file_entry_as_vec = file_entry.as_vec();

        match file_entry_as_vec {
            Ok(_) => panic!("Shouldn't be a valid FileEntry"),
            Err(e) => assert_eq!(
                e.to_string(),
                "Image is invalid: Filename size is invalid: 0"
            ),
        }
    }

    /// Test that serializing a file entry with a one length filename
    /// works
    #[test]
    fn serialize_file_name_len_1_file_entry_works() {
        let expected_data: [u8; 35] = [
            0x12, 0x0F, 0x02, 0xC8, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0,
            0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0,
            0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0x02, 0x00,
        ];

        let data = file_entry_as_bytes(&FileEntry::new(
            0x12,
            0x0F,
            FileType::AppleSoftBasic,
            false,
            "H",
            0x0002,
        ));

        assert_eq!(data.unwrap(), expected_data);
    }

    /// Test that serializing a file entry with a 30 length filename
    /// works
    #[test]
    fn serialize_file_name_len_30_file_entry_works() {
        let expected_data: [u8; 35] = [
            0x12, 0x0F, 0x02, 0xB0, 0xB1, 0xB2, 0xB3, 0xB4, 0xB5, 0xB6, 0xB7, 0xB8, 0xB9, 0xB0,
            0xB1, 0xB2, 0xB3, 0xB4, 0xB5, 0xB6, 0xB7, 0xB8, 0xB9, 0xB0, 0xB1, 0xB2, 0xB3, 0xB4,
            0xB5, 0xB6, 0xB7, 0xB8, 0xB9, 0x02, 0x00,
        ];

        let data = file_entry_as_bytes(&FileEntry::new(
            0x12,
            0x0F,
            FileType::AppleSoftBasic,
            false,
            "012345678901234567890123456789",
            0x0002,
        ));

        assert_eq!(data.unwrap(), expected_data);
    }

    /// Test that serializing a file entry with a 30 length filename
    /// works
    #[test]
    fn serialize_file_name_len_31_file_entry_fails() {
        let file_entry = FileEntry::new(
            0x12,
            0x0F,
            FileType::AppleSoftBasic,
            false,
            "0123456789012345678901234567890",
            0x0002,
        );

        let file_entry_as_vec = file_entry.as_vec();

        match file_entry_as_vec {
            Ok(_) => panic!("Shouldn't be a valid FileEntry"),
            Err(e) => assert_eq!(
                e.to_string(),
                "Image is invalid: Filename size is invalid: 31"
            ),
        }
    }

    /// Test that converting a filename works
    #[test]
    fn file_entry_filename_works() {
        let data: [u8; 35] = [
            0x12, 0x0F, 0x02, 0xC8, 0xC5, 0xCC, 0xCC, 0xCF, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0,
            0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0,
            0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0x02, 0x00,
        ];

        let result = parse_file_entry(&data);

        match result {
            Ok(file_entry) => match file_entry.1.filename() {
                Ok(filename) => {
                    assert_eq!(filename, "HELLO");
                }
                Err(e) => {
                    panic!("Invalid filename: {}", e);
                }
            },
            Err(e) => {
                panic!("Error parsing: {}", e);
            }
        }
    }

    /// Test that parsing a catalog works
    #[test]
    fn parse_catalog_one_file_works() {
        // catalog header with a single file
        let data_header: [u8; 46] = [
            0x00, 0x11, 0x0E, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x12, 0x0F, 0x02,
            0xC8, 0xC5, 0xCC, 0xCC, 0xCF, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0,
            0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0,
            0xA0, 0xA0, 0x02, 0x00,
        ];
        let data_footer: [u8; 210] = [0; 210];
        let mut data: Vec<u8> = Vec::new();

        data.extend(data_header);
        data.extend(data_footer);

        let result = parse_catalog(&data);

        match result {
            Ok(catalog) => {
                assert_eq!(catalog.1.file_entries.len(), 1);
                let file_entry = catalog.1.file_entries.first().unwrap_or_else(|| {
                    panic!("Error getting file entry");
                });
                let filename = file_entry.filename().unwrap_or_else(|e| {
                    panic!("Error getting file name: {}", e);
                });
                assert_eq!(filename, "HELLO");
            }
            Err(e) => {
                panic!("Error parsing: {}", e);
            }
        }
    }

    /// Test that parsing a catalog with two files works
    #[test]
    fn parse_catalog_two_files_works() {
        // catalog header with a single file
        let data_header: [u8; 81] = [
            0x00, 0x11, 0x0E, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x12, 0x0F, 0x02,
            0xC8, 0xC5, 0xCC, 0xCC, 0xCF, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0,
            0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0,
            0xA0, 0xA0, 0x02, 0x00, 0x12, 0x0F, 0x02, 0xC8, 0xC5, 0xCC, 0xD0, 0xA0, 0xA0, 0xA0,
            0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0,
            0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0xA0, 0x02, 0x00,
        ];
        let data_footer: [u8; 210] = [0; 210];
        let mut data: Vec<u8> = Vec::new();

        data.extend(data_header);
        data.extend(data_footer);

        let result = parse_catalog(&data);

        match result {
            Ok(catalog) => {
                assert_eq!(catalog.1.file_entries.len(), 2);

                let file_entry = catalog.1.file_entries.first().unwrap_or_else(|| {
                    panic!("Error getting file entry");
                });
                let filename = file_entry.filename().unwrap_or_else(|e| {
                    panic!("Error getting file name: {}", e);
                });
                assert_eq!(filename, "HELLO");
                let file_entry = catalog
                    .1
                    .file_entries
                    .get(1)
                    .ok_or_else(|| {
                        panic!("Error getting file entry");
                    })
                    .unwrap_or_else(|_e| {
                        panic!("Error getting file entry");
                    });
                let filename = file_entry.filename().unwrap_or_else(|e| {
                    panic!("Error getting file name: {}", e);
                });
                assert_eq!(filename, "HELP");
            }
            Err(e) => {
                panic!("Error parsing: {}", e);
            }
        }
    }
}
