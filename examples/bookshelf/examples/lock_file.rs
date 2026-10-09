//! How the lock file keeps field numbers stable while a type evolves: a field inserted in the
//! middle gets the next free number, a removed one stays reserved.
//!
//! Run with `cargo run -p bookshelf --example lock_file`.

use protobufify::{ProtoDefs, ProtoLock, ProtoSchema};

// Two versions of the same type; only their schemas are used here.
#[allow(dead_code)]
mod v1 {
    #[derive(protobufify::ProtoSchema)]
    pub struct Book {
        pub title: String,
        pub author: Option<String>,
        pub year: Option<u16>,
    }
}

#[allow(dead_code)]
mod v2 {
    /// `subtitle` is inserted in the middle, `year` is removed.
    #[derive(protobufify::ProtoSchema)]
    pub struct Book {
        pub title: String,
        pub subtitle: Option<String>,
        pub author: Option<String>,
    }
}

fn defs<T: ProtoSchema>() -> ProtoDefs {
    let mut defs = ProtoDefs::default();
    T::define(&mut defs);
    defs
}

fn main() {
    let mut lock = ProtoLock::default();

    for (version, defs) in [("v1", defs::<v1::Book>()), ("v2", defs::<v2::Book>())] {
        println!("== {version}: lock file changes");
        for change in lock.update(&defs) {
            println!("  {change}");
        }
        print!("== {version}: schema\n{}", defs.render(&lock));
    }

    println!("== lock file\n{lock}");
}
