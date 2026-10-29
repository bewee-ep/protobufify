//! A small bookshelf model, showing how a crate uses protobufify:
//!
//! - the types derive `ProtoSchema`, `ProtoDecode` and `ProtoEncode`;
//! - every field, oneof arm and enum value is numbered with `#[protobufify(number = N)]`, and
//!   the numbers of removed members are kept `reserved`;
//! - [`definitions`] lists the exported types.

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
// Field 6 was the ISBN, since removed: neither its number nor its name may come back.
#[protobufify(reserved(6), reserved_names("isbn"))]
pub struct Book {
    #[protobufify(number = 1)]
    pub title: String,
    #[protobufify(number = 2)]
    pub author: Option<String>,
    /// Number of pages; proto has no 16-bit integers, so it travels as `uint32`.
    #[protobufify(number = 3)]
    pub pages: Option<u16>,
    #[protobufify(number = 4)]
    pub genre: Option<Genre>,
    /// `None` when unknown, empty when the book has no tags.
    #[protobufify(number = 5)]
    pub tags: Option<Vec<String>>,
}

/// What a book is about, numbered by the discriminants.
#[derive(Debug, PartialEq, ProtoSchema, ProtoDecode, ProtoEncode)]
#[protobufify(numbers_from_discriminants)]
#[repr(u8)]
pub enum Genre {
    Novel = 1,
    Poetry = 2,
    /// Dictionaries, atlases and the like.
    Reference = 3,
}

/// A labelled shelf of books.
#[derive(Debug, PartialEq, ProtoSchema, ProtoDecode, ProtoEncode)]
pub struct Shelf {
    #[protobufify(number = 1)]
    pub label: String,
    #[protobufify(number = 2)]
    pub books: Vec<Book>,
    #[protobufify(number = 3)]
    pub genres: Vec<Genre>,
}

/// What to read next.
#[derive(Debug, PartialEq, ProtoSchema, ProtoDecode, ProtoEncode)]
pub enum Recommendation {
    #[protobufify(number = 1)]
    Book(Book),
    /// A whole genre to explore.
    #[protobufify(number = 2)]
    Genre(Genre),
    /// Some shelves to browse.
    #[protobufify(number = 3)]
    Shelves(Vec<Shelf>),
    /// Anything: the reader is open to surprises.
    #[protobufify(number = 4)]
    Surprise,
    /// Books by an author, optionally of one genre only.
    #[protobufify(number = 5)]
    ByAuthor {
        #[protobufify(number = 1)]
        author: String,
        #[protobufify(number = 2)]
        genre: Option<Genre>,
    },
}
