use assert_matches::assert_matches;
use protobufify::{Arm, EnumValue, Field, Kind, Label, ProtoDefs, ProtoSchema, ProtoType};
use protobufify_tests::{Book, Genre, Recommendation, Shelf};

fn defined<T: ProtoSchema>() -> ProtoDefs {
    let mut defs = ProtoDefs::default();
    T::define(&mut defs);
    defs
}

fn field(name: &'static str, label: Label, ty: ProtoType, doc: &'static str) -> Field {
    Field {
        name,
        label,
        ty,
        doc,
    }
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
            field("title", Label::Plain, ProtoType::Scalar("string"), ""),
            field("author", Label::Optional, ProtoType::Scalar("string"), ""),
            field(
                "pages",
                Label::Optional,
                ProtoType::Scalar("uint32"),
                "Number of pages; proto has no 16-bit integers, so it travels as `uint32`."
            ),
            field("genre", Label::Optional, ProtoType::Named("Genre"), ""),
            field(
                "tags",
                Label::Optional,
                ProtoType::Named("BookTagsList"),
                "`None` when unknown, empty when the book has no tags."
            ),
        ])
    );
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
            ""
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
            field("label", Label::Plain, ProtoType::Scalar("string"), ""),
            field("books", Label::Repeated, ProtoType::Named("Book"), ""),
            field("genres", Label::Repeated, ProtoType::Named("Genre"), ""),
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
    assert_eq!(
        defs.get("Genre").unwrap().kind,
        Kind::Enum(vec![
            EnumValue {
                name: "GENRE_UNSPECIFIED",
                doc: ""
            },
            EnumValue {
                name: "GENRE_NOVEL",
                doc: ""
            },
            EnumValue {
                name: "GENRE_POETRY",
                doc: ""
            },
            EnumValue {
                name: "GENRE_REFERENCE",
                doc: "Dictionaries, atlases and the like."
            },
        ])
    );
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
                doc: ""
            },
            Arm {
                name: "genre",
                ty: Some(ProtoType::Named("Genre")),
                doc: "A whole genre to explore."
            },
            Arm {
                name: "shelves",
                ty: Some(ProtoType::Named("RecommendationShelvesList")),
                doc: "Some shelves to browse."
            },
            Arm {
                name: "surprise",
                ty: None,
                doc: "Anything: the reader is open to surprises."
            },
            Arm {
                name: "by_author",
                ty: Some(ProtoType::Named("RecommendationByAuthor")),
                doc: "Books by an author, optionally of one genre only."
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
            ""
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
            field("author", Label::Plain, ProtoType::Scalar("string"), ""),
            field("genre", Label::Optional, ProtoType::Named("Genre"), ""),
        ])
    );
}
