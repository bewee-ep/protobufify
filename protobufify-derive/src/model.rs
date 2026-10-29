//! The analysis all three derives share: [`analyze`] turns a type into a [`Model`], with the proto
//! name and number of every member and the names of the messages generated for it, after
//! checking all of them. The derives only generate code from the model, so each check, and each
//! error, exists once.
//!
//! A number is `#[protobufify(number = N)]`, else, for a value of an enum
//! `#[protobufify(numbers_from_discriminants)]`, its discriminant. A name is
//! `#[protobufify(name = "…")]`, else the Rust identifier converted to the protobuf style guide.
//! Discriminants, which only the compiler knows, are checked by constants the model carries.

use proc_macro2::{Literal, Span, TokenStream};
use quote::{quote, quote_spanned};
use syn::{
    Data, DataEnum, DeriveInput, Error, Field, Fields, FieldsNamed, Ident, LitInt, Type, Variant,
    spanned::Spanned,
};

use crate::attrs::{self, MemberAttrs, ReservedItem};
use crate::errors::Errors;
use crate::names::{
    PROTO_NAME, arm_name, enum_value_name, field_list_name, field_name, is_proto_name, json_name,
    payload_name, type_name, unspecified_name, variant_list_name,
};
use crate::shape::{FieldShape, is_fieldless, vec_inner};

/// The valid numbers of a member, by what it is.
#[derive(Clone, Copy)]
enum Range {
    /// A message field or oneof arm: `1..=536_870_911`, except the reserved `19000..=19999`.
    Field,
    /// An enum value: `1..=i32::MAX`, `0` being the `…_UNSPECIFIED` value.
    EnumValue,
}

impl Range {
    fn allows(self, number: u32) -> bool {
        match self {
            Self::Field => {
                (1..=536_870_911).contains(&number) && !(19000..=19999).contains(&number)
            }
            Self::EnumValue => (1..=i32::MAX as u32).contains(&number),
        }
    }

    fn message(self) -> &'static str {
        match self {
            Self::Field => "a field number must be in 1..=536870911, except 19000..=19999",
            Self::EnumValue => {
                "an enum value number must be in 1..=2147483647, 0 is the `…_UNSPECIFIED` value"
            }
        }
    }

    /// The number `lit`, if in range.
    fn parse(self, lit: &LitInt) -> syn::Result<u32> {
        lit.base10_parse::<u32>()
            .ok()
            .filter(|n| self.allows(*n))
            .ok_or_else(|| Error::new(lit.span(), format!("protobufify: {}", self.message())))
    }
}

/// The numbers and names of a type's removed members, which no member may take.
#[derive(Default)]
pub(crate) struct Reserved {
    pub(crate) numbers: Vec<u32>,
    pub(crate) names: Vec<String>,
}

/// A type, analyzed.
pub(crate) struct Model<'a> {
    pub(crate) input: &'a DeriveInput,
    /// The proto name.
    pub(crate) name: String,
    pub(crate) reserved: Reserved,
    pub(crate) body: Body<'a>,
}

/// What a type becomes.
pub(crate) enum Body<'a> {
    /// A struct: a message.
    Message(Vec<ResolvedField<'a>>),
    /// An enum without fields: an enum, and the constants checking the discriminants among its
    /// numbers, which every derive emits, so that each checks the numbers it uses.
    Enum {
        values: Vec<ResolvedValue<'a>>,
        checks: TokenStream,
    },
    /// An enum with fields: a message with a `oneof`.
    Oneof(Vec<ResolvedArm<'a>>),
}

/// A named field of a message.
pub(crate) struct ResolvedField<'a> {
    pub(crate) field: &'a Field,
    pub(crate) name: String,
    pub(crate) number: u32,
    pub(crate) shape: Shape<'a>,
}

/// How a field maps to protobuf.
pub(crate) enum Shape<'a> {
    /// A plain field of the type.
    Plain(&'a Type),
    /// `Option<T>`: `optional T`.
    Optional(&'a Type),
    /// `Vec<T>`: `repeated T`.
    Repeated(&'a Type),
    /// `Option<Vec<T>>`: an `optional` wrapper message around a `repeated T items`.
    OptionalList { item: &'a Type, wrapper: String },
}

/// A variant of an enum with fields: an arm of the `oneof`.
pub(crate) struct ResolvedArm<'a> {
    pub(crate) variant: &'a Variant,
    pub(crate) name: String,
    pub(crate) number: u32,
    pub(crate) payload: Payload<'a>,
}

/// What an arm holds.
pub(crate) enum Payload<'a> {
    /// A unit variant: a `bool`, set to select the variant.
    Unit,
    /// A tuple variant of one value.
    Single(&'a Type),
    /// A tuple variant of one `Vec<T>`: a wrapper message around a `repeated T items`.
    List { item: &'a Type, wrapper: String },
    /// A struct variant: a payload message of its fields.
    Message {
        name: String,
        fields: Vec<ResolvedField<'a>>,
        reserved: Reserved,
    },
}

/// A value of an enum without fields.
pub(crate) struct ResolvedValue<'a> {
    pub(crate) variant: &'a Variant,
    pub(crate) name: String,
    pub(crate) number: Number,
}

/// The number of an enum value.
#[derive(Clone, Copy)]
pub(crate) enum Number {
    /// From `#[protobufify(number = N)]`.
    Explicit(u32),
    /// The variant's discriminant, `Self::<variant> as i32`.
    Discriminant,
}

/// Analyzes `input`, reporting every error found.
pub(crate) fn analyze(input: &DeriveInput) -> syn::Result<Model<'_>> {
    // The model is built even when errors are found, so that later checks still run on what is
    // valid; it is only returned without errors.
    let mut errors = Errors::default();
    if !input.generics.params.is_empty() {
        errors.push(Error::new(
            input.generics.span(),
            "protobufify: generic types are not supported",
        ));
    }
    let attrs = attrs::type_attrs(input, &mut errors);
    let name = match &attrs.name {
        Some(lit) => checked_name(lit.value(), lit.span(), false, &mut errors),
        None => checked_name(
            type_name(&input.ident),
            input.ident.span(),
            true,
            &mut errors,
        ),
    };
    let range = match &input.data {
        Data::Enum(data) if is_fieldless(data) => Range::EnumValue,
        _ => Range::Field,
    };
    let reserved = reserved(attrs.reserved, range, &mut errors);
    let body = match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(named) => Body::Message(fields(&name, named, &reserved, &mut errors)),
            fields => {
                errors.push(Error::new(
                    fields.span(),
                    "protobufify: tuple and unit structs are not supported: name the fields",
                ));
                Body::Message(vec![])
            }
        },
        Data::Enum(data) if is_fieldless(data) => values(
            &input.ident,
            &name,
            data,
            attrs.numbers_from_discriminants,
            &reserved,
            &mut errors,
        ),
        Data::Enum(data) => Body::Oneof(arms(&name, data, &reserved, &mut errors)),
        Data::Union(_) => {
            errors.push(Error::new(
                input.span(),
                "protobufify: unions are not supported",
            ));
            Body::Message(vec![])
        }
    };
    check_generated(&name, &body, &mut errors);
    errors.finish()?;
    Ok(Model {
        input,
        name,
        reserved,
        body,
    })
}

/// `name`, reporting it at `span` unless it is a proto name; `derived` from a Rust identifier,
/// which a `name = "…"` can replace.
fn checked_name(name: String, span: Span, derived: bool, errors: &mut Errors) -> String {
    if let Some(message) = invalid_name(&name, derived) {
        errors.push(Error::new(span, message));
    }
    name
}

/// The error message if `name` is not a proto name; `derived` from a Rust identifier, which a
/// `name = "…"` can replace.
fn invalid_name(name: &str, derived: bool) -> Option<String> {
    if is_proto_name(name) {
        None
    } else if derived {
        Some(format!(
            "protobufify: `{name}` is not a proto name ({PROTO_NAME}): add \
             `#[protobufify(name = \"…\")]`"
        ))
    } else {
        Some(format!(
            "protobufify: {name:?} is not a proto name: {PROTO_NAME}"
        ))
    }
}

/// The reserved numbers (in `range`) and names of `items`, each valid and given once.
fn reserved(items: Vec<ReservedItem>, range: Range, errors: &mut Errors) -> Reserved {
    let mut reserved = Reserved::default();
    for item in items {
        match item {
            ReservedItem::Number(lit) => match range.parse(&lit) {
                Ok(n) if reserved.numbers.contains(&n) => {
                    let message = format!("protobufify: {n} is reserved twice");
                    errors.push(Error::new(lit.span(), message));
                }
                Ok(n) => reserved.numbers.push(n),
                Err(error) => errors.push(error),
            },
            ReservedItem::Name(lit) => {
                let name = checked_name(lit.value(), lit.span(), false, errors);
                if reserved.names.contains(&name) {
                    let message = format!("protobufify: `{name}` is reserved twice");
                    errors.push(Error::new(lit.span(), message));
                } else {
                    reserved.names.push(name);
                }
            }
        }
    }
    reserved
}

/// A member to number and name.
struct Member {
    /// Its proto name, given or derived.
    name: String,
    /// Whether the name is derived from the Rust identifier.
    derived: bool,
    /// Where errors about it point: the name if given, else the Rust identifier.
    span: Span,
    number: Option<LitInt>,
    /// Whether its attribute failed to parse, so that its number may be missing.
    broken: bool,
}

impl Member {
    /// The member of `ident`, named `derived` unless `attrs` names it, and the reserved numbers
    /// and names `attrs` gives for its payload message.
    fn new(ident: &Ident, derived: String, attrs: MemberAttrs) -> (Self, Vec<ReservedItem>) {
        let (name, derived, span) = match attrs.name {
            Some(lit) => (lit.value(), false, lit.span()),
            None => (derived, true, ident.span()),
        };
        let member = Self {
            name,
            derived,
            span,
            number: attrs.number,
            broken: attrs.broken,
        };
        (member, attrs.reserved)
    }
}

/// What the members of a type must follow.
struct Rules<'a> {
    /// The proto name of the type.
    owner: &'a str,
    range: Range,
    reserved: &'a Reserved,
    /// Names no member may take, with what they are the name of.
    taken: &'a [(&'a str, &'a str)],
    /// Whether the members are fields, whose JSON names must differ too.
    fields: bool,
    /// Whether a member without a number is numbered by its discriminant.
    discriminants: bool,
}

/// The explicit numbers of `members`, checking their names and numbers. `None` for a member
/// numbered by its discriminant if `rules.discriminants`; otherwise `None` only with an error
/// reported, for the member or its attribute.
fn resolve(rules: &Rules, members: &[Member], errors: &mut Errors) -> Vec<Option<u32>> {
    let owner = rules.owner;
    let mut numbers: Vec<Option<u32>> = vec![];
    for (i, member) in members.iter().enumerate() {
        let name = &member.name;
        let key = format!("{owner}.{name}");
        let earlier = &members[..i];
        let mut error = |message: String| errors.push(Error::new(member.span, message));
        if let Some(message) = invalid_name(name, member.derived) {
            error(message);
        } else if earlier.iter().any(|other| other.name == *name) {
            error(format!("protobufify: `{key}` is the name of two members"));
        } else if let Some(other) = earlier
            .iter()
            .find(|other| rules.fields && json_name(&other.name) == json_name(name))
        {
            error(format!(
                "protobufify: `{key}` and `{owner}.{}` have the same JSON name `{}`",
                other.name,
                json_name(name)
            ));
        } else if rules.reserved.names.contains(name) {
            error(format!("protobufify: `{key}` is a reserved name"));
        } else if let Some((_, what)) = rules.taken.iter().find(|(taken, _)| taken == name) {
            error(format!("protobufify: `{key}` is the name of the {what}"));
        }
        let number = match &member.number {
            Some(lit) => match rules.range.parse(lit) {
                Ok(n) => {
                    let twice = earlier
                        .iter()
                        .zip(&numbers)
                        .find(|(_, number)| **number == Some(n));
                    if let Some((other, _)) = twice {
                        let message = format!(
                            "protobufify: {n} is also the number of `{owner}.{}`",
                            other.name
                        );
                        errors.push(Error::new(lit.span(), message));
                    } else if rules.reserved.numbers.contains(&n) {
                        let message = format!("protobufify: {n} is a reserved number of `{owner}`");
                        errors.push(Error::new(lit.span(), message));
                    }
                    Some(n)
                }
                Err(e) => {
                    errors.push(e);
                    None
                }
            },
            None if rules.discriminants || member.broken => None,
            None => {
                let message =
                    format!("protobufify: `{key}` has no number: add `#[protobufify(number = N)]`");
                errors.push(Error::new(member.span, message));
                None
            }
        };
        numbers.push(number);
    }
    numbers
}

/// The named fields of the message `owner`, which reserves `reserved`, in declaration order.
fn fields<'a>(
    owner: &str,
    named: &'a FieldsNamed,
    reserved: &Reserved,
    errors: &mut Errors,
) -> Vec<ResolvedField<'a>> {
    let members: Vec<_> = named
        .named
        .iter()
        .map(|field| {
            let ident = field.ident.as_ref().expect("named field");
            let attrs = attrs::member_attrs(&field.attrs, false, errors);
            Member::new(ident, field_name(ident), attrs).0
        })
        .collect();
    let rules = Rules {
        owner,
        range: Range::Field,
        reserved,
        taken: &[],
        fields: true,
        discriminants: false,
    };
    let numbers = resolve(&rules, &members, errors);
    named
        .named
        .iter()
        .zip(members)
        .zip(numbers)
        .filter_map(|((field, member), number)| {
            let shape = match FieldShape::of(&field.ty) {
                FieldShape::Plain(ty) => Shape::Plain(ty),
                FieldShape::Optional(inner) => Shape::Optional(inner),
                FieldShape::Repeated(item) => Shape::Repeated(item),
                FieldShape::OptionalList(item) => Shape::OptionalList {
                    item,
                    wrapper: field_list_name(owner, &member.name),
                },
            };
            Some(ResolvedField {
                field,
                name: member.name,
                number: number?,
                shape,
            })
        })
        .collect()
}

/// The variants of the enum with fields `owner`, which reserves `reserved`, as `oneof` arms in
/// declaration order.
fn arms<'a>(
    owner: &str,
    data: &'a DataEnum,
    reserved: &Reserved,
    errors: &mut Errors,
) -> Vec<ResolvedArm<'a>> {
    let mut members = vec![];
    let mut payloads_reserved = vec![];
    for variant in &data.variants {
        if let Fields::Unnamed(f) = &variant.fields {
            for field in &f.unnamed {
                attrs::reject(&field.attrs, errors);
            }
        }
        let payload = matches!(variant.fields, Fields::Named(_));
        let attrs = attrs::member_attrs(&variant.attrs, payload, errors);
        let (member, payload_reserved) =
            Member::new(&variant.ident, arm_name(&variant.ident), attrs);
        members.push(member);
        payloads_reserved.push(payload_reserved);
    }
    let rules = Rules {
        owner,
        range: Range::Field,
        reserved,
        taken: &[("value", "oneof")],
        fields: true,
        discriminants: false,
    };
    let numbers = resolve(&rules, &members, errors);
    let mut arms = vec![];
    for ((variant, member), (number, payload_reserved)) in data
        .variants
        .iter()
        .zip(members)
        .zip(numbers.into_iter().zip(payloads_reserved))
    {
        let payload = match &variant.fields {
            Fields::Unit => Payload::Unit,
            Fields::Unnamed(f) if f.unnamed.len() == 1 => {
                let ty = &f.unnamed[0].ty;
                match vec_inner(ty) {
                    Some(item) => Payload::List {
                        item,
                        wrapper: variant_list_name(owner, &member.name),
                    },
                    None => Payload::Single(ty),
                }
            }
            Fields::Unnamed(f) => {
                errors.push(Error::new(
                    f.span(),
                    "protobufify: tuple variants need exactly one field",
                ));
                continue;
            }
            Fields::Named(named) => {
                let name = payload_name(owner, &member.name);
                let reserved = self::reserved(payload_reserved, Range::Field, errors);
                let fields = fields(&name, named, &reserved, errors);
                Payload::Message {
                    name,
                    fields,
                    reserved,
                }
            }
        };
        if let Some(number) = number {
            arms.push(ResolvedArm {
                variant,
                name: member.name,
                number,
                payload,
            });
        }
    }
    arms
}

/// The values of the enum without fields `ident`, proto name `owner`, which reserves `reserved`:
/// numbered explicitly, else by the discriminant if `from_discriminants`.
fn values<'a>(
    ident: &Ident,
    owner: &str,
    data: &'a DataEnum,
    from_discriminants: bool,
    reserved: &Reserved,
    errors: &mut Errors,
) -> Body<'a> {
    let members: Vec<_> = data
        .variants
        .iter()
        .map(|variant| {
            let attrs = attrs::member_attrs(&variant.attrs, false, errors);
            Member::new(
                &variant.ident,
                enum_value_name(owner, &variant.ident),
                attrs,
            )
            .0
        })
        .collect();
    let unspecified = unspecified_name(owner);
    let rules = Rules {
        owner,
        range: Range::EnumValue,
        reserved,
        taken: &[(&unspecified, "zero value")],
        fields: false,
        discriminants: from_discriminants,
    };
    let numbers = resolve(&rules, &members, errors);
    let explicit: Vec<(u32, &str)> = numbers
        .iter()
        .zip(&members)
        .filter_map(|(number, member)| Some(((*number)?, member.name.as_str())))
        .collect();
    let mut checks = vec![];
    let mut values = vec![];
    for ((variant, member), number) in data.variants.iter().zip(&members).zip(&numbers) {
        let number = match number {
            Some(n) => Number::Explicit(*n),
            None if from_discriminants => {
                checks.push(discriminant_checks(
                    ident, owner, variant, &explicit, reserved,
                ));
                Number::Discriminant
            }
            // Reported.
            None => continue,
        };
        values.push(ResolvedValue {
            variant,
            name: member.name.clone(),
            number,
        });
    }
    Body::Enum {
        values,
        checks: quote!(#(#checks)*),
    }
}

/// Constants asserting that the value `variant` of the enum `ident` (proto name `owner`), numbered
/// by its discriminant, has a valid number, which is neither one of the `explicit` numbers of
/// other values nor `reserved`.
fn discriminant_checks(
    ident: &Ident,
    owner: &str,
    variant: &Variant,
    explicit: &[(u32, &str)],
    reserved: &Reserved,
) -> TokenStream {
    let v = &variant.ident;
    let mut checks = vec![];
    // One constant per assertion, so that each failing one is reported. Spanned at the variant,
    // so that rustc reports it once rather than once per derive.
    let mut check = |condition: TokenStream, message: String| {
        checks.push(quote_spanned!(v.span()=> const _: () = assert!(#condition, #message);));
    };
    // `i128` holds every discriminant but those of a `u128` beyond it, which it makes negative,
    // so out of range too.
    let value = quote!(#ident::#v as i128);
    let of = format!("protobufify: the discriminant of {ident}::{v}");
    let unspecified = unspecified_name(owner);
    check(
        quote!(#value >= 1 && #value <= i32::MAX as i128),
        format!("{of} must be in 1..=2147483647, 0 is the {unspecified} value"),
    );
    for (n, name) in explicit {
        let message = format!("{of} is also the number of `{owner}.{name}`");
        let n = Literal::u32_unsuffixed(*n);
        check(quote!(#value != #n), message);
    }
    for n in &reserved.numbers {
        let message = format!("{of} is a reserved number of `{owner}`");
        let n = Literal::u32_unsuffixed(*n);
        check(quote!(#value != #n), message);
    }
    quote!(#(#checks)*)
}

/// The list wrappers generated for `fields`, at the fields.
fn wrappers<'b>(fields: &'b [ResolvedField<'_>]) -> impl Iterator<Item = (&'b str, Span)> {
    fields.iter().filter_map(|f| match &f.shape {
        Shape::OptionalList { wrapper, .. } => Some((wrapper.as_str(), f.field.span())),
        _ => None,
    })
}

/// Reports the messages generated for the type `owner` (list wrappers, payload messages) that
/// have the same name.
fn check_generated(owner: &str, body: &Body, errors: &mut Errors) {
    let mut generated: Vec<(&str, Span)> = vec![];
    match body {
        Body::Message(fields) => generated.extend(wrappers(fields)),
        Body::Enum { .. } => {}
        Body::Oneof(arms) => {
            for arm in arms {
                let span = arm.variant.ident.span();
                match &arm.payload {
                    Payload::List { wrapper, .. } => generated.push((wrapper, span)),
                    Payload::Message { name, fields, .. } => {
                        generated.push((name, span));
                        generated.extend(wrappers(fields));
                    }
                    Payload::Unit | Payload::Single(_) => {}
                }
            }
        }
    }
    for (i, (name, span)) in generated.iter().enumerate() {
        if generated[..i].iter().any(|(other, _)| other == name) {
            let message = format!(
                "protobufify: `{name}` is the name of two messages generated for `{owner}`: \
                 rename a field or variant with `#[protobufify(name = \"…\")]`"
            );
            errors.push(Error::new(*span, message));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use syn::parse_quote;

    /// The error messages of analyzing `input`.
    fn errors(input: DeriveInput) -> Vec<String> {
        match analyze(&input) {
            Ok(_) => panic!("expected errors"),
            Err(errors) => errors.into_iter().map(|e| e.to_string()).collect(),
        }
    }

    /// The proto names and numbers of the fields of the struct `input`.
    fn fields_of(input: &DeriveInput) -> Vec<(String, u32)> {
        let model = analyze(input).unwrap_or_else(|e| panic!("{e}"));
        let Body::Message(fields) = model.body else {
            panic!("expected a message");
        };
        fields.into_iter().map(|f| (f.name, f.number)).collect()
    }

    #[test]
    fn analyze_returns_explicit_numbers() {
        // given:
        let input = parse_quote! {
            struct Book { #[protobufify(number = 5)] title: String, #[protobufify(number = 2)] pages: u32 }
        };

        // when:
        let fields = fields_of(&input);

        // then:
        assert_eq!(fields, [("title".to_string(), 5), ("pages".to_string(), 2)]);
    }

    #[test]
    fn analyze_returns_errors_for_all_members_without_number() {
        // given:
        let input = parse_quote!(
            struct Book {
                title: String,
                pages: u32,
            }
        );

        // when:
        let errors = errors(input);

        // then:
        assert_eq!(
            errors,
            [
                "protobufify: `Book.title` has no number: add `#[protobufify(number = N)]`",
                "protobufify: `Book.pages` has no number: add `#[protobufify(number = N)]`",
            ]
        );
    }

    #[test]
    fn analyze_returns_errors_for_all_invalid_numbers() {
        // given:
        let input = parse_quote! {
            struct Book {
                #[protobufify(number = 0)] title: String,
                #[protobufify(number = 19000)] pages: u32,
            }
        };

        // when:
        let errors = errors(input);

        // then:
        let message = "protobufify: a field number must be in 1..=536870911, except 19000..=19999";
        assert_eq!(errors, [message, message]);
    }

    #[test]
    fn analyze_returns_error_when_number_is_given_twice() {
        // given:
        let input = parse_quote! {
            struct Book { #[protobufify(number = 3)] title: String, #[protobufify(number = 3)] pages: u32 }
        };

        // when:
        let errors = errors(input);

        // then:
        assert_eq!(
            errors,
            ["protobufify: 3 is also the number of `Book.title`"]
        );
    }

    #[test]
    fn analyze_returns_errors_when_number_or_name_is_reserved() {
        // given:
        let input = parse_quote! {
            #[protobufify(reserved(2, 4, "isbn"))]
            struct Book { #[protobufify(number = 4)] title: String, #[protobufify(number = 5)] isbn: u64 }
        };

        // when:
        let errors = errors(input);

        // then:
        assert_eq!(
            errors,
            [
                "protobufify: 4 is a reserved number of `Book`",
                "protobufify: `Book.isbn` is a reserved name",
            ]
        );
    }

    #[test]
    fn analyze_returns_given_names_verbatim_else_identifiers_in_snake_case() {
        // given:
        let input = parse_quote! {
            struct Book {
                #[protobufify(number = 1, name = "headingText")] title: String,
                #[protobufify(number = 2)] r#type: u32,
                #[protobufify(number = 3)] pageCount: u32,
            }
        };

        // when:
        let fields = fields_of(&input);

        // then:
        assert_eq!(
            fields,
            [
                ("headingText".to_string(), 1),
                ("type".to_string(), 2),
                ("page_count".to_string(), 3),
            ]
        );
    }

    #[test]
    fn analyze_returns_error_when_identifiers_have_the_same_snake_case() {
        // given:
        let input = parse_quote! {
            struct Book { #[protobufify(number = 1)] page_count: u32, #[protobufify(number = 2)] pageCount: u32 }
        };

        // when:
        let errors = errors(input);

        // then:
        assert_eq!(
            errors,
            ["protobufify: `Book.page_count` is the name of two members"]
        );
    }

    #[test]
    fn analyze_returns_error_when_fields_have_the_same_json_name() {
        // given:
        let input = parse_quote! {
            struct Book {
                #[protobufify(number = 1)] page_count: u32,
                #[protobufify(number = 2, name = "pageCount")] pages: u32,
            }
        };

        // when:
        let errors = errors(input);

        // then:
        assert_eq!(
            errors,
            [
                "protobufify: `Book.pageCount` and `Book.page_count` have the same JSON name `pageCount`"
            ]
        );
    }

    #[test]
    fn analyze_returns_no_error_when_enum_values_have_the_same_json_name() {
        // given:
        let input: DeriveInput = parse_quote! {
            enum Genre {
                #[protobufify(number = 1, name = "GENRE_A_B")] Ab,
                #[protobufify(number = 2, name = "GENRE_AB")] AB,
            }
        };

        // when:
        let model = analyze(&input);

        // then:
        assert!(model.is_ok());
    }

    #[test]
    fn analyze_returns_error_when_identifier_is_not_a_proto_name() {
        // given:
        let input = parse_quote! {
            struct Book { #[protobufify(number = 1)] größe: u32 }
        };

        // when:
        let errors = errors(input);

        // then:
        assert_eq!(
            errors,
            [
                "protobufify: `größe` is not a proto name (an ASCII letter, then ASCII letters, digits or `_`): add `#[protobufify(name = \"…\")]`"
            ]
        );
    }

    #[test]
    fn analyze_returns_names_when_identifier_is_not_a_proto_name_but_renamed() {
        // given:
        let input = parse_quote! {
            struct Book { #[protobufify(number = 1, name = "groesse")] größe: u32 }
        };

        // when:
        let fields = fields_of(&input);

        // then:
        assert_eq!(fields, [("groesse".to_string(), 1)]);
    }

    #[test]
    fn analyze_returns_error_when_name_is_given_to_two_members() {
        // given:
        let input = parse_quote! {
            struct Book {
                #[protobufify(number = 1)] title: String,
                #[protobufify(number = 2, name = "title")] heading: String,
            }
        };

        // when:
        let errors = errors(input);

        // then:
        assert_eq!(
            errors,
            ["protobufify: `Book.title` is the name of two members"]
        );
    }

    #[test]
    fn analyze_returns_only_the_invalid_number_when_member_also_has_a_name() {
        // given:
        let input = parse_quote! {
            struct Book {
                #[protobufify(number = 0, name = "heading")] title: String,
                #[protobufify(number = 2, name = "title")] subtitle: String,
            }
        };

        // when:
        let errors = errors(input);

        // then:
        assert_eq!(
            errors,
            ["protobufify: a field number must be in 1..=536870911, except 19000..=19999"]
        );
    }

    #[test]
    fn analyze_returns_errors_of_members_when_type_name_is_invalid() {
        // given:
        let input = parse_quote! {
            struct Größe { value: u32 }
        };

        // when:
        let errors = errors(input);

        // then:
        assert_eq!(
            errors,
            [
                "protobufify: `Größe` is not a proto name (an ASCII letter, then ASCII letters, digits or `_`): add `#[protobufify(name = \"…\")]`",
                "protobufify: `Größe.value` has no number: add `#[protobufify(number = N)]`",
            ]
        );
    }

    #[test]
    fn analyze_returns_given_type_name_verbatim_else_identifier_in_upper_camel_case() {
        // given:
        let inputs: [DeriveInput; 2] = [
            parse_quote!(
                struct HTTPServer {}
            ),
            parse_quote! {
                #[protobufify(name = "HTTPServer")]
                struct HttpServer {}
            },
        ];

        // when:
        let names = inputs.map(|input| analyze(&input).unwrap_or_else(|e| panic!("{e}")).name);

        // then:
        assert_eq!(names, ["HttpServer", "HTTPServer"]);
    }

    #[test]
    fn analyze_returns_error_when_value_is_named_like_the_zero_value() {
        // given:
        let input = parse_quote! {
            enum Genre { #[protobufify(number = 1, name = "GENRE_UNSPECIFIED")] Unknown }
        };

        // when:
        let errors = errors(input);

        // then:
        assert_eq!(
            errors,
            ["protobufify: `Genre.GENRE_UNSPECIFIED` is the name of the zero value"]
        );
    }

    #[test]
    fn analyze_returns_error_when_arm_is_named_like_the_oneof() {
        // given:
        let input = parse_quote! {
            enum Shape { #[protobufify(number = 1)] Value(u32) }
        };

        // when:
        let errors = errors(input);

        // then:
        assert_eq!(
            errors,
            ["protobufify: `Shape.value` is the name of the oneof"]
        );
    }

    #[test]
    fn analyze_returns_error_when_generated_messages_have_the_same_name() {
        // given:
        let input = parse_quote! {
            enum Shape {
                #[protobufify(number = 1)] Circles(Vec<u32>),
                #[protobufify(number = 2)] CirclesList { #[protobufify(number = 1)] radius: u32 },
            }
        };

        // when:
        let errors = errors(input);

        // then:
        assert_eq!(
            errors,
            [
                "protobufify: `ShapeCirclesList` is the name of two messages generated for `Shape`: rename a field or variant with `#[protobufify(name = \"…\")]`"
            ]
        );
    }

    #[test]
    fn analyze_returns_errors_of_payload_message() {
        // given:
        let input = parse_quote! {
            enum Recommendation {
                #[protobufify(number = 1, reserved(2, "genre"))]
                ByAuthor { #[protobufify(number = 2)] author: String, #[protobufify(number = 3)] genre: u32 },
            }
        };

        // when:
        let errors = errors(input);

        // then:
        assert_eq!(
            errors,
            [
                "protobufify: 2 is a reserved number of `RecommendationByAuthor`",
                "protobufify: `RecommendationByAuthor.genre` is a reserved name",
            ]
        );
    }

    #[test]
    fn analyze_returns_errors_for_unsupported_shapes() {
        // given:
        let inputs: [DeriveInput; 4] = [
            parse_quote!(
                struct Wrapper<T> {
                    #[protobufify(number = 1)]
                    value: T,
                }
            ),
            parse_quote!(
                struct Meters(f64);
            ),
            parse_quote!(union Bits { a: u32 }),
            parse_quote!(
                enum Shape {
                    #[protobufify(number = 1)]
                    Point(u32, u32),
                }
            ),
        ];

        // when:
        let errors = inputs.map(errors);

        // then:
        assert_eq!(
            errors,
            [
                ["protobufify: generic types are not supported"],
                ["protobufify: tuple and unit structs are not supported: name the fields"],
                ["protobufify: unions are not supported"],
                ["protobufify: tuple variants need exactly one field"],
            ]
        );
    }

    #[test]
    fn analyze_returns_checks_against_explicit_and_reserved_numbers() {
        // given:
        let input: DeriveInput = parse_quote! {
            #[protobufify(numbers_from_discriminants, reserved(2))]
            enum Room { #[protobufify(number = 3)] Attic, Library = 1 }
        };

        // when:
        let model = analyze(&input).unwrap_or_else(|e| panic!("{e}"));

        // then:
        let Body::Enum { checks, .. } = model.body else {
            panic!("expected an enum");
        };
        let checks = checks.to_string();
        assert!(checks.contains("Room :: Library as i128 != 3"), "{checks}");
        assert!(checks.contains("Room :: Library as i128 != 2"), "{checks}");
    }
}
