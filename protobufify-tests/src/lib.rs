//! The fixtures of protobufify's integration tests: a bookshelf model covering every supported
//! shape. `proto.lock` in the crate root pins their numbers; it is checked in and updated by
//! hand when the fixtures change (the tests fail while it lacks numbers they use).

use protobufify::{ProtoDecode, ProtoDefs, ProtoEncode, ProtoSchema};

/// The definitions of every fixture except [`Unexported`], as exported to `proto.lock`.
pub fn definitions() -> ProtoDefs {
    let mut defs = ProtoDefs::default();
    Shelf::define(&mut defs);
    Recommendation::define(&mut defs);
    Rating::define(&mut defs);
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

/// A required (non-`Option`) enum field has no proto3 default.
#[derive(Debug, PartialEq, ProtoSchema, ProtoDecode, ProtoEncode)]
pub struct Rating {
    pub genre: Genre,
    pub stars: u8,
}

/// Not exported: the lock file has no numbers for it.
#[derive(Debug, ProtoSchema, ProtoEncode)]
pub struct Unexported {
    pub note: String,
}
