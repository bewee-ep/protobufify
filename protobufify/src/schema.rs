//! The proto definitions `#[derive(ProtoSchema)]` records, and their rendering to `.proto` text.

use alloc::{collections::BTreeMap, vec::Vec};
use core::fmt;

use crate::ProtoLock;

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
}

#[derive(Debug, Clone, PartialEq)]
pub struct Arm {
    pub name: &'static str,
    /// `None` for a variant without payload, which becomes a `bool` arm.
    pub ty: Option<ProtoType>,
    pub doc: &'static str,
}

/// The definitions of a `.proto` file, by type name.
#[derive(Debug, Default)]
pub struct ProtoDefs(pub(crate) BTreeMap<&'static str, Definition>);

impl ProtoDefs {
    /// Adds `definition` unless `name` is already defined; returns whether it was added.
    pub fn insert(&mut self, name: &'static str, definition: Definition) -> bool {
        if self.0.contains_key(name) {
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

/// Merges definitions; a name defined more than once is the same type, so any copy will do.
impl FromIterator<(&'static str, Definition)> for ProtoDefs {
    fn from_iter<I: IntoIterator<Item = (&'static str, Definition)>>(iter: I) -> Self {
        Self(iter.into_iter().collect())
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
    /// The definitions as `.proto` text, without the file header (`syntax`, `package`), numbered
    /// by `lock`, which must be up to date with them (see [`ProtoLock::update`]).
    pub fn render<'a>(&'a self, lock: &'a ProtoLock) -> impl fmt::Display + 'a {
        Rendered { defs: self, lock }
    }
}

struct Rendered<'a> {
    defs: &'a ProtoDefs,
    lock: &'a ProtoLock,
}

impl fmt::Display for Rendered<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (name, definition) in &self.defs.0 {
            write_doc(f, "", definition.doc)?;
            match &definition.kind {
                Kind::Message(fields) => write_message(f, self.lock, name, fields)?,
                Kind::Enum(values) => write_enum(f, self.lock, name, values)?,
                Kind::Oneof(arms) => write_oneof_message(f, self.lock, name, arms)?,
            }
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

fn write_message(
    f: &mut fmt::Formatter<'_>,
    lock: &ProtoLock,
    name: &str,
    fields: &[Field],
) -> fmt::Result {
    writeln!(f, "message {name} {{")?;
    for field in fields {
        let label = match field.label {
            Label::Plain => "",
            Label::Optional => "optional ",
            Label::Repeated => "repeated ",
        };
        let number = lock.number(name, field.name);
        write_doc(f, "  ", field.doc)?;
        writeln!(f, "  {label}{} {} = {number};", field.ty, field.name)?;
    }
    write_reserved(f, lock, name)?;
    writeln!(f, "}}\n")
}

fn write_enum(
    f: &mut fmt::Formatter<'_>,
    lock: &ProtoLock,
    name: &str,
    values: &[EnumValue],
) -> fmt::Result {
    writeln!(f, "enum {name} {{")?;
    for (i, value) in values.iter().enumerate() {
        // The first value is the `…_UNSPECIFIED` zero value proto3 requires.
        let number = if i == 0 {
            0
        } else {
            lock.number(name, value.name)
        };
        write_doc(f, "  ", value.doc)?;
        writeln!(f, "  {} = {number};", value.name)?;
    }
    write_reserved(f, lock, name)?;
    writeln!(f, "}}\n")
}

fn write_oneof_message(
    f: &mut fmt::Formatter<'_>,
    lock: &ProtoLock,
    name: &str,
    arms: &[Arm],
) -> fmt::Result {
    writeln!(f, "message {name} {{")?;
    writeln!(f, "  oneof value {{")?;
    for arm in arms {
        let number = lock.number(name, arm.name);
        write_doc(f, "    ", arm.doc)?;
        match arm.ty {
            Some(ty) => writeln!(f, "    {ty} {} = {number};", arm.name)?,
            None => writeln!(f, "    bool {} = {number};", arm.name)?,
        }
    }
    writeln!(f, "  }}")?;
    write_reserved(f, lock, name)?;
    writeln!(f, "}}\n")
}

/// `reserved` statements for the numbers and names of the removed members of `name`.
fn write_reserved(f: &mut fmt::Formatter<'_>, lock: &ProtoLock, name: &str) -> fmt::Result {
    let reserved = lock.reserved(name);
    if reserved.is_empty() {
        return Ok(());
    }
    write!(f, "  reserved ")?;
    for (i, (_, number)) in reserved.iter().enumerate() {
        let separator = if i == 0 { "" } else { ", " };
        write!(f, "{separator}{number}")?;
    }
    write!(f, ";\n  reserved ")?;
    for (i, (member, _)) in reserved.iter().enumerate() {
        let separator = if i == 0 { "" } else { ", " };
        write!(f, "{separator}\"{member}\"")?;
    }
    writeln!(f, ";")
}
