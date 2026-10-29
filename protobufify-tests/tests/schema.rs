use assert_matches::assert_matches;
use protobufify::{Arm, EnumValue, Field, Kind, Label, ProtoDefs, ProtoSchema, ProtoType};
use protobufify_tests::{Book, Genre, Loan, Material, Recommendation, Room, Shelf};

fn defined<T: ProtoSchema>() -> ProtoDefs {
    let mut defs = ProtoDefs::default();
    T::define(&mut defs);
    defs
}

fn field(name: &'static str, label: Label, ty: ProtoType, number: u32) -> Field {
    Field {
        name,
        label,
        ty,
        doc: "",
        number,
    }
}

fn value(name: &'static str, doc: &'static str, number: u32) -> EnumValue {
    EnumValue { name, doc, number }
}

#[test]
fn define_returns_message_with_fields_in_declaration_order() {
    // when:
    let defs = defined::<Book>();

    // then:
    let book = defs.get("Book").unwrap();
    assert_eq!(book.doc, "A book on a shelf.");
    assert_eq!(
        book.kind,
        Kind::Message(vec![
            field("title", Label::Plain, ProtoType::Scalar("string"), 1),
            field("author", Label::Optional, ProtoType::Scalar("string"), 2),
            Field {
                doc: "Number of pages; proto has no 16-bit integers, so it travels as `uint32`.",
                ..field("pages", Label::Optional, ProtoType::Scalar("uint32"), 3)
            },
            field("genre", Label::Optional, ProtoType::Named("Genre"), 4),
            Field {
                doc: "`None` when unknown, empty when the book has no tags.",
                ..field("tags", Label::Optional, ProtoType::Named("BookTagsList"), 5)
            },
        ])
    );
}

#[test]
fn define_returns_reserved_numbers_and_names() {
    // when:
    let defs = defined::<Book>();

    // then:
    let book = defs.get("Book").unwrap();
    assert_eq!(book.reserved, [6]);
    assert_eq!(book.reserved_names, ["isbn"]);
}

#[test]
fn define_returns_reserved_numbers_and_names_of_payload_message() {
    // when:
    let defs = defined::<Loan>();

    // then:
    let returned = defs.get("LoanStatusReturned").unwrap();
    assert_eq!(returned.reserved, [1, 2]);
    assert_eq!(returned.reserved_names, ["date"]);
    assert_eq!(defs.get("LoanStatus").unwrap().reserved, []);
}

#[test]
fn define_returns_list_wrapper_when_list_is_optional() {
    // when:
    let defs = defined::<Book>();

    // then:
    assert_eq!(
        defs.get("BookTagsList").unwrap().kind,
        Kind::Message(vec![field(
            "items",
            Label::Repeated,
            ProtoType::Scalar("string"),
            1
        )])
    );
}

#[test]
fn define_returns_repeated_fields_and_referenced_definitions() {
    // when:
    let defs = defined::<Shelf>();

    // then:
    assert_eq!(
        defs.get("Shelf").unwrap().kind,
        Kind::Message(vec![
            field("label", Label::Plain, ProtoType::Scalar("string"), 1),
            field("books", Label::Repeated, ProtoType::Named("Book"), 2),
            field("genres", Label::Repeated, ProtoType::Named("Genre"), 3),
        ])
    );
    assert_matches!(defs.get("Book"), Some(_));
    assert_matches!(defs.get("Genre"), Some(_));
}

#[test]
fn define_returns_enum_with_unspecified_value_first() {
    // when:
    let defs = defined::<Genre>();

    // then:
    let genre = defs.get("Genre").unwrap();
    assert_eq!(
        genre.kind,
        Kind::Enum(vec![
            value("GENRE_UNSPECIFIED", "", 0),
            value("GENRE_NOVEL", "", 1),
            value("GENRE_POETRY", "", 2),
            value("GENRE_REFERENCE", "Dictionaries, atlases and the like.", 3),
        ])
    );
    assert_eq!(genre.reserved, [4]);
    assert_eq!(genre.reserved_names, ["GENRE_COMICS"]);
}

#[test]
fn define_returns_discriminants_as_numbers_with_numbers_from_discriminants() {
    // when:
    let defs = defined::<Room>();

    // then:
    assert_eq!(
        defs.get("Room").unwrap().kind,
        Kind::Enum(vec![
            value("ROOM_UNSPECIFIED", "", 0),
            value("ROOM_LIBRARY", "", 0x101),
            value("ROOM_STUDY", "The study upstairs.", 0x201),
            value(
                "ROOM_ATTIC",
                "The attribute takes precedence over the (implicit) discriminant.",
                0x301
            ),
        ])
    );
}

#[test]
fn define_ignores_discriminants_without_numbers_from_discriminants() {
    // when:
    let defs = defined::<Material>();

    // then:
    assert_eq!(
        defs.get("Material").unwrap().kind,
        Kind::Enum(vec![
            value("MATERIAL_UNSPECIFIED", "", 0),
            value("MATERIAL_WOOD", "", 8),
            value("MATERIAL_METAL", "", 7),
        ])
    );
}

#[test]
fn define_returns_numbers_out_of_declaration_order() {
    // when:
    let defs = defined::<Loan>();

    // then:
    let numbers = |name| match &defs.get(name).unwrap().kind {
        Kind::Message(fields) => fields
            .iter()
            .map(|f| (f.name, f.number))
            .collect::<Vec<_>>(),
        Kind::Oneof(arms) => arms.iter().map(|a| (a.name, a.number)).collect(),
        Kind::Enum(_) => unreachable!(),
    };
    assert_eq!(
        numbers("Loan"),
        [
            ("borrower", 10),
            ("book", 21),
            ("reminders", 20),
            ("status", 22)
        ]
    );
    assert_eq!(numbers("LoanRemindersList"), [("items", 1)]);
    assert_eq!(
        numbers("LoanStatus"),
        [("open", 7), ("returned", 8), ("overdue", 9)]
    );
    assert_eq!(numbers("LoanStatusReturned"), [("day", 3), ("late", 4)]);
}

#[test]
fn define_returns_oneof_with_an_arm_per_variant() {
    // when:
    let defs = defined::<Recommendation>();

    // then:
    assert_eq!(
        defs.get("Recommendation").unwrap().kind,
        Kind::Oneof(vec![
            Arm {
                name: "book",
                ty: Some(ProtoType::Named("Book")),
                doc: "",
                number: 1,
            },
            Arm {
                name: "genre",
                ty: Some(ProtoType::Named("Genre")),
                doc: "A whole genre to explore.",
                number: 2,
            },
            Arm {
                name: "shelves",
                ty: Some(ProtoType::Named("RecommendationShelvesList")),
                doc: "Some shelves to browse.",
                number: 3,
            },
            Arm {
                name: "surprise",
                ty: None,
                doc: "Anything: the reader is open to surprises.",
                number: 4,
            },
            Arm {
                name: "by_author",
                ty: Some(ProtoType::Named("RecommendationByAuthor")),
                doc: "Books by an author, optionally of one genre only.",
                number: 5,
            },
        ])
    );
}

#[test]
fn define_returns_list_wrapper_for_list_variant() {
    // when:
    let defs = defined::<Recommendation>();

    // then:
    assert_eq!(
        defs.get("RecommendationShelvesList").unwrap().kind,
        Kind::Message(vec![field(
            "items",
            Label::Repeated,
            ProtoType::Named("Shelf"),
            1
        )])
    );
}

#[test]
fn define_returns_payload_message_for_struct_variant() {
    // when:
    let defs = defined::<Recommendation>();

    // then:
    assert_eq!(
        defs.get("RecommendationByAuthor").unwrap().kind,
        Kind::Message(vec![
            field("author", Label::Plain, ProtoType::Scalar("string"), 1),
            field("genre", Label::Optional, ProtoType::Named("Genre"), 2),
        ])
    );
}

#[test]
fn render_returns_reserved_statements() {
    // when:
    let proto = defined::<Genre>().render().to_string();

    // then:
    assert_eq!(
        proto,
        "\
// What a book is about.
enum Genre {
  GENRE_UNSPECIFIED = 0;
  GENRE_NOVEL = 1;
  GENRE_POETRY = 2;
  // Dictionaries, atlases and the like.
  GENRE_REFERENCE = 3;
  reserved 4;
  reserved \"GENRE_COMICS\";
}

"
    );
}
