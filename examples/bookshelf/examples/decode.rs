//! Encodes a shelf of the bookshelf model into protobuf and decodes it back. In practice the
//! bytes go to, or come from, any protobuf implementation using the schema of the `schema`
//! example.
//!
//! Run with `cargo run -p bookshelf --example decode`.

use bookshelf::{Book, Genre, Shelf};
use protobufify::{decode, encode};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let shelf = Shelf {
        label: "Fiction".to_string(),
        books: vec![Book {
            title: "Moby-Dick".to_string(),
            author: None,
            pages: Some(635),
            genre: Some(Genre::Novel),
            tags: None,
        }],
        genres: vec![Genre::Novel],
    };

    let bytes = encode(&shelf);
    println!("{} bytes: {bytes:02x?}", bytes.len());
    let decoded: Shelf = decode(&bytes)?;
    println!("{decoded:#?}");
    assert_eq!(decoded, shelf);
    Ok(())
}
