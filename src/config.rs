//! Configuration for the image-rider crate
#![warn(missing_docs)]
#![warn(unsafe_code)]

use crate::error;

#[cfg(feature = "commodore")]
use forbidden_bands::{self, Configuration as ForbiddenBandsConfiguration};

/// Configuration format
pub struct Config {
    /// Version of the configuration root
    pub version: String,

    /// The general settings
    pub settings: config::Config,

    /// A mapping for PETSCII systems
    /// TODO: Remove this, individual modules should create their own
    /// keys, in an approved namespace like good little modules.
    #[cfg(feature = "commodore")]
    pub forbidden_bands_config: forbidden_bands::Config,
}

/// Trait that defines a set of methods that allow loading and
/// unloading configuration data
pub trait Configuration {
    /// Load the configuration data from the default configuration
    /// string
    fn load(settings: config::Config) -> std::result::Result<Config, error::Error>;
}

impl Configuration for Config {
    fn load(settings: config::Config) -> std::result::Result<Config, error::Error> {
        #[cfg(feature = "commodore")]
        let forbidden_bands_config =
            forbidden_bands::Config::load().expect("Error loading forbidden bands config");

        let config = Config {
            version: String::from("0.1.0"),
            settings,
            #[cfg(feature = "commodore")]
            forbidden_bands_config,
        };

        Ok(config)
    }
}
