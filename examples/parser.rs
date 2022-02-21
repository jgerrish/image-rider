/// Parse an image file
/// Usage: cargo run --example parser --input FILENAME
///
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::process::exit;

use clap::Parser;
use config::Config;
use env_logger;
use log::{error, info};

use image_rider::disk_format::image::disk_image_parser;

/// Command line arguments to parse an image file
#[derive(Parser, Debug)]
#[clap(about, version, author)]
struct Args {
    /// Filename to parse
    #[clap(short, long)]
    input: String,
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
    // Load config
    let mut _debug = true;

    // Initialize logger
    if let Err(e) = env_logger::try_init() {
        panic!("couldn't initialize logger: {:?}", e);
    }

    let settings_result = load_settings("config/image-rider.toml");
    match settings_result {
        Ok(settings) => {
            info!("merged in config");
            if let Ok(b) = settings.get_bool("debug") {
                _debug = b;
            }
        }
        Err(s) => {
            error!("error loading config: {:?}", s)
        }
    };

    // Parse command line arguments
    let args = Args::parse();

    let data = open_file(&args.input);

    // Parse the image data
    let result = disk_image_parser(&data);

    let (_, _image) = match result {
        Err(e) => {
            error!("{}", e);
            exit(1);
        }
        Ok(res) => {
            println!("Disk: {}", res.1);
            res
        }
    };

    exit(0);
}

/// load settings from a config file
/// returns the config settings as a Config on success, or a ConfigError on failure
fn load_settings<'a>(config_name: &str) -> Result<Config, config::ConfigError> {
    Config::builder()
        // Add in config file
        .add_source(config::File::with_name(config_name))
        // Add in settings from the environment (with a prefix of APP)
        // Eg.. `APP_DEBUG=1 ./target/command_bar_widget would set the `debug` key
        .add_source(config::Environment::with_prefix("APP"))
        .build()
}
