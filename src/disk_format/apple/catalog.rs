//! Catalog structures and files for an Apple DOS 3.3 disk
use log::debug;

use nom::error::ErrorKind;
use nom::{Err, IResult};

use nom::bytes::complete::take;
use nom::multi::count;
use nom::number::complete::{le_u16, le_u8};

use std::{
    collections::HashMap,
    fmt::{Display, Formatter, Result},
    string::FromUtf8Error,
};

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
            FileType::AType => write!(f, "AT"),
            FileType::BType => write!(f, "BT"),
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

    /// The file name, bytes 0x03-0x20 (30 bytes)
    pub file_name: &'a [u8],

    /// The file length in number of sectors
    pub file_length_in_sectors: u16,
}

/// A File
/// Has a filename, Track / Sector List and data for the file.
pub struct File<'a> {
    /// The track sector lists for this file
    track_sector_lists: TrackSectorLists<'a>,

    /// The file data
    pub data: Vec<u8>,
}

impl Display for File<'_> {
    fn fmt(&self, f: &mut Formatter) -> Result {
        for tsl in &self.track_sector_lists {
            writeln!(f, "track_sector_list: {}", tsl)?;
        }
        writeln!(f, "length of data: {}", self.data.len())
    }
}

/// Files are a collection of File objects indexed by filename.
pub type Files<'a> = HashMap<String, File<'a>>;

/// A track/sector list.
/// Each file has an associated track/sector
/// list.  There may be more track/sector lists.
#[derive(Clone)]
pub struct TrackSectorList<'a> {
    /// Reserved byte
    pub reserved: u8,
    /// The track number of the next track/sector list, or None if
    /// there is none.
    pub track_number_of_next_sector: Option<u8>,
    /// The sector number of the next track/sector list, or None if
    /// there is none.
    pub sector_number_of_next_sector: Option<u8>,
    /// Two reserved bytes
    pub reserved_2: &'a [u8],

    /// Sector offset in file of the first sector described by this list
    /// Two bytes
    pub sector_offset_in_file: &'a [u8],

    /// 5 reserved bytes
    pub reserved_3: &'a [u8],

    /// Vector of TrackSectorPairs for this TrackSectorList
    pub track_sector_pairs: TrackSectorPairs, // Vec<TrackSectorPair>,
}

/// Display a FileType as a single character
impl Display for TrackSectorList<'_> {
    fn fmt(&self, f: &mut Formatter) -> Result {
        write!(f, "reserved: {}", self.reserved)?;
        match self.track_number_of_next_sector {
            Some(x) => {
                writeln!(f, "track_number_of_next_sector: 0x{:02X}", x)?;
            }
            None => {
                writeln!(f, "track_number_of_next_sector: None")?;
            }
        }
        match self.sector_number_of_next_sector {
            Some(x) => {
                writeln!(f, "sector_number_of_next_sector: 0x{:02X}, ", x)?;
            }
            None => {
                writeln!(f, "sector_number_of_next_sector: None")?;
            }
        }
        write!(f, "reserved_2: {:?}", self.reserved_2)?;
        writeln!(f, "Track Sector Pairs:")?;
        for tsp in &self.track_sector_pairs {
            writeln!(f, "track_sector_pair: {}", tsp)?;
        }
        writeln!(f)
    }
}

impl<'a> Serializer<'a> for TrackSectorList<'a> {
    fn as_vec(&'a self) -> std::result::Result<Vec<u8>, crate::error::Error> {
        let mut bytes = Vec::new();

        bytes.push(self.reserved);
        // TODO: Test this
        if let Some(track_number) = self.track_number_of_next_sector {
            bytes.push(track_number);
        } else {
            bytes.push(0);
        }
        if let Some(sector_number) = self.sector_number_of_next_sector {
            bytes.push(sector_number);
        } else {
            bytes.push(0);
        }

        bytes.append(&mut self.reserved_2.to_vec());
        bytes.append(&mut self.sector_offset_in_file.to_vec());
        bytes.append(&mut self.reserved_3.to_vec());
        bytes.append(&mut self.track_sector_pairs.as_vec().unwrap());

        Ok(bytes)
    }
}

/// A vector of track sector lists
// #[derive(Debug)]
pub type TrackSectorLists<'a> = Vec<TrackSectorList<'a>>;

/// Parse a track / sector list.
pub fn parse_track_sector_list(i: &[u8]) -> IResult<&[u8], TrackSectorList> {
    let mut track_sector_pairs: Vec<TrackSectorPair> = Vec::new();

    let (i, reserved) = le_u8(i)?;
    let (i, track_number_of_next_sector) = le_u8(i)?;
    // TODO: See if there is a more ergonomic way of doing this
    let track_number_of_next_sector = if track_number_of_next_sector != 0 {
        Some(track_number_of_next_sector)
    } else {
        None
    };

    let (i, sector_number_of_next_sector) = le_u8(i)?;
    let sector_number_of_next_sector = if sector_number_of_next_sector != 0 {
        Some(sector_number_of_next_sector)
    } else {
        None
    };

    let (i, reserved_2) = take(2_usize)(i)?;
    let (i, sector_offset_in_file) = take(2_usize)(i)?;
    let (i, reserved_3) = take(5_usize)(i)?;

    let (mut i, mut track_sector_pair) = parse_track_sector_pair(i)?;

    let max_tsps = 121;
    let mut cnt = 1;
    while (track_sector_pair.track_number != 0) && (cnt <= max_tsps) {
        track_sector_pairs.push(track_sector_pair);
        let (i2, tsp) = parse_track_sector_pair(i)?;
        track_sector_pair = tsp;
        i = i2;
        cnt += 1;
    }

    Ok((
        i,
        TrackSectorList {
            reserved,
            track_number_of_next_sector,
            sector_number_of_next_sector,
            reserved_2,
            sector_offset_in_file,
            reserved_3,
            track_sector_pairs,
        },
    ))
}

/// Pairs of track and sector numbers used in Track/Sector Lists
#[derive(Clone, Copy, Debug)]
pub struct TrackSectorPair {
    /// The track number
    pub track_number: u8,
    /// The sector number
    pub sector_number: u8,
}

impl Display for TrackSectorPair {
    fn fmt(&self, f: &mut Formatter) -> Result {
        write!(
            f,
            "track_number: 0x{:02X}, sector_number: 0x{:02X}",
            self.track_number, self.sector_number
        )
    }
}

impl<'a> Serializer<'a> for TrackSectorPair {
    fn as_vec(&'a self) -> std::result::Result<Vec<u8>, crate::error::Error> {
        let bytes: Vec<u8> = vec![self.track_number, self.sector_number];

        Ok(bytes)
    }
}

/// TrackSectorPairs is a list / vector of TrackSectorPair
pub type TrackSectorPairs = Vec<TrackSectorPair>;

impl<'a> Serializer<'a> for TrackSectorPairs {
    fn as_vec(&'a self) -> std::result::Result<Vec<u8>, crate::error::Error> {
        let mut bytes: Vec<u8> = Vec::new();

        for tsp in self {
            bytes.append(&mut tsp.as_vec().unwrap());
        }

        Ok(bytes)
    }
}

/// Parse a track/sector pair
pub fn parse_track_sector_pair(i: &[u8]) -> IResult<&[u8], TrackSectorPair> {
    let (i, track_number) = le_u8(i)?;
    let (i, sector_number) = le_u8(i)?;

    Ok((
        i,
        TrackSectorPair {
            track_number,
            sector_number,
        },
    ))
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

    /// Get the data for a file
    pub fn get_data(
        &self,
        tracks: &[Vec<&[u8]>],
        track_sector_lists: &TrackSectorLists,
    ) -> std::result::Result<Vec<u8>, crate::error::Error> {
        // We could build a custom iterator for TrackSectorList, since the
        // normal meaning of iterating through a TrackSectorList is iterating
        // through TrackSectorPairs
        // let data: Vec<u8> = track_sector_lists
        //     .into_iter()
        //     .flat_map(|tsl| tsl.track_sector_pairs.clone())
        //     .flat_map(|tsp| tracks[tsp.track_number as usize][tsp.sector_number as usize])
        //     .map(|b| *b)
        //     .collect::<Vec<u8>>();
        let data: Vec<u8> = track_sector_lists
            .iter()
            .flat_map(|tsl| tsl.track_sector_pairs.clone())
            .flat_map(|tsp| tracks[tsp.track_number as usize][tsp.sector_number as usize])
            .copied()
            .collect();

        match self.file_type {
            FileType::Binary => {
                if data.len() >= 4 {
                    let (i, address) = le_u16(data.as_slice())?;
                    debug!("Binary file address: {}", address);
                    let (_i, len) = le_u16(i)?;
                    debug!("Binary file length: {}", len);
                    // Some additional checking
                    if (data.len() - 4) >= len.into() {
                        Ok(data[4..(len + 4) as usize].to_vec())
                    } else {
                        Ok(data)
                    }
                } else {
                    Ok(data)
                }
            }
            _ => {
                let error = crate::error::Error::new(crate::error::ErrorKind::Invalid(
                    crate::error::InvalidErrorKind::Invalid(format!(
                        "Unsupported file type for export: {}",
                        self.file_type
                    )),
                ));
                debug!("{}", error);
                Err(error)
            }
        }
    }

    /// Build a file from a file entry
    /// TODO: Get the tracks / sectors down correctly
    /// E.g. tracks are a vector of sectors
    pub fn build_file(
        &self,
        tracks: &[Vec<&'a [u8]>],
    ) -> std::result::Result<TrackSectorLists<'a>, crate::error::Error> {
        let mut track_sector_lists: TrackSectorLists = Vec::new();

        let track = self.track_of_first_track_sector_list_sector;
        let sector = self.sector_of_first_track_sector_list_sector;

        // There is always at least one track and sector list for a file
        let (_i, track_sector_list) =
            parse_track_sector_list(tracks[track as usize][sector as usize]).unwrap();
        track_sector_lists.push(track_sector_list.clone());

        let mut track = track_sector_list.clone().track_number_of_next_sector;
        let mut sector = track_sector_list.clone().sector_number_of_next_sector;
        debug!("track sector list: {}", track_sector_lists.first().unwrap());

        while track.is_some() {
            debug!(
                "TSList track {}, sector {}",
                track.unwrap(),
                sector.unwrap()
            );
            let (_i, track_sector_list) =
                parse_track_sector_list(tracks[track.unwrap() as usize][sector.unwrap() as usize])
                    .unwrap();
            track = track_sector_list.track_number_of_next_sector;
            sector = track_sector_list.sector_number_of_next_sector;
            track_sector_lists.push(track_sector_list);
        }

        Ok(track_sector_lists)
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
}

/// A FileEntry with associated file data
/// Should rename this.
pub struct FullFile<'a> {
    /// The associated FileEntry for this file.
    pub file_entry: FileEntry<'a>,
    /// The raw data for this file, not including metadata or
    /// structured information.
    pub data: Vec<u8>,
    /// The address this file was at in memory .
    pub address: u16,
    /// The length of this file in bytes.
    pub length: u16,
}

/// This serializes a File to a block of memory, encoding things like
/// the address and length for a binary file as header bytes Or
/// padding with carriage returns or null bytes.
impl<'a> Serializer<'a> for FullFile<'a> {
    fn as_vec(&'a self) -> std::result::Result<Vec<u8>, crate::error::Error> {
        let mut bytes: Vec<u8> = Vec::new();
        match self.file_entry.file_type {
            FileType::Binary => {
                bytes.append(&mut little_endian_word_to_bytes(self.address));
                bytes.append(&mut little_endian_word_to_bytes(self.length));
                // TODO: Fix this
                bytes.append(&mut self.data.clone());

                Ok(bytes)
            }
            _ => Err(crate::error::Error::new(
                crate::error::ErrorKind::Unimplemented(format!(
                    "Unsupported file tyep for serialization: {}",
                    self.file_entry.file_type
                )),
            )),
        }
    }
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
#[derive(Clone)]
pub struct Catalog<'a> {
    /// One reserved byte
    pub reserved: u8,
    /// The track number of the next catalog sector
    /// When this is zero, it's the end of the catalog chain
    pub track_number_of_next_sector: u8,
    /// The sector number of the next catalog sector
    pub sector_number_of_next_sector: u8,
    /// Eight reserved bytes
    pub reserved_2: &'a [u8],

    /// Up to seven file descriptive entries
    pub file_entries: Vec<FileEntry<'a>>,

    /// The files in the catalog indexed by filename
    pub catalog_by_filename: HashMap<String, FileEntry<'a>>,
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
            write!(f, "{}", file_entry)?;
        }
        writeln!(f)
    }
}

impl<'a> Serializer<'a> for Catalog<'a> {
    fn as_vec(&'a self) -> std::result::Result<Vec<u8>, crate::error::Error> {
        let mut v: Vec<u8> = Vec::new();

        v.push(self.reserved);
        v.push(self.track_number_of_next_sector);
        v.push(self.sector_number_of_next_sector);

        v.append(&mut self.reserved_2.to_vec());
        let mut file_entries: Vec<u8> = self
            .file_entries
            .iter()
            .flat_map(|fe| fe.as_vec())
            .flatten()
            .collect();

        let padding_len = (7 - self.file_entries.len()) * 35;
        let mut padding: Vec<u8> = vec![0; padding_len];

        v.append(&mut file_entries);
        v.append(&mut padding);

        Ok(v)
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

    let file_entries: Vec<FileEntry> = file_entries
        .iter()
        .filter(|fe| valid_file(fe.track_of_first_track_sector_list_sector))
        .copied()
        .collect();

    // debug!("file_entries: {:?}", file_entries);

    let mut catalog_by_filename: HashMap<String, FileEntry> = HashMap::new();

    file_entries.iter().for_each(|fe| {
        catalog_by_filename.insert(fe.filename().unwrap(), *fe);
    });

    Ok((
        i,
        Catalog {
            reserved,
            track_number_of_next_sector,
            sector_number_of_next_sector,
            reserved_2,
            file_entries,
            catalog_by_filename,
        },
    ))
}

/// A FullCatalog combines several Catalog sectors with FileEntries
/// into a single catalog without the metadata
#[derive(Clone, Debug)]
pub struct FullCatalog<'a> {
    /// Up to seven file descriptive entries
    pub file_entries: Vec<FileEntry<'a>>,

    /// The files in the catalog indexed by filename
    pub catalog_by_filename: HashMap<String, FileEntry<'a>>,
}

/// Format a Catalog for display
impl Display for FullCatalog<'_> {
    fn fmt(&self, f: &mut Formatter) -> Result {
        for file_entry in &self.file_entries {
            write!(f, "{}", file_entry)?;
        }
        writeln!(f)
    }
}

/// Parse a series of catalog sectors
/// This parses all of the catalog sectors and builds a directory of files
pub fn parse_catalogs<'a>(
    tracks: &[Vec<&'a [u8]>],
    catalog_track: u8,
    catalog_sector: u8,
) -> std::result::Result<FullCatalog<'a>, crate::error::Error> {
    let mut file_entries: Vec<FileEntry> = Vec::new();
    let mut catalog_by_filename: HashMap<String, FileEntry> = HashMap::new();

    let (_i, mut catalog) = parse_catalog(tracks[catalog_track as usize][catalog_sector as usize])?;

    // Show info about the tracks data structure
    debug!("tracks length: {}", tracks.len());
    debug!("track one length: {}", tracks[0].len());

    // debug!("Number of files: {}", &catalog.file_entries.len());
    for file in &catalog.file_entries {
        file_entries.push(*file);
        catalog_by_filename.insert(file.filename().unwrap(), *file);
        // debug!("Filename: {}", file.filename().unwrap());
    }
    // debug!("catalog: {}", catalog.clone());

    // The first track and first sector usually contain the DOS boot
    // code (or a boot stub), so they cannot be used as a catalog
    // sector.
    while (catalog.track_number_of_next_sector != 0) && (catalog.sector_number_of_next_sector != 0)
    {
        let (_i, c) = parse_catalog(
            tracks[catalog.track_number_of_next_sector as usize]
                [catalog.sector_number_of_next_sector as usize],
        )?;

        debug!("parsed another catalog: {}", c);

        catalog = c;
        for file in &catalog.file_entries {
            file_entries.push(*file);
            catalog_by_filename.insert(file.filename().unwrap(), *file);
        }
    }

    Ok(FullCatalog {
        file_entries,
        catalog_by_filename,
    })
}

impl Catalog<'_> {
    /// Get the file data for a file in the catalog
    pub fn get_file(&self, filename: &str) -> Vec<u8> {
        let _file_entry = self.catalog_by_filename.get(filename).unwrap();

        let data: Vec<u8> = Vec::new();
        data
    }
}

/// Build the files in the catalog
pub fn build_files<'a>(
    catalog: FullCatalog<'a>,
    tracks: &[Vec<&'a [u8]>],
) -> std::result::Result<Files<'a>, crate::error::Error> {
    let mut files: Files = HashMap::new();

    for file_entry in &catalog.file_entries {
        let track_sector_lists = file_entry.build_file(tracks)?;
        debug!("Building file: {}", file_entry.filename().unwrap());
        let res = file_entry.get_data(tracks, &track_sector_lists);
        let data = res.unwrap_or_default();

        files.insert(
            file_entry.filename().unwrap(),
            File {
                track_sector_lists,
                data,
            },
        );
    }
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::{
        build_files, parse_catalog, parse_catalogs, parse_file_entry, Catalog, FileEntry, FileType,
        TrackSectorList, TrackSectorPair, TrackSectorPairs,
    };
    use crate::serialize::{little_endian_word_to_bytes, Serializer};
    use nom::AsBytes;
    use pretty_assertions::assert_eq;
    use std::collections::HashMap;

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

    /// Test that serializing a TrackSectorPair works.
    #[test]
    fn serialize_track_sector_pair_works() {
        let tsp = TrackSectorPair {
            track_number: 0x12,
            sector_number: 0x34,
        };

        let data = tsp.as_vec().unwrap();

        assert_eq!(data.len(), 2);
        assert_eq!(data[0], 0x12);
        assert_eq!(data[1], 0x34);
    }

    /// Test that serializing a TrackSectorList with zero
    /// TrackSectorPair works
    #[test]
    fn serialize_track_sector_list_with_zero_track_sector_pair_works() {
        let tsl = TrackSectorList {
            reserved: 0x01,
            track_number_of_next_sector: None,
            sector_number_of_next_sector: None,
            reserved_2: &[0x02, 0x03],
            sector_offset_in_file: &[0x04, 0x05],
            reserved_3: &[0x06, 0x07, 0x08, 0x09, 0x10],
            track_sector_pairs: Vec::new(),
        };

        let data = tsl.as_vec().unwrap();

        assert_eq!(data.len(), 12);
        assert_eq!(data[0], 0x01);
        assert_eq!(data[1], 0x00);
        assert_eq!(data[2], 0x00);
        assert_eq!(data[3], 0x02);
        assert_eq!(data[4], 0x03);
        assert_eq!(data[5], 0x04);
        assert_eq!(data[6], 0x05);
        assert_eq!(data[7], 0x06);
        assert_eq!(data[8], 0x07);
        assert_eq!(data[9], 0x08);
        assert_eq!(data[10], 0x09);
        assert_eq!(data[11], 0x10);
    }

    /// Test that serializing a TrackSectorList with one
    /// TrackSectorPair works
    #[test]
    fn serialize_track_sector_list_with_one_track_sector_pair_works() {
        let tsp = TrackSectorPair {
            track_number: 0x12,
            sector_number: 0x34,
        };
        let tsps = Vec::from([tsp]);

        let tsl = TrackSectorList {
            reserved: 0x01,
            track_number_of_next_sector: None,
            sector_number_of_next_sector: None,
            reserved_2: &[0x02, 0x03],
            sector_offset_in_file: &[0x04, 0x05],
            reserved_3: &[0x06, 0x07, 0x08, 0x09, 0x10],
            track_sector_pairs: tsps,
        };

        let data = tsl.as_vec().unwrap();

        assert_eq!(data.len(), 14);
        assert_eq!(data[0], 0x01);
        assert_eq!(data[1], 0x00);
        assert_eq!(data[2], 0x00);
        assert_eq!(data[3], 0x02);
        assert_eq!(data[4], 0x03);
        assert_eq!(data[5], 0x04);
        assert_eq!(data[6], 0x05);
        assert_eq!(data[7], 0x06);
        assert_eq!(data[8], 0x07);
        assert_eq!(data[9], 0x08);
        assert_eq!(data[10], 0x09);
        assert_eq!(data[11], 0x10);
        assert_eq!(data[12], 0x12);
        assert_eq!(data[13], 0x34);
    }

    /// Test that serializing a TrackSectorList with two
    /// TrackSectorPair works
    #[test]
    fn serialize_track_sector_list_with_two_track_sector_pair_works() {
        let tsp1 = TrackSectorPair {
            track_number: 0x12,
            sector_number: 0x34,
        };
        let tsp2 = TrackSectorPair {
            track_number: 0x56,
            sector_number: 0x78,
        };
        let tsps = Vec::from([tsp1, tsp2]);

        let tsl = TrackSectorList {
            reserved: 0x01,
            track_number_of_next_sector: None,
            sector_number_of_next_sector: None,
            reserved_2: &[0x02, 0x03],
            sector_offset_in_file: &[0x04, 0x05],
            reserved_3: &[0x06, 0x07, 0x08, 0x09, 0x10],
            track_sector_pairs: tsps,
        };

        let data = tsl.as_vec().unwrap();

        assert_eq!(data.len(), 16);
        assert_eq!(data[0], 0x01);
        assert_eq!(data[1], 0x00);
        assert_eq!(data[2], 0x00);
        assert_eq!(data[3], 0x02);
        assert_eq!(data[4], 0x03);
        assert_eq!(data[5], 0x04);
        assert_eq!(data[6], 0x05);
        assert_eq!(data[7], 0x06);
        assert_eq!(data[8], 0x07);
        assert_eq!(data[9], 0x08);
        assert_eq!(data[10], 0x09);
        assert_eq!(data[11], 0x10);
        assert_eq!(data[12], 0x12);
        assert_eq!(data[13], 0x34);
        assert_eq!(data[14], 0x56);
        assert_eq!(data[15], 0x78);
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

    /// Test that parsing a single-sector catalog with the new test
    /// helpers works.
    /// This catalog just has one file
    /// Test that parsing a catalog that spans two sectors works.
    #[test]
    fn parse_single_sector_catalog_works() {
        let file_entries_1 = [FileEntry::new(
            0x12,
            0x0F,
            FileType::AppleSoftBasic,
            false,
            "A",
            0x0002,
        )];

        let mut catalog_by_filename_1: HashMap<String, FileEntry> = HashMap::new();
        file_entries_1.iter().for_each(|fe| {
            catalog_by_filename_1.insert(fe.filename().unwrap(), *fe);
        });

        let catalog_1 = Catalog {
            reserved: 0x00,
            track_number_of_next_sector: 0x00,
            sector_number_of_next_sector: 0x00,
            reserved_2: &[0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
            file_entries: file_entries_1.to_vec(),
            catalog_by_filename: catalog_by_filename_1,
        };

        let catalog_1_bytes = catalog_1.as_vec().unwrap();

        let mut tracks: Vec<Vec<&[u8]>> = Vec::new();

        let mut disk_data: [[[u8; 256]; 16]; 35] = [[[0; 256]; 16]; 35];

        for (i, byte) in catalog_1_bytes.iter().enumerate() {
            disk_data[17][2][i] = *byte;
        }

        for track in &disk_data {
            let mut track_vec: Vec<&[u8]> = Vec::new();
            for sector in track {
                track_vec.push(sector);
            }
            tracks.push(track_vec);
        }

        let catalog = parse_catalogs(&tracks, 17, 2).expect("Should be a valid FullCatalog");
        assert_eq!(catalog.file_entries.len(), 1);
        assert_eq!(
            catalog
                .file_entries
                .first()
                .expect("Should have at least one file")
                .filename()
                .expect("Should be a valid filename"),
            "A"
        );
    }

    /// Test that parsing a catalog that spans two sectors works.
    #[test]
    fn parse_multi_sector_catalog_works() {
        let file_entries_1 = [
            FileEntry::new(0x12, 0x0F, FileType::AppleSoftBasic, false, "A", 0x0002),
            FileEntry::new(0x13, 0x0F, FileType::AppleSoftBasic, false, "B", 0x0002),
            FileEntry::new(0x14, 0x0F, FileType::AppleSoftBasic, false, "C", 0x0002),
            FileEntry::new(0x15, 0x0F, FileType::AppleSoftBasic, false, "D", 0x0002),
            FileEntry::new(0x16, 0x0F, FileType::AppleSoftBasic, false, "E", 0x0002),
            FileEntry::new(0x17, 0x0F, FileType::AppleSoftBasic, false, "F", 0x0002),
            FileEntry::new(0x18, 0x0F, FileType::AppleSoftBasic, false, "G", 0x0002),
        ];
        let file_entries_2 = [
            FileEntry::new(0x19, 0x0F, FileType::AppleSoftBasic, false, "H", 0x0002),
            FileEntry::new(0x1A, 0x0F, FileType::AppleSoftBasic, false, "I", 0x0002),
            FileEntry::new(0x1B, 0x0F, FileType::AppleSoftBasic, false, "J", 0x0002),
        ];

        let mut catalog_by_filename_1: HashMap<String, FileEntry> = HashMap::new();
        file_entries_1.iter().for_each(|fe| {
            catalog_by_filename_1.insert(fe.filename().unwrap(), *fe);
        });

        let mut catalog_by_filename_2: HashMap<String, FileEntry> = HashMap::new();
        file_entries_2.iter().for_each(|fe| {
            catalog_by_filename_2.insert(fe.filename().unwrap(), *fe);
        });

        let catalog_1 = Catalog {
            reserved: 0x00,
            track_number_of_next_sector: 0x11,
            sector_number_of_next_sector: 0x01,
            reserved_2: &[0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
            file_entries: file_entries_1.to_vec(),
            catalog_by_filename: catalog_by_filename_1,
        };
        let catalog_2 = Catalog {
            reserved: 0x00,
            track_number_of_next_sector: 0x00,
            sector_number_of_next_sector: 0x00,
            reserved_2: &[0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
            file_entries: file_entries_2.to_vec(),
            catalog_by_filename: catalog_by_filename_2,
        };

        let catalog_1_bytes = catalog_1.as_vec().unwrap();
        let catalog_2_bytes = catalog_2.as_vec().unwrap();

        let mut tracks: Vec<Vec<&[u8]>> = Vec::new();

        let mut disk_data: [[[u8; 256]; 16]; 35] = [[[0; 256]; 16]; 35];

        for (i, byte) in catalog_1_bytes.iter().enumerate() {
            disk_data[17][2][i] = *byte;
        }
        for (i, byte) in catalog_2_bytes.iter().enumerate() {
            disk_data[17][1][i] = *byte;
        }

        for track in &disk_data {
            let mut track_vec: Vec<&[u8]> = Vec::new();
            for sector in track {
                track_vec.push(sector);
            }
            tracks.push(track_vec);
        }

        let catalog = parse_catalogs(&tracks, 17, 2).expect("Should be a valid FullCatalog");
        assert_eq!(catalog.file_entries.len(), 10);
        assert_eq!(
            catalog
                .file_entries
                .first()
                .expect("Should have at least one file")
                .filename()
                .expect("Should be a valid filename"),
            "A"
        );
    }

    /// Build a test binary file with the following content:
    /// Starts with the ASCII string START
    /// ends with the ASCII string END
    /// Filled with repeating data 0x00-0xFF, e.g.:
    /// build_test_file(10) ->
    /// 53 54 41 52 54 00 01 02 45 4e 44  |START...END|
    ///
    /// The length of the returned data is eight more bytes than the
    /// requested length, this is because it includes the memory
    /// address info and the file size in the header.
    fn build_binary_test_file(size: u16) -> Vec<u8> {
        let mut data: Vec<u8> = Vec::new();

        // The address in memory the file was located at
        data.extend([0x00, 0x10]);
        // The file size in bytes
        data.extend(little_endian_word_to_bytes(size));

        match size {
            0 => {}
            1 => {
                data.push(0x53);
            }
            2 => {
                data.extend([0x53, 0x44]);
            }
            3 => {
                data.extend([0x53, 0x54, 0x44]);
            }
            4 => {
                data.extend([0x53, 0x54, 0x4e, 0x44]);
            }
            5 => {
                data.extend([0x53, 0x54, 0x41, 0x4e, 0x44]);
            }
            6 => {
                data.extend([0x53, 0x54, 0x41, 0x45, 0x4e, 0x44]);
            }
            7 => {
                data.extend([0x53, 0x54, 0x41, 0x52, 0x45, 0x4e, 0x44]);
            }
            8 => {
                data.extend([0x53, 0x54, 0x41, 0x52, 0x54, 0x45, 0x4e, 0x44]);
            }
            _ => {
                data.extend([0x53, 0x54, 0x41, 0x52, 0x54]);
                for i in 0..size - 8 {
                    data.push((i % 0x100).try_into().unwrap());
                }
                data.extend([0x45, 0x4e, 0x44]);
            }
        }

        data
    }

    /// Test that building a file works
    /// Build a file that fits in less than a single sector
    /// This is a fairly complicated test function, it should be broken down into multiple
    /// functions.
    /// First, build a test file, then build a catalog.  Then insert the file into the
    /// image and build the disk image.
    #[test]
    fn build_single_sector_binary_file_works() {
        let file_entry = FileEntry::new(0x0A, 0x0D, FileType::Binary, false, "BLAH", 0x0001);
        let file_entries_1 = [file_entry];

        let mut catalog_by_filename_1: HashMap<String, FileEntry> = HashMap::new();
        file_entries_1.iter().for_each(|fe| {
            catalog_by_filename_1.insert(fe.filename().unwrap(), *fe);
        });

        let catalog_1 = Catalog {
            reserved: 0x00,
            track_number_of_next_sector: 0x00,
            sector_number_of_next_sector: 0x00,
            reserved_2: &[0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
            file_entries: file_entries_1.to_vec(),
            catalog_by_filename: catalog_by_filename_1,
        };

        let catalog_1_bytes = catalog_1.as_vec().unwrap();

        let mut tracks: Vec<Vec<&[u8]>> = Vec::new();

        let mut disk_data: [[[u8; 256]; 16]; 35] = [[[0; 256]; 16]; 35];

        for (i, byte) in catalog_1_bytes.iter().enumerate() {
            disk_data[17][2][i] = *byte;
        }

        // Using a sector size of 256
        let data = build_binary_test_file(200);

        for (i, byte) in data.iter().enumerate() {
            disk_data[0x11][0x0B][i] = *byte;
        }

        // Build the TrackSectorList for the file
        let tsp = TrackSectorPair {
            track_number: 0x11,
            sector_number: 0x0B,
        };
        let mut tsps: TrackSectorPairs = Vec::new();
        tsps.push(tsp);

        let tsl = TrackSectorList {
            reserved: 0,
            track_number_of_next_sector: None,
            sector_number_of_next_sector: None,
            reserved_2: &[0, 0],
            sector_offset_in_file: &[0, 0],
            reserved_3: &[0, 0, 0, 0, 0],
            track_sector_pairs: tsps,
        };

        for (i, byte) in tsl.as_vec().unwrap().iter().enumerate() {
            disk_data[0x0A][0x0D][i] = *byte;
        }

        for track in &disk_data {
            let mut track_vec: Vec<&[u8]> = Vec::new();
            for sector in track {
                track_vec.push(sector);
            }
            tracks.push(track_vec);
        }

        // TODO: Test parsing the catalog and retrieving the file
        let catalog = parse_catalogs(&tracks, 17, 2).expect("Should be a valid FullCatalog");
        assert_eq!(catalog.file_entries.len(), 1);
        assert_eq!(
            catalog
                .file_entries
                .first()
                .expect("Should have at least one file")
                .filename()
                .expect("Should be a valid filename"),
            "BLAH"
        );

        let files = build_files(catalog.clone(), &tracks).unwrap();
        assert!(files.contains_key("BLAH"));
        assert!(!files.contains_key("BLARGH"));

        let file = files.get("BLAH").unwrap();

        assert_eq!(file.data.len(), 200);
        assert_eq!(&file.data[0..5], "START".as_bytes());
        for i in 0..192 {
            assert_eq!(file.data[(i as usize) + 5_usize], i);
        }
        assert_eq!(&file.data[197..200], "END".as_bytes());
    }

    /// Test that building a file works
    /// Build a file that spans two sectors
    /// This is a fairly complicated test function, it should be broken down into multiple
    /// functions.
    /// First, build a test file, then build a catalog.  Then insert the file into the
    /// image and build the disk image.
    #[test]
    fn build_two_sector_binary_file_works() {
        let file_entry = FileEntry::new(0x0A, 0x0D, FileType::Binary, false, "BLAH", 0x0002);
        let file_entries_1 = [file_entry];

        let mut catalog_by_filename_1: HashMap<String, FileEntry> = HashMap::new();
        file_entries_1.iter().for_each(|fe| {
            catalog_by_filename_1.insert(fe.filename().unwrap(), *fe);
        });

        let catalog_1 = Catalog {
            reserved: 0x00,
            track_number_of_next_sector: 0x00,
            sector_number_of_next_sector: 0x00,
            reserved_2: &[0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
            file_entries: file_entries_1.to_vec(),
            catalog_by_filename: catalog_by_filename_1,
        };

        let catalog_1_bytes = catalog_1.as_vec().unwrap();

        let mut tracks: Vec<Vec<&[u8]>> = Vec::new();

        let mut disk_data: [[[u8; 256]; 16]; 35] = [[[0; 256]; 16]; 35];

        for (i, byte) in catalog_1_bytes.iter().enumerate() {
            disk_data[17][2][i] = *byte;
        }

        // Using a sector size of 256
        // Build a binary file of 400 bytes
        let data = build_binary_test_file(400);

        for (i, byte) in data[0..=255].iter().enumerate() {
            disk_data[0x11][0x0B][i] = *byte;
        }

        for (i, byte) in data[256..].iter().enumerate() {
            disk_data[0x11][0x0C][i] = *byte;
        }

        // Build the TrackSectorList for the file
        let tsp1 = TrackSectorPair {
            track_number: 0x11,
            sector_number: 0x0B,
        };
        let tsp2 = TrackSectorPair {
            track_number: 0x11,
            sector_number: 0x0C,
        };
        let mut tsps: TrackSectorPairs = Vec::new();
        tsps.push(tsp1);
        tsps.push(tsp2);

        let tsl = TrackSectorList {
            reserved: 0,
            track_number_of_next_sector: None,
            sector_number_of_next_sector: None,
            reserved_2: &[0, 0],
            sector_offset_in_file: &[0, 0],
            reserved_3: &[0, 0, 0, 0, 0],
            track_sector_pairs: tsps,
        };

        for (i, byte) in tsl.as_vec().unwrap().iter().enumerate() {
            disk_data[0x0A][0x0D][i] = *byte;
        }

        for track in &disk_data {
            let mut track_vec: Vec<&[u8]> = Vec::new();
            for sector in track {
                track_vec.push(sector);
            }
            tracks.push(track_vec);
        }

        // TODO: Test parsing the catalog and retrieving the file
        let catalog = parse_catalogs(&tracks, 17, 2).expect("Should be a valid FullCatalog");
        assert_eq!(catalog.file_entries.len(), 1);
        assert_eq!(
            catalog
                .file_entries
                .first()
                .expect("Should have at least one file")
                .filename()
                .expect("Should be a valid filename"),
            "BLAH"
        );

        let files = build_files(catalog.clone(), &tracks).unwrap();
        assert!(files.contains_key("BLAH"));
        assert!(!files.contains_key("BLARGH"));

        let file = files.get("BLAH").unwrap();

        assert_eq!(file.data.len(), 400);
        assert_eq!(&file.data[0..5], "START".as_bytes());
        let expected_data: [u8; 392] = (0_u16..392_u16)
            .map(|i| (i % 0x100) as u8)
            .collect::<Vec<u8>>()
            .try_into()
            .unwrap();

        assert_eq!(file.data[5..397], expected_data);
        assert_eq!(&file.data[397..400], "END".as_bytes());
    }
}
