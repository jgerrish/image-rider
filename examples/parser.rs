#![warn(missing_docs)]
#![warn(unsafe_code)]
//! Parse an image file
//! Usage: cargo run --example parser --input FILENAME
//!
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::process::exit;

use clap::Parser;
use config::Config;
use env_logger;
use log::{error, info};

use image_rider::disk_format::image::{DiskImage, DiskImageParser, DiskImageSaver};

/// Command line arguments to parse an image file
#[derive(Parser, Debug)]
#[clap(about, version, author)]
struct Args {
    /// Filename to parse
    #[clap(short, long)]
    input: String,
    /// Filename to select for writing.
    /// Specifying a filename select that file to saving if output is
    /// also specified.
    #[clap(short, long)]
    filename: Option<String>,
    /// Filename to write track image data to,
    /// This writes the entire disk to a single file.
    #[clap(short, long)]
    output: Option<String>,
    /// Ignore any failed checksums on the disk data.
    #[clap(long)]
    ignore_checksums: bool,
}

/// Open up a file and read in the data
/// Returns all the data as a u8 vector
pub fn open_file(filename: &str) -> Vec<u8> {
    let path = Path::new(&filename);

    let mut file = match File::open(&path) {
        Err(why) => panic!("Couldn't open {}: {}", path.display(), why),
        Ok(file) => file,
    };

    let mut data = Vec::<u8>::new();

    let result = file.read_to_end(&mut data);

    match result {
        Err(why) => {
            error!("Error reading file: {}", why);
            panic!("Error reading file: {}", why);
        }
        Ok(result) => info!("Read {}: {} bytes", path.display(), result),
    };

    data
}

/// Parse an image file
fn main() {
    // Parse command line arguments
    let args = Args::parse();

    // Load config
    let mut _debug = true;

    // Initialize logger
    if let Err(e) = env_logger::try_init() {
        panic!("couldn't initialize logger: {:?}", e);
    }

    let settings_result = load_settings("config/image-rider.toml");
    let mut settings = match settings_result {
        Ok(settings) => {
            info!("merged in config");
            if let Ok(b) = settings.get_bool("debug") {
                _debug = b;
            }
            settings
        }
        Err(s) => {
            error!("error loading config: {:?}", s);
            Config::default()
        }
    };

    // See the comment in the load_settings function about a better solution to this
    if args.ignore_checksums == true {
        #[allow(deprecated)]
        settings
            .set("ignore-checksums", args.ignore_checksums)
            .unwrap();
    }

    let data = open_file(&args.input);

    let result = data.parse_disk_image(&settings, &args.input);

    let image = match result {
        Err(e) => {
            error!("{}", e);
            exit(1);
        }
        Ok(res) => {
            println!("Disk: {}", res);
            res
        }
    };

    let result = write_file(&settings, &args, &image);
    match result {
        Err(e) => {
            error!("{}", e);
            exit(1);
        }
        Ok(_) => {}
    }

    exit(0);
}

/// Save a file from the image to disk if the user specifies it.
fn write_file(
    settings: &Config,
    args: &Args,
    image: &DiskImage,
) -> std::result::Result<(), image_rider::error::Error> {
    // Find the type of disk image and write the track or sector data if its available
    if let Some(output_filename) = &args.output {
        info!("Got output filename, testing for image data");

        match &args.filename {
            Some(s) => {
                image.save_disk_image(&settings, Some(s.as_str()), &output_filename)?;
            }
            None => {
                image.save_disk_image(&settings, None, &output_filename)?;
            }
        };
        println!("Wrote file");
    }

    Ok(())
}

/// load settings from a config file
/// returns the config settings as a Config on success, or a ConfigError on failure
fn load_settings<'a>(config_name: &str) -> Result<Config, config::ConfigError> {
    let builder = Config::builder()
        // Add in config file
        .add_source(config::File::with_name(config_name))
        // Add in settings from the environment (with a prefix of APP)
        // Eg.. `APP_DEBUG=1 ./target/command_bar_widget would set the `debug` key
        .add_source(config::Environment::with_prefix("APP"))
        .build();

    builder
}
