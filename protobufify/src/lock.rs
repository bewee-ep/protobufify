//! The lock file: the proto number of every member ever exported, so that numbers stay stable
//! when members are inserted, reordered or removed.

use alloc::{
    collections::BTreeMap,
    format,
    string::{String, ToString},
    vec::Vec,
};
use core::{fmt, str::FromStr};

use crate::{Definition, Kind, ProtoDefs};

/// The members (message fields, oneof arms, enum values) of a definition that need a number from
/// the lock file, in declaration order. The enum `…_UNSPECIFIED` zero value is fixed.
fn numbered_members(definition: &Definition) -> Vec<&'static str> {
    match &definition.kind {
        Kind::Message(fields) => fields.iter().map(|field| field.name).collect(),
        Kind::Enum(values) => values.iter().skip(1).map(|value| value.name).collect(),
        Kind::Oneof(arms) => arms.iter().map(|arm| arm.name).collect(),
    }
}

/// The proto numbers of every member (message field, oneof arm, enum value) ever exported, by
/// type and member name; persisted as the checked-in lock file, so that numbers stay stable when
/// members are inserted, reordered or removed. A removed member stays as `reserved`, so its
/// number is never reused.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct ProtoLock(BTreeMap<String, BTreeMap<String, LockEntry>>);

#[derive(Debug, Clone, Copy, PartialEq)]
struct LockEntry {
    number: u32,
    reserved: bool,
}

/// How [`ProtoLock::update`] changed an entry.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LockAction {
    /// A new member got the next free number of its type.
    Added,
    /// A member that no longer exists keeps its number as reserved.
    Reserved,
    /// A reserved member exists again and gets its number back.
    Restored,
}

/// One entry [`ProtoLock::update`] changed.
#[derive(Debug, Clone, PartialEq)]
pub struct LockChange {
    pub action: LockAction,
    /// `<Type>.<member>`.
    pub key: String,
    pub number: u32,
}

impl fmt::Display for LockChange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let action = match self.action {
            LockAction::Added => "added",
            LockAction::Reserved => "reserved",
            LockAction::Restored => "restored",
        };
        write!(f, "{action} {} = {}", self.key, self.number)
    }
}

impl ProtoLock {
    /// Brings the lock up to date with `defs`; returns what changed, nothing if it already was.
    pub fn update(&mut self, defs: &ProtoDefs) -> Vec<LockChange> {
        let mut changes = Vec::new();
        for (ty, definition) in &defs.0 {
            let entries = self.0.entry(ty.to_string()).or_default();
            let members = numbered_members(definition);
            for member in &members {
                let key = || format!("{ty}.{member}");
                match entries.get_mut(*member) {
                    Some(entry) if !entry.reserved => {}
                    Some(entry) => {
                        entry.reserved = false;
                        changes.push(LockChange {
                            action: LockAction::Restored,
                            key: key(),
                            number: entry.number,
                        });
                    }
                    None => {
                        let number = entries.values().map(|e| e.number).max().unwrap_or(0) + 1;
                        entries.insert(
                            member.to_string(),
                            LockEntry {
                                number,
                                reserved: false,
                            },
                        );
                        changes.push(LockChange {
                            action: LockAction::Added,
                            key: key(),
                            number,
                        });
                    }
                }
            }
            for (member, entry) in entries.iter_mut() {
                if !entry.reserved && !members.contains(&member.as_str()) {
                    entry.reserved = true;
                    changes.push(LockChange {
                        action: LockAction::Reserved,
                        key: format!("{ty}.{member}"),
                        number: entry.number,
                    });
                }
            }
        }
        for (ty, entries) in &mut self.0 {
            if defs.get(ty).is_some() {
                continue;
            }
            for (member, entry) in entries.iter_mut().filter(|(_, e)| !e.reserved) {
                entry.reserved = true;
                changes.push(LockChange {
                    action: LockAction::Reserved,
                    key: format!("{ty}.{member}"),
                    number: entry.number,
                });
            }
        }
        changes
    }

    /// The number of `member` of the type `ty`; `None` if it has none or is reserved.
    pub fn number_of(&self, ty: &str, member: &str) -> Option<u32> {
        let entry = self.0.get(ty)?.get(member)?;
        (!entry.reserved).then_some(entry.number)
    }

    pub(crate) fn number(&self, ty: &str, member: &str) -> u32 {
        self.number_of(ty, member).unwrap_or_else(|| {
            panic!("`{ty}.{member}` has no number in the lock file, update it first")
        })
    }

    /// The reserved members of `ty` with their numbers, by number.
    pub(crate) fn reserved(&self, ty: &str) -> Vec<(&str, u32)> {
        let mut reserved: Vec<(&str, u32)> = self
            .0
            .get(ty)
            .into_iter()
            .flatten()
            .filter(|(_, entry)| entry.reserved)
            .map(|(member, entry)| (member.as_str(), entry.number))
            .collect();
        reserved.sort_by_key(|(_, number)| *number);
        reserved
    }
}

/// The lock file entries, `<Type>.<member> = <number>[ reserved]`, one per line, by type and
/// number; without the file header.
impl fmt::Display for ProtoLock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (ty, entries) in &self.0 {
            let mut entries: Vec<_> = entries.iter().collect();
            entries.sort_by_key(|(_, entry)| entry.number);
            for (member, entry) in entries {
                let reserved = if entry.reserved { " reserved" } else { "" };
                writeln!(f, "{ty}.{member} = {}{reserved}", entry.number)?;
            }
        }
        Ok(())
    }
}

/// A lock file line that failed parsing.
#[derive(Debug, PartialEq)]
pub struct ParseLockError {
    /// 1-based.
    pub line: usize,
    pub text: String,
}

impl fmt::Display for ParseLockError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "line {}: expected `<Type>.<member> = <number>[ reserved]`, got `{}`",
            self.line, self.text
        )
    }
}

impl core::error::Error for ParseLockError {}

/// Parses the lock file; empty lines and `#` comments are skipped.
impl FromStr for ProtoLock {
    type Err = ParseLockError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut lock = Self::default();
        for (i, text) in s.lines().enumerate() {
            let line = text.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let error = || ParseLockError {
                line: i + 1,
                text: text.to_string(),
            };
            let (key, value) = line.split_once(" = ").ok_or_else(error)?;
            let (ty, member) = key.split_once('.').ok_or_else(error)?;
            let (number, reserved) = match value.split_once(' ') {
                None => (value, false),
                Some((number, "reserved")) => (number, true),
                Some(_) => return Err(error()),
            };
            let number = number.parse().map_err(|_| error())?;
            lock.0
                .entry(ty.to_string())
                .or_default()
                .insert(member.to_string(), LockEntry { number, reserved });
        }
        Ok(lock)
    }
}

/// The comment [`update_lock_file`] writes at the top of a lock file.
pub const LOCK_FILE_HEADER: &str = "\
# Protobuf field and enum value numbers, maintained by protobufify. Commit changes.
# Do not edit by hand: the numbers are part of the wire format.
";

/// Why [`update_lock_file`] failed.
#[cfg(feature = "std")]
#[derive(Debug)]
pub enum LockFileError {
    /// Reading or writing the lock file failed; frozen, a missing file fails here too.
    Io(std::io::Error),
    /// The lock file is malformed.
    Parse(ParseLockError),
    /// Frozen, and the lock file would change.
    OutOfDate(Vec<LockChange>),
}

#[cfg(feature = "std")]
impl fmt::Display for LockFileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "{e}"),
            Self::Parse(e) => write!(f, "{e}"),
            Self::OutOfDate(changes) => {
                write!(f, "the lock file is out of date:")?;
                for change in changes {
                    write!(f, "\n  {change}")?;
                }
                Ok(())
            }
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for LockFileError {}

/// Brings the lock file at `path` up to date with `defs` and returns it with what changed; a
/// missing file starts empty.
///
/// `frozen` (for CI) writes nothing: a lock file that would change fails with
/// [`LockFileError::OutOfDate`], a missing one with [`LockFileError::Io`].
#[cfg(feature = "std")]
pub fn update_lock_file(
    path: impl AsRef<std::path::Path>,
    defs: &ProtoDefs,
    frozen: bool,
) -> Result<(ProtoLock, Vec<LockChange>), LockFileError> {
    let path = path.as_ref();
    let mut lock = match std::fs::read_to_string(path) {
        Ok(text) => text.parse().map_err(LockFileError::Parse)?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound && !frozen => ProtoLock::default(),
        Err(e) => return Err(LockFileError::Io(e)),
    };
    let changes = lock.update(defs);
    if !changes.is_empty() {
        if frozen {
            return Err(LockFileError::OutOfDate(changes));
        }
        std::fs::write(path, format!("{LOCK_FILE_HEADER}{lock}")).map_err(LockFileError::Io)?;
    }
    Ok((lock, changes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Arm, EnumValue, Field, Label, ProtoType};
    use alloc::vec;
    use assert_matches::assert_matches;

    fn field(name: &'static str, label: Label, ty: ProtoType) -> Field {
        Field {
            name,
            label,
            ty,
            doc: "",
        }
    }

    fn defs(definitions: Vec<(&'static str, Definition)>) -> ProtoDefs {
        definitions.into_iter().collect()
    }

    /// A message of plain `bool` fields named `fields`.
    fn message(fields: &[&'static str]) -> Definition {
        Definition {
            doc: "",
            kind: Kind::Message(
                fields
                    .iter()
                    .map(|name| field(name, Label::Plain, ProtoType::Scalar("bool")))
                    .collect(),
            ),
        }
    }

    fn lock(text: &str) -> ProtoLock {
        text.parse().unwrap()
    }

    /// `lock` with numbers for everything in `defs`.
    fn locked(defs: &ProtoDefs) -> ProtoLock {
        let mut lock = ProtoLock::default();
        lock.update(defs);
        lock
    }

    fn change(action: LockAction, key: &str, number: u32) -> LockChange {
        LockChange {
            action,
            key: key.to_string(),
            number,
        }
    }

    #[test]
    fn from_str_returns_lock_that_writes_back_unchanged() {
        // given:
        let text = "Fixture.b = 1\nFixture.a = 2 reserved\nOther.X = 1\n";

        // when:
        let lock: ProtoLock = text.parse().unwrap();

        // then:
        assert_eq!(lock.to_string(), text);
    }

    #[test]
    fn from_str_returns_entries_sorted_by_number_and_skips_comments() {
        // given:
        let text = "# comment\n\nFixture.a = 2\nFixture.b = 1\n";

        // when:
        let lock: ProtoLock = text.parse().unwrap();

        // then:
        assert_eq!(lock.to_string(), "Fixture.b = 1\nFixture.a = 2\n");
    }

    #[test]
    fn from_str_returns_error_with_line_number_when_entry_is_malformed() {
        // given:
        let text = "# comment\nFixture.a = 1\nFixture.b = two\n";

        // when:
        let result = text.parse::<ProtoLock>();

        // then:
        assert_matches!(result, Err(ParseLockError { line: 3, .. }));
    }

    #[test]
    fn update_returns_numbers_in_declaration_order_when_lock_is_empty() {
        // given:
        let defs = defs(vec![
            ("Fixture", message(&["b", "a"])),
            (
                "Color",
                Definition {
                    doc: "",
                    kind: Kind::Enum(vec![
                        EnumValue {
                            name: "COLOR_UNSPECIFIED",
                            doc: "",
                        },
                        EnumValue {
                            name: "COLOR_RED",
                            doc: "",
                        },
                    ]),
                },
            ),
        ]);
        let mut lock = ProtoLock::default();

        // when:
        let changes = lock.update(&defs);

        // then:
        assert_eq!(
            lock.to_string(),
            "Color.COLOR_RED = 1\nFixture.b = 1\nFixture.a = 2\n"
        );
        assert_eq!(changes.len(), 3);
    }

    #[test]
    fn update_returns_next_free_number_when_member_is_inserted_in_the_middle() {
        // given:
        let defs = defs(vec![("Fixture", message(&["a", "b", "c"]))]);
        let mut lock = lock("Fixture.a = 1\nFixture.c = 2\n");

        // when:
        let changes = lock.update(&defs);

        // then:
        assert_eq!(
            lock.to_string(),
            "Fixture.a = 1\nFixture.c = 2\nFixture.b = 3\n"
        );
        assert_eq!(changes, vec![change(LockAction::Added, "Fixture.b", 3)]);
    }

    #[test]
    fn update_returns_reserved_member_when_member_is_removed() {
        // given:
        let defs = defs(vec![("Fixture", message(&["a", "c"]))]);
        let mut lock = lock("Fixture.a = 1\nFixture.b = 2\n");

        // when:
        let changes = lock.update(&defs);

        // then:
        assert_eq!(
            lock.to_string(),
            "Fixture.a = 1\nFixture.b = 2 reserved\nFixture.c = 3\n"
        );
        assert_eq!(
            changes,
            vec![
                change(LockAction::Added, "Fixture.c", 3),
                change(LockAction::Reserved, "Fixture.b", 2),
            ]
        );
    }

    #[test]
    fn update_returns_reserved_members_when_type_is_removed() {
        // given:
        let defs = defs(vec![]);
        let mut lock = lock("Fixture.a = 1\n");

        // when:
        let changes = lock.update(&defs);

        // then:
        assert_eq!(lock.to_string(), "Fixture.a = 1 reserved\n");
        assert_eq!(changes, vec![change(LockAction::Reserved, "Fixture.a", 1)]);
    }

    #[test]
    fn update_returns_old_number_when_reserved_member_reappears() {
        // given:
        let defs = defs(vec![("Fixture", message(&["a", "b"]))]);
        let mut lock = lock("Fixture.a = 1\nFixture.b = 2 reserved\n");

        // when:
        let changes = lock.update(&defs);

        // then:
        assert_eq!(lock.to_string(), "Fixture.a = 1\nFixture.b = 2\n");
        assert_eq!(changes, vec![change(LockAction::Restored, "Fixture.b", 2)]);
    }

    #[test]
    fn update_returns_no_changes_when_lock_is_up_to_date() {
        // given:
        let defs = defs(vec![("Fixture", message(&["a", "b"]))]);
        let mut lock = lock("Fixture.a = 1\nFixture.b = 2\n");

        // when:
        let changes = lock.update(&defs);

        // then:
        assert_eq!(changes, vec![]);
    }

    #[test]
    fn render_returns_message_with_numbers_from_lock() {
        // given:
        let defs = defs(vec![(
            "Fixture",
            Definition {
                doc: "",
                kind: Kind::Message(vec![
                    field("plain", Label::Plain, ProtoType::Scalar("double")),
                    field("optional", Label::Optional, ProtoType::Named("Other")),
                    field("repeated", Label::Repeated, ProtoType::Scalar("uint32")),
                ]),
            },
        )]);
        let lock = lock("Fixture.plain = 1\nFixture.repeated = 2\nFixture.optional = 3\n");

        // when:
        let proto = defs.render(&lock).to_string();

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
    fn render_returns_reserved_numbers_and_names_of_removed_members() {
        // given:
        let defs = defs(vec![("Fixture", message(&["a"]))]);
        let lock = lock("Fixture.a = 1\nFixture.b = 2 reserved\nFixture.c = 3 reserved\n");

        // when:
        let proto = defs.render(&lock).to_string();

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
    fn render_returns_enum_with_unspecified_as_zero() {
        // given:
        let defs = defs(vec![(
            "Color",
            Definition {
                doc: "",
                kind: Kind::Enum(vec![
                    EnumValue {
                        name: "COLOR_UNSPECIFIED",
                        doc: "",
                    },
                    EnumValue {
                        name: "COLOR_RED",
                        doc: "",
                    },
                ]),
            },
        )]);
        let lock = locked(&defs);

        // when:
        let proto = defs.render(&lock).to_string();

        // then:
        assert_eq!(
            proto,
            "\
enum Color {
  COLOR_UNSPECIFIED = 0;
  COLOR_RED = 1;
}

"
        );
    }

    #[test]
    fn render_returns_oneof_with_bool_arm_when_variant_has_no_payload() {
        // given:
        let defs = defs(vec![(
            "Union",
            Definition {
                doc: "",
                kind: Kind::Oneof(vec![
                    Arm {
                        name: "number",
                        ty: Some(ProtoType::Scalar("double")),
                        doc: "",
                    },
                    Arm {
                        name: "nothing",
                        ty: None,
                        doc: "",
                    },
                ]),
            },
        )]);
        let lock = locked(&defs);

        // when:
        let proto = defs.render(&lock).to_string();

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
        let defs = defs(vec![(
            "Documented",
            Definition {
                doc: "First line.\n\nSecond paragraph.",
                kind: Kind::Message(vec![Field {
                    name: "value",
                    label: Label::Plain,
                    ty: ProtoType::Scalar("bool"),
                    doc: "The value.",
                }]),
            },
        )]);
        let lock = locked(&defs);

        // when:
        let proto = defs.render(&lock).to_string();

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
