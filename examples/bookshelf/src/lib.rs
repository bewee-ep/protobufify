//! A small bookshelf model, showing how a crate uses protobufify:
//!
//! - the types derive `ProtoSchema`, `ProtoDecode` and `ProtoEncode`;
//! - [`definitions`] lists the exported types;
//! - `proto.lock` in the crate root pins their numbers, kept up to date by
//!   `cargo run -p bookshelf --bin update-lock` (with `-- --frozen` in CI).

use protobufify::{ProtoDecode, ProtoDefs, ProtoEncode, ProtoSchema};

/// The definitions of the exported types; everything they reference is defined along with them.
pub fn definitions() -> ProtoDefs {
    let mut defs = ProtoDefs::default();
    Shelf::define(&mut defs);
    Recommendation::define(&mut defs);
    defs
}

/// A book on a shelf.
#[derive(Debug, PartialEq, ProtoSchema, ProtoDecode, ProtoEncode)]
pub struct Book {
    pub title: String,
    pub author: Option<String>,
    /// Number of pages; proto has no 16-bit integers, so it travels as `uint32`.
    pub pages: Option<u16>,
    pub genre: Option<Genre>,
    /// `None` when unknown, empty when the book has no tags.
    pub tags: Option<Vec<String>>,
}

/// What a book is about.
#[derive(Debug, PartialEq, ProtoSchema, ProtoDecode, ProtoEncode)]
pub enum Genre {
    Novel,
    Poetry,
    /// Dictionaries, atlases and the like.
    Reference,
}

/// A labelled shelf of books.
#[derive(Debug, PartialEq, ProtoSchema, ProtoDecode, ProtoEncode)]
pub struct Shelf {
    pub label: String,
    pub books: Vec<Book>,
    pub genres: Vec<Genre>,
}

/// What to read next.
#[derive(Debug, PartialEq, ProtoSchema, ProtoDecode, ProtoEncode)]
pub enum Recommendation {
    Book(Book),
    /// A whole genre to explore.
    Genre(Genre),
    /// Some shelves to browse.
    Shelves(Vec<Shelf>),
    /// Anything: the reader is open to surprises.
    Surprise,
    /// Books by an author, optionally of one genre only.
    ByAuthor {
        author: String,
        genre: Option<Genre>,
    },
}
