//! Renders the `.proto` schema of the bookshelf model.
//!
//! Run with `cargo run -p bookshelf --example schema`.

fn main() {
    print!(
        "syntax = \"proto3\";\n\npackage bookshelf;\n\n{}",
        bookshelf::definitions().render()
    );
}
