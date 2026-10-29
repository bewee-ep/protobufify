//! The proto definitions `#[derive(ProtoSchema)]` records, and their rendering to `.proto` text.

use alloc::{collections::BTreeMap, vec::Vec};
use core::fmt;

/// The number of the `items` field of a `<Owner><Field>List` wrapper message.
pub const LIST_ITEMS_NUMBER: u32 = 1;

/// The proto type a Rust type maps to.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ProtoType {
    Scalar(&'static str),
    Named(&'static str),
}

/// A type with a proto representation.
pub trait ProtoSchema {
    fn proto_type() -> ProtoType;

    /// Adds the definitions of this type and of every type it references.
    fn define(_defs: &mut ProtoDefs) {}
}

macro_rules! scalars {
    ($($ty:ty => $proto:literal),* $(,)?) => {
        $(
            impl ProtoSchema for $ty {
                fn proto_type() -> ProtoType {
                    ProtoType::Scalar($proto)
                }
            }
        )*
    };
}

// Proto has no 8/16-bit integers, so those widen to the 32-bit types.
scalars! {
    bool => "bool",
    alloc::string::String => "string",
    f32 => "float",
    f64 => "double",
    u8 => "uint32",
    u16 => "uint32",
    u32 => "uint32",
    u64 => "uint64",
    i8 => "int32",
    i16 => "int32",
    i32 => "int32",
    i64 => "int64",
}

#[derive(Debug, Clone, PartialEq)]
pub struct Definition {
    /// The type's doc comment, empty if it has none.
    pub doc: &'static str,
    pub kind: Kind,
    /// Numbers no member may take, those of removed ones (`#[protobufify(reserved(N, …))]`).
    pub reserved: &'static [u32],
    /// Names no member may take, those of removed ones (`#[protobufify(reserved("name", …))]`).
    pub reserved_names: &'static [&'static str],
}

#[derive(Debug, Clone, PartialEq)]
pub enum Kind {
    Message(Vec<Field>),
    /// Values in proto order, starting with the `…_UNSPECIFIED` zero value.
    Enum(Vec<EnumValue>),
    /// A message holding a single `oneof value`.
    Oneof(Vec<Arm>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    pub name: &'static str,
    pub label: Label,
    pub ty: ProtoType,
    pub doc: &'static str,
    pub number: u32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Label {
    Plain,
    Optional,
    Repeated,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EnumValue {
    pub name: &'static str,
    pub doc: &'static str,
    /// `#[protobufify(number = …)]` or the variant's discriminant; `0` for the `…_UNSPECIFIED`
    /// value.
    pub number: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Arm {
    pub name: &'static str,
    /// `None` for a variant without payload, which becomes a `bool` arm.
    pub ty: Option<ProtoType>,
    pub doc: &'static str,
    pub number: u32,
}

/// The definitions of a `.proto` file, by type name.
#[derive(Debug, Default)]
pub struct ProtoDefs(pub(crate) BTreeMap<&'static str, Definition>);

impl ProtoDefs {
    /// Adds `definition` unless `name` is already defined; returns whether it was added.
    ///
    /// # Panics
    ///
    /// If `name` is already defined differently: two types (or generated messages) have the
    /// same proto name.
    pub fn insert(&mut self, name: &'static str, definition: Definition) -> bool {
        if let Some(defined) = self.0.get(name) {
            assert!(
                *defined == definition,
                "two different definitions are named `{name}`: rename a type with \
                 `#[protobufify(name = \"…\")]`"
            );
            return false;
        }
        self.0.insert(name, definition);
        true
    }

    pub fn get(&self, name: &str) -> Option<&Definition> {
        self.0.get(name)
    }
}

impl IntoIterator for ProtoDefs {
    type Item = (&'static str, Definition);
    type IntoIter = alloc::collections::btree_map::IntoIter<&'static str, Definition>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

/// Merges definitions, as [`ProtoDefs::insert`] does: a name defined alike more than once is kept
/// once.
///
/// # Panics
///
/// If a name is defined differently, as [`ProtoDefs::insert`].
impl FromIterator<(&'static str, Definition)> for ProtoDefs {
    fn from_iter<I: IntoIterator<Item = (&'static str, Definition)>>(iter: I) -> Self {
        let mut defs = Self::default();
        defs.extend(iter);
        defs
    }
}

/// Adds definitions, as [`ProtoDefs::insert`] does: a name already defined alike is kept once.
///
/// # Panics
///
/// If a name is defined differently, as [`ProtoDefs::insert`].
impl Extend<(&'static str, Definition)> for ProtoDefs {
    fn extend<I: IntoIterator<Item = (&'static str, Definition)>>(&mut self, iter: I) {
        for (name, definition) in iter {
            self.insert(name, definition);
        }
    }
}

impl fmt::Display for ProtoType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Scalar(name) | Self::Named(name) => f.write_str(name),
        }
    }
}

impl ProtoDefs {
    /// The definitions as `.proto` text, without the file header (`syntax`, `package`).
    pub fn render(&self) -> impl fmt::Display + '_ {
        Rendered(self)
    }
}

struct Rendered<'a>(&'a ProtoDefs);

impl fmt::Display for Rendered<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (name, definition) in &self.0.0 {
            write_doc(f, "", definition.doc)?;
            match &definition.kind {
                Kind::Message(fields) => {
                    writeln!(f, "message {name} {{")?;
                    write_fields(f, fields)?;
                }
                Kind::Enum(values) => {
                    writeln!(f, "enum {name} {{")?;
                    write_values(f, values)?;
                }
                Kind::Oneof(arms) => {
                    writeln!(f, "message {name} {{")?;
                    write_oneof(f, arms)?;
                }
            }
            write_reserved(f, definition)?;
            writeln!(f, "}}\n")?;
        }
        Ok(())
    }
}

fn write_doc(f: &mut fmt::Formatter<'_>, indent: &str, doc: &str) -> fmt::Result {
    for line in doc.lines() {
        let separator = if line.is_empty() { "" } else { " " };
        writeln!(f, "{indent}//{separator}{line}")?;
    }
    Ok(())
}

fn write_fields(f: &mut fmt::Formatter<'_>, fields: &[Field]) -> fmt::Result {
    for field in fields {
        let label = match field.label {
            Label::Plain => "",
            Label::Optional => "optional ",
            Label::Repeated => "repeated ",
        };
        write_doc(f, "  ", field.doc)?;
        writeln!(
            f,
            "  {label}{} {} = {};",
            field.ty, field.name, field.number
        )?;
    }
    Ok(())
}

fn write_values(f: &mut fmt::Formatter<'_>, values: &[EnumValue]) -> fmt::Result {
    for value in values {
        write_doc(f, "  ", value.doc)?;
        writeln!(f, "  {} = {};", value.name, value.number)?;
    }
    Ok(())
}

fn write_oneof(f: &mut fmt::Formatter<'_>, arms: &[Arm]) -> fmt::Result {
    writeln!(f, "  oneof value {{")?;
    for arm in arms {
        write_doc(f, "    ", arm.doc)?;
        match arm.ty {
            Some(ty) => writeln!(f, "    {ty} {} = {};", arm.name, arm.number)?,
            None => writeln!(f, "    bool {} = {};", arm.name, arm.number)?,
        }
    }
    writeln!(f, "  }}")
}

/// The `reserved` statements of `definition`'s numbers and names.
fn write_reserved(f: &mut fmt::Formatter<'_>, definition: &Definition) -> fmt::Result {
    if !definition.reserved.is_empty() {
        write!(f, "  reserved ")?;
        for (i, number) in definition.reserved.iter().enumerate() {
            let separator = if i == 0 { "" } else { ", " };
            write!(f, "{separator}{number}")?;
        }
        writeln!(f, ";")?;
    }
    if !definition.reserved_names.is_empty() {
        write!(f, "  reserved ")?;
        for (i, name) in definition.reserved_names.iter().enumerate() {
            let separator = if i == 0 { "" } else { ", " };
            write!(f, "{separator}\"{name}\"")?;
        }
        writeln!(f, ";")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::{string::ToString, vec};

    fn field(name: &'static str, label: Label, ty: ProtoType, number: u32) -> Field {
        Field {
            name,
            label,
            ty,
            doc: "",
            number,
        }
    }

    fn definition(kind: Kind) -> Definition {
        Definition {
            doc: "",
            kind,
            reserved: &[],
            reserved_names: &[],
        }
    }

    fn render(name: &'static str, definition: Definition) -> alloc::string::String {
        [(name, definition)]
            .into_iter()
            .collect::<ProtoDefs>()
            .render()
            .to_string()
    }

    #[test]
    fn insert_returns_false_when_name_is_defined_alike() {
        // given:
        let mut defs = ProtoDefs::default();
        defs.insert("Fixture", definition(Kind::Message(vec![])));

        // when:
        let inserted = defs.insert("Fixture", definition(Kind::Message(vec![])));

        // then:
        assert!(!inserted);
    }

    #[test]
    #[should_panic(expected = "two different definitions are named `Fixture`")]
    fn insert_panics_when_name_is_defined_differently() {
        // given:
        let mut defs = ProtoDefs::default();
        defs.insert("Fixture", definition(Kind::Message(vec![])));

        // when:
        defs.insert("Fixture", definition(Kind::Enum(vec![])));
    }

    #[test]
    fn collect_returns_name_defined_alike_once() {
        // given:
        let definitions = [
            ("Fixture", definition(Kind::Message(vec![]))),
            ("Fixture", definition(Kind::Message(vec![]))),
        ];

        // when:
        let defs: ProtoDefs = definitions.into_iter().collect();

        // then:
        assert_eq!(defs.into_iter().count(), 1);
    }

    #[test]
    #[should_panic(expected = "two different definitions are named `Fixture`")]
    fn collect_panics_when_name_is_defined_differently() {
        // given:
        let definitions = [
            ("Fixture", definition(Kind::Message(vec![]))),
            ("Fixture", definition(Kind::Enum(vec![]))),
        ];

        // when:
        let _: ProtoDefs = definitions.into_iter().collect();
    }

    #[test]
    #[should_panic(expected = "two different definitions are named `Fixture`")]
    fn extend_panics_when_name_is_defined_differently() {
        // given:
        let mut defs = ProtoDefs::default();
        defs.insert("Fixture", definition(Kind::Message(vec![])));

        // when:
        defs.extend([("Fixture", definition(Kind::Enum(vec![])))]);
    }

    #[test]
    fn render_returns_message_with_labels_and_numbers() {
        // given:
        let definition = definition(Kind::Message(vec![
            field("plain", Label::Plain, ProtoType::Scalar("double"), 1),
            field("optional", Label::Optional, ProtoType::Named("Other"), 3),
            field("repeated", Label::Repeated, ProtoType::Scalar("uint32"), 2),
        ]));

        // when:
        let proto = render("Fixture", definition);

        // then:
        assert_eq!(
            proto,
            "\
message Fixture {
  double plain = 1;
  optional Other optional = 3;
  repeated uint32 repeated = 2;
}

"
        );
    }

    #[test]
    fn render_returns_reserved_numbers_and_names() {
        // given:
        let definition = Definition {
            reserved: &[2, 3],
            reserved_names: &["b", "c"],
            ..definition(Kind::Message(vec![field(
                "a",
                Label::Plain,
                ProtoType::Scalar("bool"),
                1,
            )]))
        };

        // when:
        let proto = render("Fixture", definition);

        // then:
        assert_eq!(
            proto,
            "\
message Fixture {
  bool a = 1;
  reserved 2, 3;
  reserved \"b\", \"c\";
}

"
        );
    }

    #[test]
    fn render_returns_enum_values() {
        // given:
        let definition = definition(Kind::Enum(vec![
            EnumValue {
                name: "ROOM_UNSPECIFIED",
                doc: "",
                number: 0,
            },
            EnumValue {
                name: "ROOM_LIBRARY",
                doc: "",
                number: 0x101,
            },
        ]));

        // when:
        let proto = render("Room", definition);

        // then:
        assert_eq!(
            proto,
            "\
enum Room {
  ROOM_UNSPECIFIED = 0;
  ROOM_LIBRARY = 257;
}

"
        );
    }

    #[test]
    fn render_returns_oneof_with_bool_arm_when_variant_has_no_payload() {
        // given:
        let definition = definition(Kind::Oneof(vec![
            Arm {
                name: "number",
                ty: Some(ProtoType::Scalar("double")),
                doc: "",
                number: 1,
            },
            Arm {
                name: "nothing",
                ty: None,
                doc: "",
                number: 2,
            },
        ]));

        // when:
        let proto = render("Union", definition);

        // then:
        assert_eq!(
            proto,
            "\
message Union {
  oneof value {
    double number = 1;
    bool nothing = 2;
  }
}

"
        );
    }

    #[test]
    fn render_returns_doc_comments_above_their_items() {
        // given:
        let definition = Definition {
            doc: "First line.\n\nSecond paragraph.",
            ..definition(Kind::Message(vec![Field {
                doc: "The value.",
                ..field("value", Label::Plain, ProtoType::Scalar("bool"), 1)
            }]))
        };

        // when:
        let proto = render("Documented", definition);

        // then:
        assert_eq!(
            proto,
            "\
// First line.
//
// Second paragraph.
message Documented {
  // The value.
  bool value = 1;
}

"
        );
    }
}
