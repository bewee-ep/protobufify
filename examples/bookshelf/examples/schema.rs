//! Renders the `.proto` schema of the bookshelf model, numbered by the checked-in `proto.lock`
//! (which `cargo run -p bookshelf --bin update-lock` keeps up to date).
//!
//! Run with `cargo run -p bookshelf --example schema`.

use protobufify::ProtoLock;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // The committed numbers, the same the derived decoders and encoders are compiled with.
    let lock: ProtoLock = include_str!("../proto.lock").parse()?;

    print!(
        "syntax = \"proto3\";\n\npackage bookshelf;\n\n{}",
        bookshelf::definitions().render(&lock)
    );
    Ok(())
}
