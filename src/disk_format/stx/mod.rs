/// Parse an Atari ST STX (Pasti) disk image
/// The basic structure of a Pasti image is:
///
/// File Header
/// Track Header
///   Sector Header
///   ...
///   Sector Header
///   Fuzzy sector mask (if it exists)
///   Track data, may be track image itself, or image containing sector data, may be missing
///   Sector data x sector count, if not in track image
/// ...
/// Track Header
///   Sector Header
///    ...
/// etc.
///
/// Information from:
/// https://atari.8bitchip.info/STXdesc.html
///   (Good summary)
/// Hatari https://github.com/hatari/hatari.git
/// Thomas Bernard https://github.com/miniupnp/AtariST.git
///   (short Python script that gives a simple overview of reading in the metadata)
/// CLK https://github.com/TomHarte/CLK.git
///   (modern C++ code)
/// pce https://github.com/jsdf/pce.git
///   (easy to understand code)
#[warn(missing_docs)]
#[warn(unsafe_code)]

/// STX disk image module
pub mod disk;

/// STX track module
pub mod track;

/// STX sector module
pub mod sector;

use crate::disk_format::sanity_check::SanityCheck;

const CCITT_CRC16_POLY: u16 = 0x1021;

/// Add a byte to the CRC
/// TODO: Double check this
pub fn crc16_add_byte(crc: u16, byte: u8) -> u16 {
    let mut new_crc = crc;

    // exclusive or the shifted byte and the current CRC
    new_crc ^= ((byte as u16) << 8) as u16;

    // Rust for-loop iteration is not inclusive on the end
    for _i in 0..8 {
        if (new_crc & 0x8000u16) != 0 {
            // exclusive or the shifted CRC and the CRC16 polynomial
            // assuming we are using the CCITT CRC16 polynomial: 0b0001000000100001
            new_crc = (new_crc << 1) ^ CCITT_CRC16_POLY;
        } else {
            new_crc <<= 1;
        }
    }

    new_crc
}

#[cfg(test)]
mod tests {
    use super::crc16_add_byte;
    use super::CCITT_CRC16_POLY;

    /// Test calculating a CRC16
    #[test]
    fn crc16_add_one_works() {
        let test_byte = 0x01_u8;
        let crc = 0xFFFF_u16;

        // verify the CRC16 polynomial is a known value
        assert_eq!(CCITT_CRC16_POLY, 0x1021);

        let crc = crc16_add_byte(crc, test_byte);

        // Each shift:
        //   0xFFFF (start) -> 0xFEFF (XOR shited byte) ->
        //   0xEDDF -> 0xCB9F -> 0x871F -> 0x1E1F -> 0x3C3E -> 0x787C -> 0xF0F8 -> 0xF1D1
        //   (shift loop)
        assert_eq!(crc, 0xF1D1);
    }
}
