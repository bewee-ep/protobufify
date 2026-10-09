//! Brings `proto.lock` up to date with the exported types and prints what changed.
//! `--frozen` (for CI) only checks: it fails if the lock file is out of date.
//!
//! Run with `cargo run -p bookshelf --bin update-lock [-- --frozen]`.

use std::process::ExitCode;

fn main() -> ExitCode {
    let frozen = std::env::args().skip(1).any(|arg| arg == "--frozen");
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/proto.lock");
    match protobufify::update_lock_file(path, &bookshelf::definitions(), frozen) {
        Ok((_, changes)) => {
            for change in changes {
                println!("{change}");
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("{path}: {e}");
            ExitCode::FAILURE
        }
    }
}
