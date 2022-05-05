# image-rider disk image parser
![build](https://github.com/jgerrish/image-rider/actions/workflows/rust.yml/badge.svg)

This is a library of parsers built using the nom parsing framework to parse disk images
and ROMs.

# Supported Formats

The following formats are currently detected.  Parsing is not fully
implemented for any of them yet.

D64: A Commodore 64 D64 Disk Image
DSK: Apple ][ DOS Disk Image
NIB: Apple ][ Nibble encoded Disk Image
STX: An Atari ST STX Disk Image

# Usage

You can run the example application with the following command:

RUST_LOG=debug cargo run --example parser -- --input FILENAME

To save track or sector image data (for example, the FAT filesystem
embedded in a STX image):

RUST_LOG=debug cargo run --example parser -- --input INFILENAME --output OUTFILENAME


There are several sanity checks in the code to panic or exit the
parsing process if an image format is found that isn't known about or
currently supported.  In addition, checksums failures usually cause
parsing failures.

To disable checksum checks, pass the --ignore-checksums command line
flag to the parser example:

RUST_LOG=debug cargo run --example parser -- --ignore-checksums --input FILENAME

# Development

The usual Rust build process and commands are used to build and test this program:

$ cargo build
$ cargo test

## Creating Your Own Format Parser

You can create your own ROM or disk image parser.

Building a parser loading system that uses dynamic loading roadmap.
In the meantime, here are the steps to add a new parser called foo:

  * Make a new directory (module) in src/disk_format/foo
  * Include that module in src/disk_format/mod.rs with a "pub mod foo;" line
  * Add a mod.rs file in the new directory and include any
    module-level code in there.  The entire plugin can live in there
    if you want.
  * The plugin should have a top-level structure called FooDisk or similar.
  * The plugin should have an implementation of Display for FooDisk
  * The plugin should have an implmentation of DiskImageParser for FooDisk
  * Import FooDisk and add the FooDisk structure to DiskImage in
    src/disk_format/image.rs and supporting functions:
	  * pub enum DiskImage
	  * impl Display for DiskImage
  * Add the parser to the disk_image_parser function in
    src/disk_format/image.rs file as another alt:
	map(foo_disk_parser, DiskImage::Foo)
