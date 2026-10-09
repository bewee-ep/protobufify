mod common;

use assert_matches::assert_matches;
use common::{message, number};
use prost::encoding;
use protobufify::{EncodeError, encode};
use protobufify_tests::{Book, Genre, Rating, Recommendation, Shelf, Unexported};

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
fn encode_omits_plain_fields_when_default() {
    // given:
    let shelf = Shelf {
        label: String::new(),
        books: vec![],
        genres: vec![],
    };

    // when:
    let bytes = encode(&shelf).unwrap();

    // then:
    assert_eq!(bytes, Vec::<u8>::new());
}

#[test]
fn encode_writes_plain_enum_when_default_scalars_are_omitted() {
    // given:
    let rating = Rating {
        genre: Genre::Novel,
        stars: 0,
    };

    // when:
    let bytes = encode(&rating).unwrap();

    // then:
    let mut expected = Vec::new();
    encoding::int32::encode(
        number("Rating", "genre"),
        &genre("GENRE_NOVEL"),
        &mut expected,
    );
    assert_eq!(bytes, expected);
}

#[test]
fn encode_writes_optional_scalar_as_its_wire_type_when_zero() {
    // given:
    let book = Book {
        pages: Some(0),
        ..book("")
    };

    // when:
    let bytes = encode(&book).unwrap();

    // then:
    let mut expected = Vec::new();
    encoding::uint32::encode(number("Book", "pages"), &0, &mut expected);
    assert_eq!(bytes, expected);
}

#[test]
fn encode_writes_repeated_enums_packed() {
    // given:
    let shelf = Shelf {
        label: String::new(),
        books: vec![],
        genres: vec![Genre::Novel, Genre::Reference],
    };

    // when:
    let bytes = encode(&shelf).unwrap();

    // then:
    let genres = [genre("GENRE_NOVEL"), genre("GENRE_REFERENCE")];
    let mut expected = Vec::new();
    encoding::int32::encode_packed(number("Shelf", "genres"), &genres, &mut expected);
    assert_eq!(bytes, expected);
}

#[test]
fn encode_writes_empty_wrapper_when_optional_list_is_empty() {
    // given:
    let book = Book {
        tags: Some(vec![]),
        ..book("")
    };

    // when:
    let bytes = encode(&book).unwrap();

    // then:
    let mut expected = Vec::new();
    message("Book", "tags", &[], &mut expected);
    assert_eq!(bytes, expected);
}

#[test]
fn encode_writes_unit_arm_as_true() {
    // when:
    let bytes = encode(&Recommendation::Surprise).unwrap();

    // then:
    let mut expected = Vec::new();
    encoding::bool::encode(number("Recommendation", "surprise"), &true, &mut expected);
    assert_eq!(bytes, expected);
}

#[test]
fn encode_writes_struct_arm_as_payload_message() {
    // given:
    let recommendation = Recommendation::ByAuthor {
        author: "Herman Melville".to_string(),
        genre: None,
    };

    // when:
    let bytes = encode(&recommendation).unwrap();

    // then:
    let mut payload = Vec::new();
    let author = number("RecommendationByAuthor", "author");
    encoding::string::encode(author, &"Herman Melville".to_string(), &mut payload);
    let mut expected = Vec::new();
    message("Recommendation", "by_author", &payload, &mut expected);
    assert_eq!(bytes, expected);
}

#[test]
fn encode_returns_error_when_member_has_no_number() {
    // given:
    let unexported = Unexported {
        note: "draft".to_string(),
    };

    // when:
    let result = encode(&unexported);

    // then:
    assert_matches!(
        result,
        Err(EncodeError::Unnumbered {
            ty: "Unexported",
            member: "note"
        })
    );
}

#[test]
fn encode_returns_nothing_when_member_without_number_is_default() {
    // given:
    let unexported = Unexported {
        note: String::new(),
    };

    // when:
    let result = encode(&unexported);

    // then:
    assert_eq!(result.unwrap(), Vec::<u8>::new());
}
