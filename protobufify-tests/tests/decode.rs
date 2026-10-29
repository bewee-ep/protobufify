mod common;

use assert_matches::assert_matches;
use common::message;
use prost::encoding;
use protobufify::{ProtoError, decode, encode};
use protobufify_tests::{
    Book, Detail, Genre, Loan, LoanStatus, Location, Material, Rating, Recommendation, Review,
    Room, Shelf, Verdict,
};

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
    let bytes = encode(&shelf);

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
    let bytes = encode(&book);

    // when:
    let decoded = decode::<Book>(&bytes).unwrap();

    // then:
    assert_eq!(decoded.genre, Some(Genre::Poetry));
}

#[test]
fn decode_returns_enum_by_its_explicit_number() {
    // given:
    let mut bytes = Vec::new();
    encoding::int32::encode(2, &0x201, &mut bytes);
    encoding::int32::encode_packed(3, &[0x101, 0x201], &mut bytes);

    // when:
    let location = decode::<Location>(&bytes).unwrap();

    // then:
    assert_eq!(
        location,
        Location {
            title: String::new(),
            room: Room::Study,
            rooms: vec![Room::Library, Room::Study],
            material: None,
        }
    );
}

#[test]
fn decode_returns_enum_value_numbered_by_attribute_over_discriminant() {
    // given:
    let mut bytes = Vec::new();
    encoding::int32::encode(2, &0x301, &mut bytes);

    // when:
    let location = decode::<Location>(&bytes).unwrap();

    // then:
    assert_eq!(location.room, Room::Attic);
}

#[test]
fn decode_returns_enum_values_numbered_by_attribute_despite_discriminants() {
    // given:
    let material = 4;
    let wood = 8;
    let decoded = |value: i32| {
        let mut bytes = Vec::new();
        encoding::int32::encode(2, &0x101, &mut bytes);
        encoding::int32::encode(material, &value, &mut bytes);
        decode::<Location>(&bytes)
    };

    // when:
    let results = [decoded(wood), decoded(7), decoded(10)];

    // then:
    assert_matches!(
        results,
        [
            Ok(Location {
                material: Some(Material::Wood),
                ..
            }),
            Ok(Location {
                material: Some(Material::Metal),
                ..
            }),
            Err(ProtoError::UnknownEnumValue {
                ty: "Material",
                number: 10
            }),
        ]
    );
}

#[test]
fn decode_returns_fields_and_arms_by_their_explicit_numbers() {
    // given:
    let mut bytes = Vec::new();
    encoding::string::encode(10, &"Ada".to_string(), &mut bytes);
    let mut reminders = Vec::new();
    encoding::uint32::encode_packed(1, &[3], &mut reminders);
    message(20, &reminders, &mut bytes);
    let mut status = Vec::new();
    encoding::bool::encode(7, &true, &mut status);
    message(22, &status, &mut bytes);

    // when:
    let loan = decode::<Loan>(&bytes).unwrap();

    // then:
    assert_eq!(
        loan,
        Loan {
            borrower: "Ada".to_string(),
            book: None,
            reminders: Some(vec![3]),
            status: LoanStatus::Open,
        }
    );
}

#[test]
fn decode_returns_payload_field_by_its_explicit_number() {
    // given:
    let loan = Loan {
        borrower: String::new(),
        book: None,
        reminders: None,
        status: LoanStatus::Returned {
            day: 12,
            late: true,
        },
    };

    // when:
    let decoded = decode::<Loan>(&encode(&loan)).unwrap();

    // then:
    assert_eq!(decoded, loan);
}

#[test]
fn decode_returns_error_when_enum_value_is_unknown() {
    // given:
    let mut bytes = Vec::new();
    encoding::int32::encode(4, &999, &mut bytes);

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
    let bytes = encode(&book);

    // when:
    let decoded = decode::<Book>(&bytes).unwrap();

    // then:
    assert_eq!(decoded.pages, Some(635));
}

#[test]
fn decode_returns_error_when_integer_is_out_of_range() {
    // given:
    let mut bytes = Vec::new();
    encoding::uint32::encode(3, &70_000, &mut bytes);

    // when:
    let result = decode::<Book>(&bytes);

    // then:
    assert_matches!(result, Err(ProtoError::OutOfRange));
}

#[test]
fn decode_returns_none_when_optional_list_is_absent() {
    // given:
    let bytes = encode(&book("Untagged"));

    // when:
    let decoded = decode::<Book>(&bytes).unwrap();

    // then:
    assert_eq!(decoded.tags, None);
}

#[test]
fn decode_returns_empty_list_when_optional_list_is_empty() {
    // given:
    let mut bytes = Vec::new();
    message(5, &[], &mut bytes);

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
    let bytes = encode(&book);

    // when:
    let decoded = decode::<Book>(&bytes).unwrap();

    // then:
    assert_eq!(decoded.tags, Some(tags));
}

#[test]
fn decode_returns_repeated_enums_when_packed() {
    // given:
    let genres = [1, 3];
    let mut bytes = Vec::new();
    encoding::int32::encode_packed(3, &genres, &mut bytes);

    // when:
    let shelf = decode::<Shelf>(&bytes).unwrap();

    // then:
    assert_eq!(shelf.genres, vec![Genre::Novel, Genre::Reference]);
}

#[test]
fn decode_returns_repeated_enums_when_unpacked() {
    // given:
    let genres = [1, 3];
    let mut bytes = Vec::new();
    encoding::int32::encode_repeated(3, &genres, &mut bytes);

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
    bytes.extend(encode(&book("Moby-Dick")));

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
    encoding::uint32::encode(2, &4, &mut bytes);

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
    encoding::encode_key(2, encoding::WireType::LengthDelimited, &mut bytes);
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
    let bytes = encode(&recommendation);

    // when:
    let decoded = decode::<Recommendation>(&bytes).unwrap();

    // then:
    assert_eq!(decoded, recommendation);
}

#[test]
fn decode_returns_oneof_enum_arm() {
    // given:
    let recommendation = Recommendation::Genre(Genre::Novel);
    let bytes = encode(&recommendation);

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
    let bytes = encode(&recommendation);

    // when:
    let decoded = decode::<Recommendation>(&bytes).unwrap();

    // then:
    assert_eq!(decoded, recommendation);
}

#[test]
fn decode_returns_oneof_unit_arm() {
    // given:
    let bytes = encode(&Recommendation::Surprise);

    // when:
    let decoded = decode::<Recommendation>(&bytes).unwrap();

    // then:
    assert_eq!(decoded, Recommendation::Surprise);
}

#[test]
fn decode_returns_oneof_unit_arm_when_false() {
    // given:
    let mut bytes = Vec::new();
    encoding::bool::encode(4, &false, &mut bytes);

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
    let bytes = encode(&recommendation);

    // when:
    let decoded = decode::<Recommendation>(&bytes).unwrap();

    // then:
    assert_eq!(decoded, recommendation);
}

#[test]
fn decode_returns_defaults_when_struct_arm_fields_are_absent() {
    // given:
    let mut bytes = Vec::new();
    message(5, &[], &mut bytes);

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
fn decode_returns_value_encoded_with_proto_names_differing_from_rust_ones() {
    // given:
    let review = Review {
        author: "Ishmael".to_string(),
        r#type: Verdict::NotRecommended,
        excerpts: Some(vec!["Call me Ishmael.".to_string()]),
        detail: Some(Detail::Score { stars: 2 }),
    };

    // when:
    let decoded: Review = decode(&encode(&review)).unwrap();

    // then:
    assert_eq!(decoded, review);
}

#[test]
fn decode_returns_error_with_proto_names_when_required_field_is_absent() {
    // when:
    let result = decode::<Review>(&[]);

    // then:
    assert_matches!(
        result,
        Err(ProtoError::Missing {
            ty: "BookReview",
            field: "type"
        })
    );
}
