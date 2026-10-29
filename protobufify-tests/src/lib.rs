//! The fixtures of protobufify's integration tests: a bookshelf model covering every supported
//! shape.

use protobufify::{ProtoDecode, ProtoDefs, ProtoEncode, ProtoSchema};

/// The definitions of every fixture.
pub fn definitions() -> ProtoDefs {
    let mut defs = ProtoDefs::default();
    Shelf::define(&mut defs);
    Recommendation::define(&mut defs);
    Rating::define(&mut defs);
    Location::define(&mut defs);
    Loan::define(&mut defs);
    Review::define(&mut defs);
    defs
}

/// A book on a shelf.
#[derive(Debug, PartialEq, ProtoSchema, ProtoDecode, ProtoEncode)]
#[protobufify(reserved(6, "isbn"))]
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

/// What a book is about.
#[derive(Debug, PartialEq, ProtoSchema, ProtoDecode, ProtoEncode)]
#[protobufify(reserved(4, "GENRE_COMICS"))]
pub enum Genre {
    #[protobufify(number = 1)]
    Novel,
    #[protobufify(number = 2)]
    Poetry,
    /// Dictionaries, atlases and the like.
    #[protobufify(number = 3)]
    Reference,
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

/// A required (non-`Option`) enum field has no proto3 default.
#[derive(Debug, PartialEq, ProtoSchema, ProtoDecode, ProtoEncode)]
pub struct Rating {
    #[protobufify(number = 1)]
    pub genre: Genre,
    #[protobufify(number = 2)]
    pub stars: u8,
}

/// A room of the house, numbered `0xFRR` (floor, room) by its discriminants or the attribute.
#[derive(Debug, Clone, Copy, PartialEq, ProtoSchema, ProtoDecode, ProtoEncode)]
#[protobufify(numbers_from_discriminants)]
#[repr(u32)]
pub enum Room {
    Library = 0x101,
    /// The study upstairs.
    Study = 0x201,
    /// The attribute takes precedence over the (implicit) discriminant.
    #[protobufify(number = 0x301)]
    Attic,
}

/// What a shelf is made of. Without `numbers_from_discriminants`, the discriminants do not
/// number it: the attribute does.
#[derive(Debug, Clone, Copy, PartialEq, ProtoSchema, ProtoDecode, ProtoEncode)]
#[repr(u8)]
pub enum Material {
    #[protobufify(number = 8)]
    Wood = 10,
    #[protobufify(number = 7)]
    Metal = 20,
}

/// Where a book is kept.
#[derive(Debug, PartialEq, ProtoSchema, ProtoDecode, ProtoEncode)]
pub struct Location {
    #[protobufify(number = 1)]
    pub title: String,
    #[protobufify(number = 2)]
    pub room: Room,
    #[protobufify(number = 3)]
    pub rooms: Vec<Room>,
    #[protobufify(number = 4)]
    pub material: Option<Material>,
}

/// A book lent out, its fields numbered out of declaration order.
#[derive(Debug, PartialEq, ProtoSchema, ProtoDecode, ProtoEncode)]
pub struct Loan {
    #[protobufify(number = 10)]
    pub borrower: String,
    #[protobufify(number = 21)]
    pub book: Option<Book>,
    /// Days on which the borrower was reminded.
    #[protobufify(number = 20)]
    pub reminders: Option<Vec<u32>>,
    #[protobufify(number = 22)]
    pub status: LoanStatus,
}

/// Where a loan stands.
#[derive(Debug, PartialEq, ProtoSchema, ProtoDecode, ProtoEncode)]
pub enum LoanStatus {
    #[protobufify(number = 7)]
    Open,
    /// The payload message `LoanStatusReturned` reserves the numbers of removed fields.
    #[protobufify(number = 8, reserved(1, 2, "date"))]
    Returned {
        #[protobufify(number = 3)]
        day: u32,
        #[protobufify(number = 4)]
        late: bool,
    },
    #[protobufify(number = 9)]
    Overdue(u32),
}

/// A review of a book, its proto names differing from the Rust ones.
#[derive(Debug, PartialEq, ProtoSchema, ProtoDecode, ProtoEncode)]
#[protobufify(name = "BookReview")]
pub struct Review {
    #[protobufify(number = 1, name = "reviewer")]
    pub author: String,
    /// A keyword in Rust, not in proto.
    #[protobufify(number = 2)]
    pub r#type: Verdict,
    /// Its list wrapper follows the proto names: `BookReviewQuotesList`.
    #[protobufify(number = 3, name = "quotes")]
    pub excerpts: Option<Vec<String>>,
    #[protobufify(number = 4)]
    pub detail: Option<Detail>,
}

/// Its values are prefixed with the proto name: `REVIEW_VERDICT_…`.
#[derive(Debug, PartialEq, ProtoSchema, ProtoDecode, ProtoEncode)]
#[protobufify(name = "ReviewVerdict")]
pub enum Verdict {
    #[protobufify(number = 1)]
    Recommended,
    #[protobufify(number = 2, name = "REVIEW_VERDICT_SKIP")]
    NotRecommended,
}

/// Its generated messages follow the proto names of the arms.
#[derive(Debug, PartialEq, ProtoSchema, ProtoDecode, ProtoEncode)]
pub enum Detail {
    /// The payload message `DetailRating`.
    #[protobufify(number = 1, name = "rating")]
    Score {
        #[protobufify(number = 1)]
        stars: u8,
    },
    /// The list wrapper `DetailPagesList`.
    #[protobufify(number = 2, name = "pages")]
    PageList(Vec<u32>),
}
