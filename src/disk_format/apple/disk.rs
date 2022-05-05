/// Disk-level functions and data structures for Apple disks.
use log::{debug, error};

use std::fs;
use std::path::Path;

use config::Config;

use nom::bytes::complete::take;
use nom::error::ErrorKind;
use nom::multi::count;
use nom::number::complete::{le_i8, le_u16, le_u8};
use nom::{Err, IResult};

use std::fmt::{Display, Formatter, Result};

use crate::disk_format::apple::catalog::{parse_catalog, Catalog};
use crate::disk_format::apple::nibble::parse_nib_disk;
use crate::disk_format::sanity_check::SanityCheck;

use super::nibble::NibbleDisk;

/// The different types of endoding wrappers for the disks
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Encoding {
    /// No special encoding, a raw disk image
    Plain,
    /// Nibble encoding for a disk
    Nibble,
}

/// Format a Format for display
impl Display for Encoding {
    fn fmt(&self, f: &mut Formatter) -> Result {
        write!(f, "{:?}", self)
    }
}

/// The different types of Apple disks
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Format {
    /// Unknown disk format.
    /// We may not have enough information at the current stage to know the format
    /// This is a simple data type so it should be fast to update
    Unknown(u64),
    /// Apple DOS (3.3)
    DOS(u64),
    /// Apple ProDOS
    ProDOS(u64),
}

/// Format a Format for display
impl Display for Format {
    fn fmt(&self, f: &mut Formatter) -> Result {
        write!(f, "{:?}", self)
    }
}

/// The Volume Table of Contents (VTOC)
/// The VTOC contains
pub struct VolumeTableOfContents<'a> {
    /// Reserved
    pub reserved: u8,

    /// Track number of first catalog sector
    pub track_number_of_first_catalog_sector: u8,
    /// Sector number of first catalog sector
    pub sector_number_of_first_catalog_sector: u8,

    /// Release number of DOS used to initialize disk
    pub release_number_of_dos: u8,
    /// Reserved
    /// 2 bytes
    pub reserved2: &'a [u8],
    /// Diskette volume number (1-254)
    pub diskette_volume_number: u8,
    /// Reserved
    /// 0x20 bytes
    pub reserved3: &'a [u8],

    /// Maximum number of track/sector pairs which will fit in
    /// one file track/sector list sector (122 for 256 byte sectors)
    pub maximum_number_of_track_sector_pairs: u8,

    /// Reserved
    /// 8 bytes
    pub reserved4: &'a [u8],

    /// Last track where sectors were allocated
    pub last_track_where_sectors_were_allocated: u8,
    /// Direction of track allocation, +1 or -1
    pub direction_of_track_allocation: i8,

    /// Reserved
    /// 2 bytes
    pub reserved5: &'a [u8],

    /// Number of tracks per diskette (normally 35)
    pub number_of_tracks_per_diskette: u8,
    /// Number of sectors per track (13 or 16)
    pub number_of_sectors_per_track: u8,
    /// Number of bytes per sector (little endian format)
    pub number_of_bytes_per_sector: u16,

    /// bytes 0x38 - 0xFF
    pub bit_map_of_free_sectors: Vec<&'a [u8]>,
}

/// Format a Format for display
impl Display for VolumeTableOfContents<'_> {
    fn fmt(&self, f: &mut Formatter) -> Result {
        writeln!(
            f,
            "track number of first catalog sector: {}",
            self.track_number_of_first_catalog_sector
        )?;
        writeln!(
            f,
            "sector number of first catalog sector: {}",
            self.sector_number_of_first_catalog_sector
        )?;
        writeln!(f, "release number of DOS: {}", self.release_number_of_dos)?;
        writeln!(f, "diskette volume number: {}", self.diskette_volume_number)?;

        writeln!(
            f,
            "number of tracks per diskette: {}",
            self.number_of_tracks_per_diskette
        )?;
        writeln!(
            f,
            "number of sectors per track: {}",
            self.number_of_sectors_per_track
        )?;
        writeln!(
            f,
            "number of bytes per sector: {}",
            self.number_of_bytes_per_sector
        )?;
        writeln!(
            f,
            "last_track_where_sectors_were_allocated: {}",
            self.last_track_where_sectors_were_allocated
        )
    }
}

/// Parse a Volume Table of Contents
pub fn parse_volume_table_of_contents(i: &[u8]) -> IResult<&[u8], VolumeTableOfContents> {
    let (i, reserved) = le_u8(i)?;
    let (i, track_number_of_first_catalog_sector) = le_u8(i)?;
    let (i, sector_number_of_first_catalog_sector) = le_u8(i)?;
    let (i, release_number_of_dos) = le_u8(i)?;
    let (i, reserved2) = take(2_usize)(i)?;
    let (i, diskette_volume_number) = le_u8(i)?;
    let (i, reserved3) = take(32_usize)(i)?;
    let (i, maximum_number_of_track_sector_pairs) = le_u8(i)?;
    let (i, reserved4) = take(8_usize)(i)?;
    let (i, last_track_where_sectors_were_allocated) = le_u8(i)?;
    let (i, direction_of_track_allocation) = le_i8(i)?;
    let (i, reserved5) = take(2_usize)(i)?;
    let (i, number_of_tracks_per_diskette) = le_u8(i)?;
    let (i, number_of_sectors_per_track) = le_u8(i)?;
    let (i, number_of_bytes_per_sector) = le_u16(i)?;

    let (i, bit_map_of_free_sectors) = count(take(4_usize), 50_usize)(i)?;

    Ok((
        i,
        VolumeTableOfContents {
            reserved,
            track_number_of_first_catalog_sector,
            sector_number_of_first_catalog_sector,
            release_number_of_dos,
            reserved2,
            diskette_volume_number,
            reserved3,
            maximum_number_of_track_sector_pairs,
            reserved4,
            last_track_where_sectors_were_allocated,
            direction_of_track_allocation,
            reserved5,
            number_of_tracks_per_diskette,
            number_of_sectors_per_track,
            number_of_bytes_per_sector,
            bit_map_of_free_sectors,
        },
    ))
}

impl SanityCheck for VolumeTableOfContents<'_> {
    fn check(&self) -> bool {
        if (self.number_of_tracks_per_diskette != 35) && (self.number_of_tracks_per_diskette != 40)
        {
            debug!("Suspicious number of tracks per diskette");
            return false;
        }

        if (self.number_of_sectors_per_track != 13) && (self.number_of_sectors_per_track != 16) {
            debug!("Suspicious number of tracks per diskette");
            return false;
        }

        true
    }
}

/// An Apple ][ DOS Disk
pub struct AppleDOSDisk<'a> {
    /// The Volume Table of Contents
    pub volume_table_of_contents: VolumeTableOfContents<'a>,
    /// The disk catalog
    pub _catalog: Catalog<'a>,
    /// Disk tracks
    pub _tracks: Vec<&'a [u8]>,
}

/// The different types of Apple disks
pub enum AppleDiskData<'a> {
    /// An Apple ][ DOS disk (1.x, 2.x, 3.x)
    DOS(AppleDOSDisk<'a>),
    /// An Apple ][ ProDOS disk
    ProDOS,
    /// A nibble encoded disk (may contain a DOS image or other data)
    Nibble(NibbleDisk),
}

/// An Apple ][ Disk
//#[derive(Debug)]
pub struct AppleDisk<'a> {
    /// The disk encoding
    pub encoding: Encoding,
    /// The disk format
    pub format: Format,

    /// The parsed disk data
    pub data: AppleDiskData<'a>,
}

/// Format an AppleDisk for display
impl Display for AppleDisk<'_> {
    fn fmt(&self, f: &mut Formatter) -> Result {
        write!(f, "encoding: {}, format: {}", self.encoding, self.format)
    }
}

// impl DiskImageParser for AppleDisk<'_> {
//     fn parse_disk_image<'a>(config: &Config, filename: &str, data: &[u8])
//                         -> IResult<&'a [u8], DiskImage<'a>> {
//         let guess = format_from_filename(filename);

//         let (i, disk) = apple_disk_parser(guess, config)(data)?;
//         Ok((i, DiskImage::Apple(disk)))
//     }
//     info!(
//         "config ignore-checksums: {:?}",
//         config.get_bool("ignore-checksums")
//     );

//     match guess_image_type {
//         Some(i) => match i {
//             DiskImageGuess::Apple(guess) => {
//                 let result = apple_disk_parser(Some(guess), config)(data);

//                 match result {
//                     Ok(r) => Ok(result.0, DiskImage::Apple(result.1)),
//                     Err(e) => Err(e),
//             }
//             _ => Err("Invalid disk format"),
//         },
//         None => Err("Invalid disk format")
//     }
// }

// }

/// Heuristic guesses for what kind of disk this is
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AppleDiskGuess {
    /// The disk encoding
    pub encoding: Encoding,
    /// The disk format
    pub format: Format,
}

/// Format an AppleDiskGuess for display
impl Display for AppleDiskGuess {
    fn fmt(&self, f: &mut Formatter) -> Result {
        write!(f, "encoding: {}, format: {}", self.encoding, self.format)
    }
}

/// Try to guess a file format from a filename
pub fn format_from_filename(filename: &str) -> Option<AppleDiskGuess> {
    let filename_extension: Vec<_> = filename.split('.').collect();
    let path = Path::new(&filename);

    let filesize = match fs::metadata(&path) {
        Ok(metadata) => metadata.len(),
        Err(e) => {
            error!("Couldn't get file metadata: {}", e);
            panic!("Couldn't get file metadata");
        }
    };

    match filename_extension[filename_extension.len() - 1]
        .to_lowercase()
        .as_str()
    {
        "dsk" => Some(AppleDiskGuess {
            encoding: Encoding::Plain,
            format: Format::DOS(filesize),
        }),
        "nib" => Some(AppleDiskGuess {
            encoding: Encoding::Nibble,
            format: Format::Unknown(filesize),
        }),
        &_ => None,
    }
}

/// Parse the tracks on an Apple ][ Disk
pub fn apple_tracks_parser(
    track_size: usize,
    number_of_tracks: usize,
) -> impl Fn(&[u8]) -> IResult<&[u8], Vec<&[u8]>> {
    move |i| count(take(track_size), number_of_tracks)(i)
}

/// Parse the tracks on a 140K Apple ][ Disk
pub fn apple_140_k_dos_parser(i: &[u8]) -> IResult<&[u8], Vec<&[u8]>> {
    apple_tracks_parser(4096, 35)(i)
    // apple_tracks_parser(3584, 40)(i)
}

/// Parse an Apple ][ Disk
pub fn apple_disk_parser(
    guess: Option<AppleDiskGuess>,
    config: &Config,
) -> impl Fn(&[u8]) -> IResult<&[u8], AppleDisk> + '_ {
    move |i| {
        if let Some(e) = &guess {
            let filesize = if let Format::DOS(size) = e.format {
                size
            } else {
                let (i, disk) = parse_nib_disk(config)(i)?;

                return Ok((
                    i,
                    AppleDisk {
                        encoding: e.encoding,
                        format: e.format,
                        data: AppleDiskData::Nibble(disk),
                    },
                ));
                // if config.output {
                //     disk.save_disk_image(config, &config.output);
                // }
            };

            if filesize == 143360 {
                // 140K Apple DOS image
                // Guess 35 tracks per disk
                let (_i, tracks) = apple_140_k_dos_parser(i)?;

                debug!("number of tracks: {}", tracks.len());
                // Verify that this is the Volume Table of Contents
                // The catalog should start on sector 17
                // Sometimes this is zero-based indexing, sometimes it's one-based

                // One heuristic is to check if byte 1 is equal to 17,
                // the standard track number of the first catalog
                // sector.
                // byte 2 is usually equal to 15, the sector number of
                // the first catalog sector
                // Another heuristic is to check for a valid DOS release number:
                // DOS versions to check for: 1, 2, 3
                let (i, vtoc) = parse_volume_table_of_contents(tracks[17])?;

                debug!("VTOC: {}", vtoc);

                if !vtoc.check() {
                    error!("Invalid data");
                    return Err(Err::Error(nom::error::Error::new(i, ErrorKind::Fail)));
                    // return Err(Err::Error(nom::error_position!(i, ErrorKind::Fail)));
                }

                // parse out the sectors for track 17
                let (_i, sectors) = count(take(256_usize), 16)(tracks[17])?;
                let catalog_sector = tracks[17][2];
                let (_i2, catalog) = parse_catalog(sectors[catalog_sector as usize])?;

                debug!("Catalog: {}", catalog);

                let apple_dos_disk = AppleDOSDisk {
                    volume_table_of_contents: vtoc,
                    _catalog: catalog,
                    _tracks: tracks,
                };

                Ok((
                    i,
                    AppleDisk {
                        encoding: Encoding::Plain,
                        format: Format::DOS(filesize),
                        data: AppleDiskData::DOS(apple_dos_disk),
                    },
                ))
            } else {
                Err(Err::Error(nom::error_position!(i, ErrorKind::Fail)))
            }
        } else {
            Err(Err::Error(nom::error_position!(i, ErrorKind::Fail)))
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs::OpenOptions;
    use std::io::Write;
    use std::path::Path;

    use config::Config;

    use super::{
        apple_disk_parser, format_from_filename, parse_volume_table_of_contents, AppleDiskData,
        AppleDiskGuess, Encoding, Format,
    };

    const VTOC_DATA: [u8; 256] = [
        0x00, 0x11, 0x0F, 0x03, 0x00, 0x00, 0xFE, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x7A, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x12, 0x01, 0x00, 0x00, 0x23, 0x10, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xFF, 0xFF, 0x00, 0x00, 0xFF, 0xFF, 0x00,
        0x00, 0xFF, 0xFF, 0x00, 0x00, 0xFF, 0xFF, 0x00, 0x00, 0xFF, 0xFF, 0x00, 0x00, 0xFF, 0xFF,
        0x00, 0x00, 0xFF, 0xFF, 0x00, 0x00, 0xFF, 0xFF, 0x00, 0x00, 0xFF, 0xFF, 0x00, 0x00, 0xFF,
        0xFF, 0x00, 0x00, 0xFF, 0xFF, 0x00, 0x00, 0xFF, 0xFF, 0x00, 0x00, 0xFF, 0xFF, 0x00, 0x00,
        0xFF, 0xFF, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x3F, 0xFF, 0x00, 0x00, 0xFF, 0xFF, 0x00,
        0x00, 0xFF, 0xFF, 0x00, 0x00, 0xFF, 0xFF, 0x00, 0x00, 0xFF, 0xFF, 0x00, 0x00, 0xFF, 0xFF,
        0x00, 0x00, 0xFF, 0xFF, 0x00, 0x00, 0xFF, 0xFF, 0x00, 0x00, 0xFF, 0xFF, 0x00, 0x00, 0xFF,
        0xFF, 0x00, 0x00, 0xFF, 0xFF, 0x00, 0x00, 0xFF, 0xFF, 0x00, 0x00, 0xFF, 0xFF, 0x00, 0x00,
        0xFF, 0xFF, 0x00, 0x00, 0xFF, 0xFF, 0x00, 0x00, 0xFF, 0xFF, 0x00, 0x00, 0xFF, 0xFF, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00,
    ];

    /// Try testing format_from_filename
    #[test]
    fn format_from_filename_works() {
        let filename = "testdata/test-disk_format_from_filename_works.dsk";

        /* Version where we build the file in the test instead of
         * saving it to version control */
        let path = Path::new(&filename);
        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .open(path)
            .unwrap_or_else(|e| {
                panic!("Couldn't open file: {}", e);
            });
        let data: [u8; 143360] = [0; 143360];

        file.write_all(&data).unwrap_or_else(|e| {
            panic!("Error writing test file: {}", e);
        });
        file.flush().unwrap_or_else(|e| {
            panic!("Couldn't flush file stream: {}", e);
        });

        let guess = format_from_filename(filename).unwrap_or_else(|| {
            panic!("Invalid filename guess");
        });

        assert_eq!(
            guess,
            AppleDiskGuess {
                encoding: Encoding::Plain,
                format: Format::DOS(143360),
            }
        );

        std::fs::remove_file(filename).unwrap_or_else(|e| {
            panic!("Error removing test file: {}", e);
        });
    }

    /// Test parsing a Volume Table of Contents
    #[test]
    fn parse_volume_table_of_contents_works() {
        let vtoc_data = VTOC_DATA;

        let result = parse_volume_table_of_contents(&vtoc_data);

        match result {
            Ok(vtoc) => {
                assert_eq!(vtoc.1.track_number_of_first_catalog_sector, 17);
                assert_eq!(vtoc.1.sector_number_of_first_catalog_sector, 15);
                assert_eq!(vtoc.1.release_number_of_dos, 3);
                assert_eq!(vtoc.1.diskette_volume_number, 254);
                assert_eq!(vtoc.1.number_of_tracks_per_diskette, 35);
                assert_eq!(vtoc.1.number_of_sectors_per_track, 16);
                assert_eq!(vtoc.1.number_of_bytes_per_sector, 256);
                assert_eq!(vtoc.1.last_track_where_sectors_were_allocated, 18);
            }
            Err(e) => {
                panic!("Couldn't parse VTOC: {}", e);
            }
        }
    }

    /// Test parsing a non-standard Apple ][ DOS 3.3 disk
    /// A lot of these disks have custom code to and different locations for the VTOC
    /// Test collecting heuristics on Apple disk images
    #[test]
    fn apple_disk_parser_disk_works() {
        let filename = "testdata/test-apple_disk_parser_works.dsk";
        let path = Path::new(&filename);

        let mut data: Vec<u8> = Vec::new();
        let data_prefix: [u8; 0x11000] = [0; 0x11000];
        let data_vtoc = VTOC_DATA;
        let data_suffix: [u8; 0x11F00] = [0; 0x11F00];

        data.extend(data_prefix);
        data.extend(data_vtoc);
        data.extend(data_suffix);

        std::fs::write(&path, &data).unwrap_or_else(|e| {
            panic!("Error writing test file: {}", e);
        });

        // This may not work on GitHub Actions due to their CI
        // environment restrictions
        // let guess = format_from_filename(filename).unwrap_or_else(|| {
        //     panic!("Invalid filename guess");
        // });

        let guess = AppleDiskGuess {
            encoding: Encoding::Plain,
            format: Format::DOS(143360),
        };

        let res = apple_disk_parser(Some(guess), &Config::default())(&data);

        match res {
            Ok(disk) => match disk.1.data {
                AppleDiskData::DOS(apple_dos_disk) => {
                    let vtoc = apple_dos_disk.volume_table_of_contents;

                    assert_eq!(disk.1.encoding, Encoding::Plain);
                    assert_eq!(disk.1.format, Format::DOS(143360));
                    assert_eq!(vtoc.track_number_of_first_catalog_sector, 17);
                    assert_eq!(vtoc.sector_number_of_first_catalog_sector, 15);
                    assert_eq!(vtoc.release_number_of_dos, 3);
                    assert_eq!(vtoc.diskette_volume_number, 254);
                    assert_eq!(vtoc.number_of_tracks_per_diskette, 35);
                    assert_eq!(vtoc.number_of_sectors_per_track, 16);
                    assert_eq!(vtoc.number_of_bytes_per_sector, 256);
                    assert_eq!(vtoc.last_track_where_sectors_were_allocated, 18);
                }
                _ => {
                    panic!("Invalid format");
                }
            },
            Err(_e) => {
                panic!("This should have succeeded");
            }
        }

        std::fs::remove_file(filename).unwrap_or_else(|e| {
            panic!("Error removing test file: {}", e);
        });
    }

    /// Test parsing a non-standard Apple ][ DOS 3.3 disk
    /// A lot of these disks have custom code to and different locations for the VTOC
    /// Test collecting heuristics on Apple disk images
    #[test]
    fn apple_disk_parser_nonstandard_disk_panics() {
        let filename = "testdata/test-apple_disk_parser_nonstandard_disk_panics.dsk";

        /* Version where we build the file in the test instead of
         * saving it to version control */
        let path = Path::new(&filename);
        let data: [u8; 143360] = [0; 143360];
        std::fs::write(&path, data).unwrap_or_else(|e| {
            panic!("Error writing test file: {}", e);
        });

        // let guess = format_from_filename(filename).unwrap_or_else(|| {
        //     panic!("Invalid filename guess");
        // });
        // assert_eq!(
        //     guess,
        //     AppleDiskGuess {
        //         encoding: Encoding::Plain,
        //         format: Format::DOS(143360),
        //     }
        // );
        let guess = AppleDiskGuess {
            encoding: Encoding::Plain,
            format: Format::DOS(143360),
        };

        let res = apple_disk_parser(Some(guess), &Config::default())(&data);

        match res {
            Ok(_disk) => {
                panic!("This should have failed parsing");
            }
            Err(_e) => {
                // Check for more specific error result
                assert_eq!(true, true);
            }
        }

        std::fs::remove_file(filename).unwrap_or_else(|e| {
            panic!("Error removing test file: {}", e);
        });
    }
}
