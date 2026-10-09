mod common;

use assert_matches::assert_matches;
use common::{message, number};
use prost::encoding;
use protobufify::{ProtoDecode, ProtoError, decode, encode};
use protobufify_tests::{Book, Genre, Rating, Recommendation, Shelf};

fn genre(value: &str) -> i32 {
    number("Genre", value) as i32
}

fn book(title: &str) -> Book {
    Book {
        title: title.to_string(),
        author: None,
        pages: None,
        genre: None,
        tags: None,
    }
}

#[test]
fn decode_returns_scalar_and_nested_fields() {
    // given:
    let shelf = Shelf {
        label: "Fiction".to_string(),
        books: vec![book("Moby-Dick")],
        genres: vec![],
    };
    let bytes = encode(&shelf).unwrap();

    // when:
    let decoded = decode::<Shelf>(&bytes).unwrap();

    // then:
    assert_eq!(decoded, shelf);
}

#[test]
fn decode_returns_enum_field() {
    // given:
    let book = Book {
        genre: Some(Genre::Poetry),
        ..book("Leaves of Grass")
    };
    let bytes = encode(&book).unwrap();

    // when:
    let decoded = decode::<Book>(&bytes).unwrap();

    // then:
    assert_eq!(decoded.genre, Some(Genre::Poetry));
}

#[test]
fn decode_returns_error_when_enum_value_is_unknown() {
    // given:
    let mut bytes = Vec::new();
    encoding::int32::encode(number("Book", "genre"), &999, &mut bytes);

    // when:
    let result = decode::<Book>(&bytes);

    // then:
    assert_matches!(
        result,
        Err(ProtoError::UnknownEnumValue {
            ty: "Genre",
            number: 999
        })
    );
}

#[test]
fn decode_returns_narrowed_integer() {
    // given:
    let book = Book {
        pages: Some(635),
        ..book("Moby-Dick")
    };
    let bytes = encode(&book).unwrap();

    // when:
    let decoded = decode::<Book>(&bytes).unwrap();

    // then:
    assert_eq!(decoded.pages, Some(635));
}

#[test]
fn decode_returns_error_when_integer_is_out_of_range() {
    // given:
    let mut bytes = Vec::new();
    encoding::uint32::encode(number("Book", "pages"), &70_000, &mut bytes);

    // when:
    let result = decode::<Book>(&bytes);

    // then:
    assert_matches!(result, Err(ProtoError::OutOfRange));
}

#[test]
fn decode_returns_none_when_optional_list_is_absent() {
    // given:
    let bytes = encode(&book("Untagged")).unwrap();

    // when:
    let decoded = decode::<Book>(&bytes).unwrap();

    // then:
    assert_eq!(decoded.tags, None);
}

#[test]
fn decode_returns_empty_list_when_optional_list_is_empty() {
    // given:
    let mut bytes = Vec::new();
    message("Book", "tags", &[], &mut bytes);

    // when:
    let book = decode::<Book>(&bytes).unwrap();

    // then:
    assert_eq!(book.tags, Some(vec![]));
}

#[test]
fn decode_returns_optional_list_items() {
    // given:
    let tags = vec!["sea".to_string(), "whale".to_string()];
    let book = Book {
        tags: Some(tags.clone()),
        ..book("Moby-Dick")
    };
    let bytes = encode(&book).unwrap();

    // when:
    let decoded = decode::<Book>(&bytes).unwrap();

    // then:
    assert_eq!(decoded.tags, Some(tags));
}

#[test]
fn decode_returns_repeated_enums_when_packed() {
    // given:
    let genres = [genre("GENRE_NOVEL"), genre("GENRE_REFERENCE")];
    let mut bytes = Vec::new();
    encoding::int32::encode_packed(number("Shelf", "genres"), &genres, &mut bytes);

    // when:
    let shelf = decode::<Shelf>(&bytes).unwrap();

    // then:
    assert_eq!(shelf.genres, vec![Genre::Novel, Genre::Reference]);
}

#[test]
fn decode_returns_repeated_enums_when_unpacked() {
    // given:
    let genres = [genre("GENRE_NOVEL"), genre("GENRE_REFERENCE")];
    let mut bytes = Vec::new();
    encoding::int32::encode_repeated(number("Shelf", "genres"), &genres, &mut bytes);

    // when:
    let shelf = decode::<Shelf>(&bytes).unwrap();

    // then:
    assert_eq!(shelf.genres, vec![Genre::Novel, Genre::Reference]);
}

#[test]
fn decode_returns_known_fields_when_unknown_field_is_present() {
    // given:
    let mut bytes = Vec::new();
    encoding::uint64::encode(9_999, &42, &mut bytes);
    bytes.extend(encode(&book("Moby-Dick")).unwrap());

    // when:
    let book = decode::<Book>(&bytes).unwrap();

    // then:
    assert_eq!(book.title, "Moby-Dick");
}

#[test]
fn decode_returns_defaults_when_required_scalars_are_absent() {
    // given:
    let bytes = [];

    // when:
    let shelf = decode::<Shelf>(&bytes).unwrap();

    // then:
    assert_eq!(shelf.label, "");
    assert_eq!(shelf.books.len(), 0);
}

#[test]
fn decode_returns_error_when_required_enum_is_absent() {
    // given:
    let mut bytes = Vec::new();
    encoding::uint32::encode(number("Rating", "stars"), &4, &mut bytes);

    // when:
    let result = decode::<Rating>(&bytes);

    // then:
    assert_matches!(
        result,
        Err(ProtoError::Missing {
            ty: "Rating",
            field: "genre"
        })
    );
}

#[test]
fn decode_returns_error_when_input_is_truncated() {
    // given:
    let mut bytes = Vec::new();
    encoding::encode_key(
        number("Shelf", "books"),
        encoding::WireType::LengthDelimited,
        &mut bytes,
    );
    encoding::encode_varint(10, &mut bytes);

    // when:
    let result = decode::<Shelf>(&bytes);

    // then:
    assert_matches!(result, Err(ProtoError::Truncated));
}

#[test]
fn decode_returns_oneof_message_arm() {
    // given:
    let recommendation = Recommendation::Book(book("Moby-Dick"));
    let bytes = encode(&recommendation).unwrap();

    // when:
    let decoded = decode::<Recommendation>(&bytes).unwrap();

    // then:
    assert_eq!(decoded, recommendation);
}

#[test]
fn decode_returns_oneof_enum_arm() {
    // given:
    let recommendation = Recommendation::Genre(Genre::Novel);
    let bytes = encode(&recommendation).unwrap();

    // when:
    let decoded = decode::<Recommendation>(&bytes).unwrap();

    // then:
    assert_eq!(decoded, recommendation);
}

#[test]
fn decode_returns_oneof_list_arm() {
    // given:
    let recommendation = Recommendation::Shelves(vec![Shelf {
        label: "Poetry".to_string(),
        books: vec![],
        genres: vec![Genre::Poetry],
    }]);
    let bytes = encode(&recommendation).unwrap();

    // when:
    let decoded = decode::<Recommendation>(&bytes).unwrap();

    // then:
    assert_eq!(decoded, recommendation);
}

#[test]
fn decode_returns_oneof_unit_arm() {
    // given:
    let bytes = encode(&Recommendation::Surprise).unwrap();

    // when:
    let decoded = decode::<Recommendation>(&bytes).unwrap();

    // then:
    assert_eq!(decoded, Recommendation::Surprise);
}

#[test]
fn decode_returns_oneof_unit_arm_when_false() {
    // given:
    let mut bytes = Vec::new();
    encoding::bool::encode(number("Recommendation", "surprise"), &false, &mut bytes);

    // when:
    let recommendation = decode::<Recommendation>(&bytes).unwrap();

    // then:
    assert_matches!(recommendation, Recommendation::Surprise);
}

#[test]
fn decode_returns_oneof_struct_arm() {
    // given:
    let recommendation = Recommendation::ByAuthor {
        author: "Herman Melville".to_string(),
        genre: Some(Genre::Novel),
    };
    let bytes = encode(&recommendation).unwrap();

    // when:
    let decoded = decode::<Recommendation>(&bytes).unwrap();

    // then:
    assert_eq!(decoded, recommendation);
}

#[test]
fn decode_returns_defaults_when_struct_arm_fields_are_absent() {
    // given:
    let mut bytes = Vec::new();
    message("Recommendation", "by_author", &[], &mut bytes);

    // when:
    let recommendation = decode::<Recommendation>(&bytes).unwrap();

    // then:
    assert_matches!(
        recommendation,
        Recommendation::ByAuthor { author, genre: None } if author.is_empty()
    );
}

#[test]
fn decode_returns_error_when_oneof_is_empty() {
    // given:
    let bytes = [];

    // when:
    let result = decode::<Recommendation>(&bytes);

    // then:
    assert_matches!(
        result,
        Err(ProtoError::Missing {
            ty: "Recommendation",
            field: "value"
        })
    );
}

#[test]
fn locked_returns_true_for_every_decodable_fixture() {
    // when:
    let locked = [
        <Book as ProtoDecode>::LOCKED,
        <Genre as ProtoDecode>::LOCKED,
        <Shelf as ProtoDecode>::LOCKED,
        <Recommendation as ProtoDecode>::LOCKED,
        <Rating as ProtoDecode>::LOCKED,
    ];

    // then:
    assert_eq!(locked, [true; 5]);
}
