mod common;

use common::message;
use prost::encoding;
use protobufify::encode;
use protobufify_tests::{
    Book, Genre, Loan, LoanStatus, Location, Material, Rating, Recommendation, Room, Shelf,
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
fn encode_writes_explicit_numbers_of_enum() {
    // given:
    let location = Location {
        title: String::new(),
        room: Room::Study,
        rooms: vec![Room::Library],
        material: None,
    };
    let mut expected = Vec::new();
    encoding::int32::encode(2, &0x201, &mut expected);
    encoding::int32::encode_packed(3, &[0x101], &mut expected);

    // when:
    let bytes = encode(&location);

    // then:
    assert_eq!(bytes, expected);
}

#[test]
fn encode_writes_enum_values_numbered_by_attribute_despite_discriminants() {
    // given:
    let location = Location {
        title: String::new(),
        room: Room::Attic,
        rooms: vec![],
        material: Some(Material::Wood),
    };
    let mut expected = Vec::new();
    encoding::int32::encode(2, &0x301, &mut expected);
    let wood = 8;
    encoding::int32::encode(4, &wood, &mut expected);

    // when:
    let bytes = encode(&location);

    // then:
    assert_eq!(bytes, expected);
}

#[test]
fn encode_writes_explicit_numbers_of_fields_and_arms() {
    // given:
    let loan = Loan {
        borrower: "Ada".to_string(),
        book: None,
        reminders: Some(vec![3]),
        status: LoanStatus::Returned {
            day: 12,
            late: true,
        },
    };
    let mut expected = Vec::new();
    encoding::string::encode(10, &"Ada".to_string(), &mut expected);
    let mut reminders = Vec::new();
    encoding::uint32::encode_packed(1, &[3], &mut reminders);
    message(20, &reminders, &mut expected);
    let mut returned = Vec::new();
    encoding::uint32::encode(3, &12, &mut returned);
    encoding::bool::encode(4, &true, &mut returned);
    let mut status = Vec::new();
    message(8, &returned, &mut status);
    message(22, &status, &mut expected);

    // when:
    let bytes = encode(&loan);

    // then:
    assert_eq!(bytes, expected);
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
    let bytes = encode(&shelf);

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
    let bytes = encode(&rating);

    // then:
    let mut expected = Vec::new();
    encoding::int32::encode(1, &1, &mut expected);
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
    let bytes = encode(&book);

    // then:
    let mut expected = Vec::new();
    encoding::uint32::encode(3, &0, &mut expected);
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
    let bytes = encode(&shelf);

    // then:
    let genres = [1, 3];
    let mut expected = Vec::new();
    encoding::int32::encode_packed(3, &genres, &mut expected);
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
    let bytes = encode(&book);

    // then:
    let mut expected = Vec::new();
    message(5, &[], &mut expected);
    assert_eq!(bytes, expected);
}

#[test]
fn encode_writes_unit_arm_as_true() {
    // when:
    let bytes = encode(&Recommendation::Surprise);

    // then:
    let mut expected = Vec::new();
    encoding::bool::encode(4, &true, &mut expected);
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
    let bytes = encode(&recommendation);

    // then:
    let mut payload = Vec::new();
    let author = 1;
    encoding::string::encode(author, &"Herman Melville".to_string(), &mut payload);
    let mut expected = Vec::new();
    message(5, &payload, &mut expected);
    assert_eq!(bytes, expected);
}
