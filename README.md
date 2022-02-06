# image-rider disk image parser
![build](https://github.com/jgerrish/image-rider/actions/workflows/rust.yml/badge.svg)

This is a library of parsers built using the nom parsing framework to parse disk images
and ROMs.

# Supported Formats

The following formats are currently detected.  Parsing is not fully
implemented for any of them yet.

D64: A Commodore 64 D64 Disk Image
STX: An Atari ST STX Disk Image

# Usage

You can run the example application with the following command:

RUST_LOG=debug cargo run --example parser -- --input FILENAME

