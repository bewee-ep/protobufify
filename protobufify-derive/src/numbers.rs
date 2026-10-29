//! The proto numbers of a type's members: `#[protobufify(number = N)]`, else, for a value of an
//! enum `#[protobufify(numbers_from_discriminants)]`, its discriminant. All three derives number
//! through here, which checks the explicit numbers: each member has one, none is given twice or
//! reserved, and no member has a reserved name. Discriminants, which only the compiler knows,
//! are checked by constants emitted here.

use heck::ToShoutySnakeCase;
use proc_macro2::{Literal, Span, TokenStream};
use quote::{quote, quote_spanned};
use syn::{DataEnum, Error, Field, Fields, FieldsNamed, Ident, Variant, ext::IdentExt};

use crate::attrs::{Range, Reserved, member, reject};
use crate::names::{arm_name, enum_value_name};

/// The number of an enum value.
#[derive(Clone, Copy)]
pub(crate) enum Number {
    /// From `#[protobufify(number = N)]`.
    Explicit(u32),
    /// The variant's discriminant, `Self::<variant> as i32`.
    Discriminant,
}

/// Errors collected to be reported together.
#[derive(Default)]
struct Errors(Option<Error>);

impl Errors {
    fn push(&mut self, error: Error) {
        match &mut self.0 {
            Some(errors) => errors.combine(error),
            None => self.0 = Some(error),
        }
    }

    fn finish(self) -> syn::Result<()> {
        self.0.map_or(Ok(()), Err)
    }
}

/// A member to number.
struct Member {
    /// Its name in the schema.
    name: String,
    /// Where errors about it point.
    span: Span,
    /// `#[protobufify(number = N)]`, with the span of `N`; an error if the attribute is invalid.
    explicit: syn::Result<Option<(u32, Span)>>,
}

impl Member {
    /// The valid explicit number.
    fn explicit(&self) -> Option<u32> {
        self.explicit
            .as_ref()
            .ok()
            .copied()
            .flatten()
            .map(|(n, _)| n)
    }
}

/// The numbers of the `members` of the type `owner`: explicit, else the discriminant if
/// `discriminants`. Errors (with those already in `errors`) for an invalid attribute, a member
/// without a number, a number given twice or `reserved`, and a reserved name.
fn resolve(
    owner: &str,
    members: &[Member],
    reserved: &Reserved,
    discriminants: bool,
    mut errors: Errors,
) -> syn::Result<Vec<Number>> {
    let mut numbers = vec![];
    for (i, member) in members.iter().enumerate() {
        let key = format!("{owner}.{}", member.name);
        if reserved.names.contains(&member.name) {
            errors.push(Error::new(
                member.span,
                format!("protobufify: `{key}` is a reserved name"),
            ));
        }
        match &member.explicit {
            Err(error) => errors.push(error.clone()),
            Ok(Some((n, span))) => {
                let n = *n;
                let twice = members[..i]
                    .iter()
                    .find(|other| other.explicit() == Some(n));
                if let Some(other) = twice {
                    let message = format!(
                        "protobufify: {n} is also the number of `{owner}.{}`",
                        other.name
                    );
                    errors.push(Error::new(*span, message));
                } else if reserved.numbers.contains(&n) {
                    let message = format!("protobufify: {n} is a reserved number of `{owner}`");
                    errors.push(Error::new(*span, message));
                }
                numbers.push(Number::Explicit(n));
            }
            Ok(None) if discriminants => numbers.push(Number::Discriminant),
            Ok(None) => {
                let message =
                    format!("protobufify: `{key}` has no number: add `#[protobufify(number = N)]`");
                errors.push(Error::new(member.span, message));
            }
        }
    }
    errors.finish()?;
    Ok(numbers)
}

/// The number of a message field or oneof arm, which has no discriminant.
fn field_number(number: Number) -> u32 {
    match number {
        Number::Explicit(n) => n,
        Number::Discriminant => unreachable!("only enum values are numbered by discriminants"),
    }
}

/// The named fields of the message `owner` with their numbers, in declaration order.
pub(crate) fn field_numbers<'a>(
    owner: &str,
    fields: &'a FieldsNamed,
    reserved: &Reserved,
) -> syn::Result<Vec<(&'a Field, u32)>> {
    let members: Vec<_> = fields
        .named
        .iter()
        .map(|field| {
            let ident = field.ident.as_ref().expect("named field");
            Member {
                name: ident.unraw().to_string(),
                span: ident.span(),
                explicit: member(&field.attrs, Range::Field, false).map(|(n, _)| n),
            }
        })
        .collect();
    let numbers = resolve(owner, &members, reserved, false, Errors::default())?;
    Ok(fields
        .named
        .iter()
        .zip(numbers.into_iter().map(field_number))
        .collect())
}

/// A variant of an enum with fields, numbered as a oneof arm.
pub(crate) struct NumberedVariant<'a> {
    pub(crate) variant: &'a Variant,
    pub(crate) number: u32,
    /// For a struct variant, the reserved numbers and names of its payload message.
    pub(crate) payload_reserved: Reserved,
}

/// The variants of the enum with fields `owner` with the numbers of their oneof arms, in
/// declaration order.
pub(crate) fn arm_numbers<'a>(
    owner: &str,
    data: &'a DataEnum,
    reserved: &Reserved,
) -> syn::Result<Vec<NumberedVariant<'a>>> {
    let mut errors = Errors::default();
    let mut members = vec![];
    let mut payloads_reserved = vec![];
    for variant in &data.variants {
        if let Fields::Unnamed(f) = &variant.fields {
            for field in &f.unnamed {
                if let Err(e) = reject(&field.attrs) {
                    errors.push(e);
                }
            }
        }
        let payload = matches!(variant.fields, Fields::Named(_));
        let (explicit, payload_reserved) = match member(&variant.attrs, Range::Field, payload) {
            Ok((explicit, reserved)) => (Ok(explicit), reserved),
            Err(e) => (Err(e), Reserved::default()),
        };
        members.push(Member {
            name: arm_name(&variant.ident),
            span: variant.ident.span(),
            explicit,
        });
        payloads_reserved.push(payload_reserved);
    }
    let numbers = resolve(owner, &members, reserved, false, errors)?;
    Ok(data
        .variants
        .iter()
        .zip(numbers)
        .zip(payloads_reserved)
        .map(|((variant, number), payload_reserved)| NumberedVariant {
            variant,
            number: field_number(number),
            payload_reserved,
        })
        .collect())
}

/// The values of an enum without fields with their numbers, in declaration order.
pub(crate) struct ValueNumbers<'a> {
    pub(crate) values: Vec<(&'a Variant, Number)>,
    /// The constants checking the discriminants among the numbers, none without any. Every
    /// derive emits them, so that each checks the numbers it uses.
    pub(crate) checks: TokenStream,
}

/// The values of the enum without fields `ident` with their numbers: explicit, else the
/// discriminant if `from_discriminants`.
pub(crate) fn value_numbers<'a>(
    ident: &Ident,
    data: &'a DataEnum,
    from_discriminants: bool,
    reserved: &Reserved,
) -> syn::Result<ValueNumbers<'a>> {
    let owner = ident.to_string();
    let members: Vec<_> = data
        .variants
        .iter()
        .map(|variant| Member {
            name: enum_value_name(&owner, &variant.ident),
            span: variant.ident.span(),
            explicit: member(&variant.attrs, Range::EnumValue, false).map(|(n, _)| n),
        })
        .collect();
    let numbers = resolve(
        &owner,
        &members,
        reserved,
        from_discriminants,
        Errors::default(),
    )?;
    let checks = discriminant_checks(ident, data, &members, &numbers, reserved);
    Ok(ValueNumbers {
        values: data.variants.iter().zip(numbers).collect(),
        checks,
    })
}

/// Constants asserting that each value of the enum `ident` numbered by its discriminant has a
/// valid number, which is neither another member's nor `reserved`.
fn discriminant_checks(
    ident: &Ident,
    data: &DataEnum,
    members: &[Member],
    numbers: &[Number],
    reserved: &Reserved,
) -> TokenStream {
    let owner = ident.to_string();
    let unspecified = format!("{}_UNSPECIFIED", owner.to_shouty_snake_case());
    let mut checks = vec![];
    for (variant, number) in data.variants.iter().zip(numbers) {
        if !matches!(number, Number::Discriminant) {
            continue;
        }
        let v = &variant.ident;
        // One constant per assertion, so that each failing one is reported. Spanned at the
        // variant, so that rustc reports it once rather than once per derive.
        let mut check = |condition: TokenStream, message: String| {
            checks.push(quote_spanned!(v.span()=> const _: () = assert!(#condition, #message);));
        };
        // `i128` holds every discriminant but those of a `u128` beyond it, which it makes
        // negative, so out of range too.
        let value = quote!(#ident::#v as i128);
        let of = format!("protobufify: the discriminant of {owner}::{v}");
        check(
            quote!(#value >= 1 && #value <= i32::MAX as i128),
            format!("{of} must be in 1..=2147483647, 0 is the {unspecified} value"),
        );
        for other in members {
            if let Some(n) = other.explicit() {
                let n = Literal::u32_unsuffixed(n);
                let message = format!("{of} is also the number of `{owner}.{}`", other.name);
                check(quote!(#value != #n), message);
            }
        }
        for n in &reserved.numbers {
            let message = format!("{of} is a reserved number of `{owner}`");
            let n = Literal::u32_unsuffixed(*n);
            check(quote!(#value != #n), message);
        }
    }
    quote!(#(#checks)*)
}

#[cfg(test)]
mod tests {
    use super::*;
    use syn::{DeriveInput, parse_quote};

    use crate::attrs::container;

    /// The numbers of the fields of `input`, a struct, or the error messages.
    fn numbers(input: DeriveInput) -> Result<Vec<u32>, Vec<String>> {
        let syn::Data::Struct(syn::DataStruct {
            fields: Fields::Named(fields),
            ..
        }) = &input.data
        else {
            panic!("expected a struct with named fields");
        };
        let container = container(&input).map_err(|e| vec![e.to_string()])?;
        field_numbers(&input.ident.to_string(), fields, &container.reserved)
            .map(|fields| fields.into_iter().map(|(_, n)| n).collect())
            .map_err(|e| e.into_iter().map(|e| e.to_string()).collect())
    }

    #[test]
    fn field_numbers_returns_explicit_numbers() {
        // given:
        let input = parse_quote! {
            struct Book { #[protobufify(number = 5)] title: String, #[protobufify(number = 2)] pages: u32 }
        };

        // when:
        let numbers = numbers(input);

        // then:
        assert_eq!(numbers, Ok(vec![5, 2]));
    }

    #[test]
    fn field_numbers_returns_errors_for_all_members_without_number() {
        // given:
        let input = parse_quote!(
            struct Book {
                title: String,
                pages: u32,
            }
        );

        // when:
        let errors = numbers(input).unwrap_err();

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
    fn field_numbers_returns_errors_for_all_invalid_numbers() {
        // given:
        let input = parse_quote! {
            struct Book {
                #[protobufify(number = 0)] title: String,
                #[protobufify(number = 19000)] pages: u32,
            }
        };

        // when:
        let errors = numbers(input).unwrap_err();

        // then:
        assert_eq!(errors.len(), 2);
        assert!(errors[0].contains("1..=536870911"), "{errors:?}");
    }

    #[test]
    fn field_numbers_returns_error_when_number_is_given_twice() {
        // given:
        let input = parse_quote! {
            struct Book { #[protobufify(number = 3)] title: String, #[protobufify(number = 3)] pages: u32 }
        };

        // when:
        let errors = numbers(input).unwrap_err();

        // then:
        assert_eq!(
            errors,
            ["protobufify: 3 is also the number of `Book.title`"]
        );
    }

    #[test]
    fn field_numbers_returns_errors_when_number_or_name_is_reserved() {
        // given:
        let input = parse_quote! {
            #[protobufify(reserved(2, 4), reserved_names("isbn"))]
            struct Book { #[protobufify(number = 4)] title: String, #[protobufify(number = 5)] isbn: u64 }
        };

        // when:
        let errors = numbers(input).unwrap_err();

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
    fn value_numbers_returns_checks_against_explicit_and_reserved_numbers() {
        // given:
        let input: DeriveInput = parse_quote! {
            #[protobufify(numbers_from_discriminants, reserved(2))]
            enum Room { #[protobufify(number = 3)] Attic, Library = 1 }
        };
        let syn::Data::Enum(data) = &input.data else {
            unreachable!()
        };
        let container = container(&input).unwrap();

        // when:
        let numbers = value_numbers(&input.ident, data, true, &container.reserved).unwrap();

        // then:
        let checks = numbers.checks.to_string();
        assert!(checks.contains("Room :: Library as i128 != 3"), "{checks}");
        assert!(checks.contains("Room :: Library as i128 != 2"), "{checks}");
    }
}
