//! Disk-level functions and data structures for Apple disks.
use log::{debug, error, info, warn};

use std::{
    cmp::min,
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
};

use config::Config;

use nom::bytes::complete::take;
use nom::multi::count;
use nom::number::complete::{le_i8, le_u16, le_u8};
use nom::{Err, IResult};

use std::fmt::{Display, Formatter, Result};

use crate::disk_format::apple::catalog::{build_files, parse_catalogs, Files, FullCatalog};
use crate::disk_format::apple::nibble::{parse_nib_disk, recognize_prologue};
use crate::disk_format::image::{DiskImage, DiskImageParser, DiskImageSaver};
use crate::disk_format::sanity_check::SanityCheck;
use crate::error::{Error, ErrorKind, InvalidErrorKind};

use super::nibble::NibbleDisk;

/// The different types of endoding wrappers for the disks
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Encoding {
    /// No special encoding, a raw disk image
    Plain,
    /// Nibble encoding for a disk
    Nibble,
}

/// Format an Encoding for display
impl Display for Encoding {
    fn fmt(&self, f: &mut Formatter) -> Result {
        write!(f, "{:?}", self)
    }
}

/// The different types of Apple disks
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Format {
    /// Unknown disk format.
    /// We may not have enough information at the current stage to know the format
    /// This is a simple data type so it should be fast to update
    ///
    /// There's a design decision here to use an Unknown enum variant
    /// as opposed to an Option with None.  I don't know if I made the
    /// right choice.
    Unknown(u64),
    /// Apple DOS (3.2)
    DOS32(u64),
    /// Apple DOS (3.3)
    DOS33(u64),
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
    /// Each bit map of free sectors for a track is four bytes long
    /// There is one for each track, usually 35 in DOS 3.3 disks
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

    // We'll read in as many as the VTOC says we have tracks, limited
    // to 50, which stays within the 256-byte limit.
    let bit_maps_to_read = min(number_of_tracks_per_diskette, 50);

    let (i, bit_map_of_free_sectors) = count(take(4_usize), bit_maps_to_read.into())(i)?;

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
            debug!(
                "Suspicious number of tracks per diskette: {}",
                self.number_of_tracks_per_diskette
            );
            return false;
        }

        if (self.number_of_sectors_per_track != 13) && (self.number_of_sectors_per_track != 16) {
            debug!(
                "Suspicious number of sectors per track: {}",
                self.number_of_sectors_per_track
            );
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
    pub catalog: FullCatalog<'a>,
    /// Disk tracks.
    /// Tracks is a vector of sectors, which is a vector of byte
    /// slices.
    pub tracks: Vec<Vec<&'a [u8]>>,

    /// The files with data
    pub files: Files<'a>,
}

/// The different types of Apple disks
/// We're ignoring the large_enum_variant warning for now, enum size is still less than
/// 512 bytes
/// On normal invocations in the current codebase we only have one
/// instance of this enum.  Future versions may have more, but for now
/// the cost is not an issue.
#[allow(clippy::large_enum_variant)]
pub enum AppleDiskData<'a> {
    /// An Apple ][ DOS disk (1.x, 2.x, 3.x)
    DOS(AppleDOSDisk<'a>),
    /// An Apple ][ ProDOS disk
    ProDOS,
    /// A nibble encoded disk (may contain a DOS image or other data)
    Nibble(NibbleDisk),
}

impl<'a> DiskImageSaver for AppleDOSDisk<'a> {
    fn save_disk_image(
        &self,
        _config: &Config,
        selected_filename: Option<&str>,
        filename: &str,
    ) -> std::result::Result<(), crate::error::Error> {
        if selected_filename.is_none() {
            error!("Filename must be specified for saving Apple DOS 3.3 images");
            return Err(crate::error::Error::new(ErrorKind::Message(String::from(
                "Filename must be specified for saving Apple DOS 3.3 images",
            ))));
        }
        let selected_filename = selected_filename.unwrap();
        let filename = PathBuf::from(filename);
        let file_result = File::create(filename);
        match file_result {
            Ok(mut file) => {
                let selected_file = self.files.get(selected_filename).unwrap();

                file.write_all(&selected_file.data)?;
            }
            Err(e) => error!("Error opening file: {}", e),
        }
        Ok(())
    }
}

/// An Apple ][ Disk
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

/// Heuristic guesses for what kind of disk this is
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AppleDiskGuess<'a> {
    /// The disk encoding
    pub encoding: Encoding,
    /// The disk format
    pub format: Format,
    /// The raw image data
    pub data: &'a [u8],
}

impl AppleDiskGuess<'_> {
    /// Return a new AppleDiskGuess with some default parameters that can't
    /// be easily guessed from basic heuristics like filename
    pub fn new(encoding: Encoding, format: Format, data: &[u8]) -> AppleDiskGuess {
        AppleDiskGuess {
            encoding,
            format,
            data,
        }
    }
}

/// Format an AppleDiskGuess for display
impl Display for AppleDiskGuess<'_> {
    fn fmt(&self, f: &mut Formatter) -> Result {
        write!(f, "encoding: {}, format: {}", self.encoding, self.format)
    }
}

/// Try to guess a file format from a filename
pub fn format_from_filename_and_data<'a>(
    filename: &str,
    data: &'a [u8],
) -> Option<AppleDiskGuess<'a>> {
    let filename_extension: Vec<_> = filename.split('.').collect();
    let path = Path::new(&filename);

    let filesize = match fs::metadata(path) {
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
        "do" => Some(AppleDiskGuess::new(
            Encoding::Plain,
            Format::DOS33(filesize),
            data,
        )),
        "dsk" => Some(AppleDiskGuess::new(
            Encoding::Plain,
            Format::DOS33(filesize),
            data,
        )),
        "nib" => {
            let prologue_byte_result = recognize_prologue(data);
            let format = match prologue_byte_result {
                Some(r) => match r {
                    0xB5 => Format::DOS32(filesize),
                    0x96 => Format::DOS33(filesize),
                    _ => Format::Unknown(filesize),
                },
                None => Format::Unknown(filesize),
            };

            Some(AppleDiskGuess::new(Encoding::Nibble, format, data))
        }
        &_ => None,
    }
}

/// Try to guess a file format from a magic number in the file
///
/// # Arguments
///
/// * `data` - A u8 slice containing the entire image data to guess
///
/// # Returns
///   Returns a result with an optional AppleDiskGuess, the meaning of this is:
///      Returns an Err result if there was an error parsing the data.
///      Returns an Ok result if the parsing was successful, even if
///      it's not a known image type.
///        Returns an Ok result with None if the image type is unknown.
///        Returns an Ok result with an AppleDiskGuess if it's a known type.
///
/// There was a design decision here to return None as opposed to an
/// Unknown Apple image type.  I don't know if it's the right choice.
pub fn format_from_data(data: &[u8]) -> core::result::Result<Option<AppleDiskGuess<'_>>, Error> {
    let filesize: u64 = data.len().try_into().unwrap();

    info!("Reading magic number from file");
    let (_i, header) = take(0x09_usize)(data)?;

    if header != [0x01, 0xA5, 0x27, 0xC9, 0x09, 0xD0, 0x18, 0xA5, 0x2B] {
        return Ok(None);
    }
    // Check for an Apple II DOS 3.3 header
    let (i, _junk) = take(0x11000_usize)(data)?;
    let (i, _reserved) = le_u8(i)?;
    let (i, track_number_of_first_catalog_sector) = le_u8(i)?;
    let (i, sector_number_of_first_catalog_sector) = le_u8(i)?;
    let (_i, release_number_of_dos) = le_u8(i)?;

    if (track_number_of_first_catalog_sector == 0x11)
        && (sector_number_of_first_catalog_sector == 0x0F)
        && (release_number_of_dos == 0x03)
    {
        info!("Found Apple DOS 3.3 disk");
        Ok(Some(AppleDiskGuess::new(
            Encoding::Plain,
            Format::DOS33(filesize),
            data,
        )))
    } else {
        Ok(None)
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
/// This parses the disks and returns the farthest index and a vector
/// of u8 slices or an error (it actually returns an IResult, which is
/// composed of this).
/// The index of the vector is the track number.
/// Each vector element is 4096 bytes long if tracks_per_disk is 35
/// It's 143360 / tracks_per_disk for a 140k disk.  That's all the
/// sectors for that track index.
pub fn apple_140_k_dos_parser(
    guess: AppleDiskGuess,
    tracks_per_disk: usize,
) -> IResult<&[u8], Vec<&[u8]>> {
    if tracks_per_disk == 35 {
        apple_tracks_parser(4096, 35)(guess.data)
    } else if tracks_per_disk == 40 {
        apple_tracks_parser(3584, 40)(guess.data)
    } else {
        Err(Err::Error(nom::error::Error::new(
            guess.data,
            nom::error::ErrorKind::Fail,
        )))
    }
}

/// Parse a DOS 3.3 disk volume
pub fn volume_parser(guess: AppleDiskGuess, filesize: u64) -> IResult<&[u8], AppleDisk> {
    // guess the tracks per disk
    let tracks_per_disk = 35;

    // guess the starting track for the catalog.
    // This sometimes starts at other locations.
    // The variable name is somewhat confusing, it's the track
    // where the catalog starts.
    let catalog_sector_start = 17;

    // 140K Apple DOS image
    // Use the apple_140_k_dos_parser
    // raw_tracks is a vector of all the tracks, NOT split into
    // separate sectors
    let (_i, raw_tracks) = apple_140_k_dos_parser(guess, tracks_per_disk)?;

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
    let (i, vtoc) = parse_volume_table_of_contents(raw_tracks[catalog_sector_start])?;

    debug!("VTOC: {}", vtoc);

    if !vtoc.check() {
        error!("Invalid data");
        return Err(Err::Error(nom::error::Error::new(
            i,
            nom::error::ErrorKind::Fail,
        )));
    }

    let mut tracks: Vec<Vec<&[u8]>> = Vec::new();

    // parse out the sectors for track 17
    // This parses through every sector in track catalog_sector_start
    // and splits it up into 16 sectors of 256 bytes each

    let catalog_sector = raw_tracks[catalog_sector_start][2];

    for track in raw_tracks {
        let mut track_vec: Vec<&[u8]> = Vec::new();
        let (_i, sectors) = count(take(256_usize), 16)(track)?;
        for sector in sectors {
            track_vec.push(sector);
        }
        tracks.push(track_vec);
    }

    let catalog_res = parse_catalogs(
        &tracks,
        catalog_sector_start.try_into().unwrap(),
        catalog_sector,
    );
    let catalog = match catalog_res {
        Ok(catalog) => catalog,
        Err(_e) => {
            return Err(Err::Error(nom::error::Error::new(
                i,
                nom::error::ErrorKind::Fail,
            )));
        }
    };

    debug!("Catalog:\n{}", catalog);

    // TODO: Properly convert errors and define an error for this
    let files = build_files(catalog.clone(), &tracks).unwrap();

    let apple_dos_disk = AppleDOSDisk {
        volume_table_of_contents: vtoc,
        catalog,
        tracks,
        files,
    };

    Ok((
        i,
        AppleDisk {
            encoding: Encoding::Plain,
            format: Format::DOS33(filesize),
            data: AppleDiskData::DOS(apple_dos_disk),
        },
    ))
}

/// Parse an Apple ][ Disk
pub fn apple_disk_parser<'a>(
    guess: AppleDiskGuess<'a>,
    config: &Config,
) -> IResult<&'a [u8], AppleDisk<'a>> {
    let i = guess.data;

    debug!("Parsing based on guess: {}", guess);

    match guess.encoding {
        Encoding::Plain => {
            let filesize = if let Format::DOS33(size) = guess.format {
                size
            } else {
                0
            };

            if filesize == 143360 {
                volume_parser(guess, filesize)
            } else {
                // TODO: Refactor this, it's not really a nom error
                Err(Err::Error(nom::error::make_error(
                    i,
                    nom::error::ErrorKind::Fail,
                )))
            }
        }
        Encoding::Nibble => {
            debug!("Parsing as nibble format");
            let (i, disk) = parse_nib_disk(config)(i)?;

            return Ok((
                i,
                AppleDisk {
                    encoding: guess.encoding,
                    format: guess.format,
                    data: AppleDiskData::Nibble(disk),
                },
            ));
        }
    }
}

/// DiskImageParser implementation for AppleDiskGuess
impl<'a, 'b> DiskImageParser<'a, 'b> for AppleDiskGuess<'a> {
    fn parse_disk_image(
        &'a self,
        config: &'b Config,
        _filename: &str,
    ) -> std::result::Result<DiskImage<'a>, Error> {
        info!("DiskImageParser Attempting to parse Apple disk");
        let result = apple_disk_parser(*self, config);
        match result {
            Ok(apple_disk) => Ok(DiskImage::Apple(apple_disk.1)),
            Err(e) => Err(Error::new(ErrorKind::Invalid(InvalidErrorKind::Invalid(
                nom::Err::Error(e).to_string(),
            )))),
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
        apple_disk_parser, format_from_data, format_from_filename_and_data,
        parse_volume_table_of_contents, AppleDiskData, AppleDiskGuess, Encoding, Format,
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

    /// Try testing format_from_filename_and_data
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

        let guess = format_from_filename_and_data(filename, &data).unwrap_or_else(|| {
            panic!("Invalid filename guess");
        });

        assert_eq!(
            guess,
            AppleDiskGuess::new(Encoding::Plain, Format::DOS33(143360), &data)
        );

        std::fs::remove_file(filename).unwrap_or_else(|e| {
            panic!("Error removing test file: {}", e);
        });
    }

    /// Try testing format_from_data
    /// Right now, this only tests for Apple DOS 3.3 disk images using
    /// magic numbers at the start and in the VTOC
    #[test]
    fn format_from_data_works() {
        let mut data: [u8; 143360] = [0; 143360];
        let magic_number_start = [0x01, 0xA5, 0x27, 0xC9, 0x09, 0xD0, 0x18, 0xA5, 0x2B];
        let magic_number_vtoc = [0x11, 0x0F, 0x03];

        data[0..9].copy_from_slice(&magic_number_start);
        data[0x11001..0x11004].copy_from_slice(&magic_number_vtoc);

        let guess_res = format_from_data(&data);
        match guess_res {
            Ok(g) => {
                if let Some(guess) = g {
                    assert_eq!(
                        guess,
                        AppleDiskGuess::new(Encoding::Plain, Format::DOS33(143360), &data)
                    );
                } else {
                    panic!("Invalid data guess");
                }
            }
            Err(_) => {
                panic!("Data guess failed");
            }
        };
    }

    /// Try testing format_from_data fails with wrong magic number
    ///
    /// Right now, this only tests for Apple DOS 3.3 disk images using
    /// magic numbers at the start and in the VTOC
    ///
    /// This test should fail with invalid VTOC for DOS 3.3
    ///
    /// It may need to be changed when other filesystem guesses are
    /// impelemented, so it will be hopefully be helpful for the next
    /// maintainer or developer.
    #[test]
    fn format_from_data_fails() {
        let mut data: [u8; 143360] = [0; 143360];
        let magic_number_start = [0x01, 0xA5, 0x27, 0xC9, 0x09, 0xD0, 0x18, 0xA5, 0x2B];

        data[0..9].copy_from_slice(&magic_number_start);

        let guess_res = format_from_data(&data);
        match guess_res {
            Ok(g) => {
                if let Some(guess) = g {
                    assert_ne!(
                        guess,
                        AppleDiskGuess::new(Encoding::Plain, Format::DOS33(143360), &data)
                    );
                } else {
                    assert_eq!(g, None, "Correct data guess for invalid DOS 3.3 data");
                }
            }
            Err(_) => {
                panic!("Data guess failed");
            }
        };
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

        let guess = AppleDiskGuess::new(Encoding::Plain, Format::DOS33(143360), &data);

        let config = Config::default();
        let res = apple_disk_parser(guess, &config);

        match res {
            Ok(disk) => match disk.1.data {
                AppleDiskData::DOS(apple_dos_disk) => {
                    let vtoc = apple_dos_disk.volume_table_of_contents;

                    assert_eq!(disk.1.encoding, Encoding::Plain);
                    assert_eq!(disk.1.format, Format::DOS33(143360));
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

        let guess = AppleDiskGuess::new(Encoding::Plain, Format::DOS33(143360), &data);

        let config = Config::default();
        let res = apple_disk_parser(guess, &config);

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
