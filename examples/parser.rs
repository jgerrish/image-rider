#![warn(missing_docs)]
#![warn(unsafe_code)]
//! Parse an image file
//! Usage: cargo run --example parser --input FILENAME
//!
use std::process::exit;

use clap::Parser;
use config::Config;
use log::{error, info};

use image_rider::{
    config::Configuration,
    disk_format::image::{DiskImage, DiskImageOps, DiskImageParser, DiskImageSaver},
    file::read_file,
};

/// Command line arguments to parse an image file
#[derive(Parser, Debug)]
#[clap(about, version, author)]
struct Args {
    /// Filename to parse
    #[clap(short, long)]
    input: String,
    /// List the disk contents
    #[clap(short, long)]
    catalog: bool,
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
    if args.ignore_checksums {
        #[allow(deprecated)]
        settings
            .set("ignore-checksums", args.ignore_checksums)
            .unwrap();
    }

    let config =
        image_rider::config::Config::load(settings).expect("Error loading image rider config");

    let res = read_file(&args.input);
    let data = match res {
        Err(e) => {
            error!("Error opening file: {}", e);
            panic!("Error opening file: {}", e);
        }
        Ok(data) => data,
    };

    let result = data.parse_disk_image(&config, &args.input);

    let image = match result {
        Err(e) => {
            error!("{}", e);
            exit(1);
        }
        Ok(res) => {
            // List the disk contents or catalog if requested
            if args.catalog {
                let catalog_res = res.catalog(&config);
                match catalog_res {
                    Err(e) => {
                        error!("{}", e);
                        exit(1);
                    }
                    Ok(res) => {
                        println!("Disk catalog: {}", res);
                        println!("{}", res);
                    }
                }
            }

            println!("Disk: {}", res);
            res
        }
    };

    let result = write_file(&config, &args, &image);
    if let Err(e) = result {
        error!("{}", e);
        exit(1);
    }

    exit(0);
}

/// Save a file from the image to disk if the user specifies it.
fn write_file(
    config: &image_rider::config::Config,
    args: &Args,
    image: &DiskImage,
) -> std::result::Result<(), image_rider::error::Error> {
    // Find the type of disk image and write the track or sector data if its available
    if let Some(output_filename) = &args.output {
        info!("Got output filename, testing for image data");

        match &args.filename {
            Some(s) => {
                image.save_disk_image(config, Some(s.as_str()), output_filename)?;
            }
            None => {
                image.save_disk_image(config, None, output_filename)?;
            }
        };
        println!("Wrote file");
    }

    Ok(())
}

/// load settings from a config file
/// returns the config settings as a Config on success, or a ConfigError on failure
fn load_settings(config_name: &str) -> Result<Config, config::ConfigError> {
    Config::builder()
        // Add in config file
        .add_source(config::File::with_name(config_name))
        // Add in settings from the environment (with a prefix of APP)
        // Eg.. `APP_DEBUG=1 ./target/command_bar_widget would set the `debug` key
        .add_source(config::Environment::with_prefix("APP"))
        .build()
}
