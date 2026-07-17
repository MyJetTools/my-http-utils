//! Reading serde's key-renaming attributes off a model, so the client's `JsonValueWriter` writes
//! the **same** key names the server's serde `Deserialize` expects.
//!
//! Why this exists: an object structure is written into a request body by the derive-generated
//! `JsonValueWriter` (my-json, no serde), but read back out of the body by serde
//! (`HttpInputValue::deserialize_json`). The two only agree if this crate reproduces serde's key
//! naming exactly — so the rules below mirror `serde_derive::internals::case::RenameRule`
//! **as applied to struct fields**, and are covered by `rename_all_matches_serde` in the `tests`
//! crate, which asserts the generated keys byte-for-byte against `serde_json`'s own output.
//!
//! Note on the idiom: this uses plain `syn` rather than `types_reader`. `types_reader::Attributes`
//! is not reachable from outside types-reader (its module is private; only `MacrosAttribute` is
//! re-exported), so it has no usable constructor for a container. More importantly, routing
//! `#[serde(..)]` through `types_reader::TokensObject` **silently drops** the
//! `rename_all(serialize = .., deserialize = ..)` form — which would reintroduce exactly the
//! silent key mismatch this module exists to prevent.

/// serde's `rename_all` rules, mirroring `serde_derive::internals::case::RenameRule`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RenameAllRule {
    Lower,
    Upper,
    Pascal,
    Camel,
    Snake,
    ScreamingSnake,
    Kebab,
    ScreamingKebab,
}

impl RenameAllRule {
    fn from_literal(value: &str, spanned: &dyn quote::ToTokens) -> Result<Self, syn::Error> {
        match value {
            "lowercase" => Ok(Self::Lower),
            "UPPERCASE" => Ok(Self::Upper),
            "PascalCase" => Ok(Self::Pascal),
            "camelCase" => Ok(Self::Camel),
            "snake_case" => Ok(Self::Snake),
            "SCREAMING_SNAKE_CASE" => Ok(Self::ScreamingSnake),
            "kebab-case" => Ok(Self::Kebab),
            "SCREAMING-KEBAB-CASE" => Ok(Self::ScreamingKebab),
            other => Err(syn::Error::new_spanned(
                spanned,
                format!(
                    "Unknown serde rename rule `rename_all = \"{}\"`, expected one of \"lowercase\", \
                     \"UPPERCASE\", \"PascalCase\", \"camelCase\", \"snake_case\", \
                     \"SCREAMING_SNAKE_CASE\", \"kebab-case\", \"SCREAMING-KEBAB-CASE\"",
                    other
                ),
            )),
        }
    }

    /// Applies the rule to a Rust field name, byte-for-byte as serde does.
    ///
    /// `field` must already have any raw-identifier `r#` prefix stripped (serde strips it before
    /// applying a rule, so `r#type` renames as `type`).
    ///
    /// Beware the two identities: for **fields** (unlike enum variants) serde treats `lowercase`
    /// and `snake_case` as no-ops — it assumes a field name is already snake_case and does not
    /// case-fold it. `field.to_lowercase()` here would silently corrupt the wire format of any
    /// field whose name is not already lowercase.
    pub fn apply_to_field(&self, field: &str) -> String {
        match self {
            Self::Lower | Self::Snake => field.to_string(),
            Self::Upper | Self::ScreamingSnake => field.to_ascii_uppercase(),
            Self::Pascal => {
                let mut result = String::with_capacity(field.len());
                let mut capitalize = true;
                for ch in field.chars() {
                    if ch == '_' {
                        capitalize = true;
                    } else if capitalize {
                        result.push(ch.to_ascii_uppercase());
                        capitalize = false;
                    } else {
                        result.push(ch);
                    }
                }
                result
            }
            Self::Camel => {
                let pascal = Self::Pascal.apply_to_field(field);
                match pascal.chars().next() {
                    // serde does exactly `pascal[..1].to_ascii_lowercase() + &pascal[1..]`.
                    Some(first) if first.is_ascii() => {
                        pascal[..1].to_ascii_lowercase() + &pascal[1..]
                    }
                    // An empty or non-ASCII-leading result makes serde's own byte-slicing panic,
                    // so serde_derive fails the build first either way. Don't panic here too —
                    // a panicking proc macro buries the real error.
                    _ => pascal,
                }
            }
            Self::Kebab => field.replace('_', "-"),
            Self::ScreamingKebab => field.to_ascii_uppercase().replace('_', "-"),
        }
    }
}

/// Consumes whatever a serde param we don't care about carries, so parsing continues at the next
/// one. Three shapes have to be handled: a bare flag (`deny_unknown_fields`), `name = value`
/// (`crate = ".."`), and the **list form** `name(..)` (`bound(deserialize = "..")`, or a
/// container-level `rename(serialize = .., deserialize = ..)`, which renames the struct and has
/// nothing to do with field keys).
///
/// Raw token trees rather than a typed parse: a parenthesised group is a single `TokenTree`, so
/// this swallows any value shape serde accepts without having to enumerate them.
fn skip_param_value(meta: &syn::meta::ParseNestedMeta) -> Result<(), syn::Error> {
    while !meta.input.is_empty() && !meta.input.peek(syn::Token![,]) {
        meta.input.parse::<proc_macro2::TokenTree>()?;
    }

    Ok(())
}

/// Container-level serde params that reshape the object, rejected for the same reason as the
/// field-level ones above.
///
/// `deny_unknown_fields`, `bound`, `crate`, `expecting`, `default` and a container `rename` are
/// deliberately absent: none of them changes what this path writes or reads.
const WIRE_SHAPING_SERDE_CONTAINER_PARAMS: &[(&str, &str)] = &[
    (
        "transparent",
        "the object would still be written with its key, not as the bare inner value",
    ),
    ("tag", "the tag would not be written"),
    ("untagged", "the object would still be written with its keys"),
    ("from", "the conversion would not be applied"),
    ("into", "the conversion would not be applied"),
    ("try_from", "the conversion would not be applied"),
];

/// Rejects the container params above.
fn reject_wire_shaping_serde_container_params(ast: &syn::DeriveInput) -> Result<(), syn::Error> {
    for attr in &ast.attrs {
        if !attr.path().is_ident("serde") || !matches!(attr.meta, syn::Meta::List(_)) {
            continue;
        }

        attr.parse_nested_meta(|meta| {
            for (param, effect) in WIRE_SHAPING_SERDE_CONTAINER_PARAMS {
                if meta.path.is_ident(param) {
                    return Err(meta.error(format!(
                        "`#[serde({})]` is not supported on this derive: a `#[http_body]` object is \
                         written and read by the derive itself (my-json on both sides) and never \
                         goes through serde, so {}. Carry the payload as `#[http_body_raw] \
                         RawDataTyped<T>` if you need the full serde semantics — that is serde on \
                         both sides.",
                        param, effect
                    )));
                }
            }

            skip_param_value(&meta)
        })?;
    }

    Ok(())
}

/// Reads the container-level `#[serde(rename_all = "...")]` off a `syn::DeriveInput`.
///
/// Every other container attribute is tolerated and ignored: doc comments, `#[derive]`,
/// `#[allow]`, `#[non_exhaustive]`, several separate `#[serde(..)]` attributes, and any other
/// serde parameter (`deny_unknown_fields`, `default`, `tag = ".."`, …).
///
/// The split form `rename_all(serialize = .., deserialize = ..)` is a hard error: it gives the
/// serialize and deserialize sides different key names, and the client writer has only one name to
/// write — so there is no key that satisfies both halves of the contract.
pub fn read_container_rename_all(
    ast: &syn::DeriveInput,
) -> Result<Option<RenameAllRule>, syn::Error> {
    // Both object derives call this exactly once per struct, so it is the one place that sees every
    // container attribute — the counterpart of the per-field check in `resolve_field_key`.
    reject_wire_shaping_serde_container_params(ast)?;

    let mut result = None;

    for attr in &ast.attrs {
        if !attr.path().is_ident("serde") {
            continue;
        }

        // `#[serde]` / `#[serde = ..]` is never valid serde — nothing to walk into.
        if !matches!(attr.meta, syn::Meta::List(_)) {
            continue;
        }

        attr.parse_nested_meta(|meta| {
            if !meta.path.is_ident("rename_all") {
                return skip_param_value(&meta);
            }

            // `rename_all(serialize = .., deserialize = ..)`.
            if meta.input.peek(syn::token::Paren) {
                return Err(meta.error(
                    "`#[serde(rename_all(serialize = .., deserialize = ..))]` is not supported \
                     here: it gives the serialize and deserialize sides different key names, but \
                     this model is written into a request by the client and read back with serde \
                     on the server, so one key must satisfy both. Use a single \
                     `#[serde(rename_all = \"..\")]`.",
                ));
            }

            let literal: syn::LitStr = meta.value()?.parse()?;
            result = Some(RenameAllRule::from_literal(&literal.value(), &literal)?);
            Ok(())
        })?;
    }

    Ok(result)
}

/// Reads a field-level `#[serde(rename = "...")]`.
///
/// The split form `rename(serialize = .., deserialize = ..)` is a hard error, for the same reason
/// as the container's. It is worth being explicit that this used to be *silently ignored*: the
/// previous `if let Ok(..) = get_named_param("serde", "rename")` swallowed the parse failure and
/// fell back to the Rust field name, so the client wrote a key serde would never look for.
pub fn read_field_rename(field: &syn::Field) -> Result<Option<String>, syn::Error> {
    let mut result = None;

    for attr in &field.attrs {
        if !attr.path().is_ident("serde") {
            continue;
        }

        if !matches!(attr.meta, syn::Meta::List(_)) {
            continue;
        }

        attr.parse_nested_meta(|meta| {
            if !meta.path.is_ident("rename") {
                return skip_param_value(&meta);
            }

            if meta.input.peek(syn::token::Paren) {
                return Err(meta.error(
                    "`#[serde(rename(serialize = .., deserialize = ..))]` is not supported here: \
                     it gives the serialize and deserialize sides different key names, but this \
                     model is written into a request by the client and read back with serde on the \
                     server, so one key must satisfy both. Use a single `#[serde(rename = \"..\")]`.",
                ));
            }

            let literal: syn::LitStr = meta.value()?.parse()?;
            result = Some(literal.value());
            Ok(())
        })?;
    }

    Ok(result)
}

/// serde params that change an object's wire **shape**, which this path cannot honour.
///
/// A `#[http_body]` object is written and read by the derive (my-json on both sides); serde is
/// never called, so these have no effect here — and silently having no effect is the worst outcome
/// of the three. `#[serde(skip_serializing)]` is the sharpest: it is the idiomatic way to keep a
/// field off the wire, and ignoring it *sends* the field the author marked secret. The rest lose or
/// mangle data just as quietly.
///
/// Naming (`rename`, `rename_all`) is not in this list — that is honoured, and is the whole point
/// of this module.
const WIRE_SHAPING_SERDE_PARAMS: &[(&str, &str)] = &[
    ("skip", "the field would still be written, and read back, by this path"),
    (
        "skip_serializing",
        "the field would still be written — a field marked never-to-be-sent would go on the wire",
    ),
    ("skip_deserializing", "the field would still be read by this path"),
    ("skip_serializing_if", "the key would be written even when the predicate says to omit it"),
    ("flatten", "the object would be nested, not inlined"),
    ("with", "the codec would not be called"),
    ("serialize_with", "the codec would not be called"),
    ("deserialize_with", "the codec would not be called"),
];

/// Rejects the params above on a field, with a message that names the escape hatch.
pub fn reject_wire_shaping_serde_params(field: &syn::Field) -> Result<(), syn::Error> {
    for attr in &field.attrs {
        if !attr.path().is_ident("serde") || !matches!(attr.meta, syn::Meta::List(_)) {
            continue;
        }

        attr.parse_nested_meta(|meta| {
            for (param, effect) in WIRE_SHAPING_SERDE_PARAMS {
                if meta.path.is_ident(param) {
                    return Err(meta.error(format!(
                        "`#[serde({})]` is not supported on this derive: a `#[http_body]` object is \
                         written and read by the derive itself (my-json on both sides) and never \
                         goes through serde, so {}. Carry the payload as `#[http_body_raw] \
                         RawDataTyped<T>` if you need the full serde semantics — that is serde on \
                         both sides.",
                        param, effect
                    )));
                }
            }

            skip_param_value(&meta)
        })?;
    }

    Ok(())
}

/// Reads this crate's own `#[json_name("...")]` — the native way to name a field on the wire.
///
/// It exists because a `#[http_body]` object is no longer written or read with serde at all: the
/// derive owns both halves. Borrowing serde's attribute to name a key therefore made a model carry
/// `#[derive(Serialize, Deserialize)]` purely so the attribute would be *registered*, for a crate
/// that never calls serde. `#[serde(rename)]` keeps working — a model that does use serde
/// elsewhere (say inside a `RawDataTyped<T>` payload) should not have to say the name twice.
pub fn read_field_json_name(field: &syn::Field) -> Result<Option<String>, syn::Error> {
    let mut result = None;

    for attr in &field.attrs {
        if !attr.path().is_ident("json_name") {
            continue;
        }

        let literal: syn::LitStr = attr.parse_args().map_err(|_| {
            syn::Error::new_spanned(
                attr,
                "`json_name` takes exactly one string: `#[json_name(\"cardNumber\")]`",
            )
        })?;

        result = Some(literal.value());
    }

    Ok(result)
}

/// The wire key for a field:
/// `#[json_name]` > `#[serde(rename)]` > container `#[serde(rename_all)]` > the Rust field name.
///
/// `json_name` and `serde(rename)` disagreeing is a hard error rather than a silent pick: serde is
/// still what reads a model carried inside a `RawDataTyped<T>`, so two different names would mean
/// two different wire formats for one field depending on how it travels.
pub fn resolve_field_key(
    field: &syn::Field,
    field_name: &str,
    rename_all: Option<RenameAllRule>,
) -> Result<String, syn::Error> {
    // Every field flows through here on its way to both the writer and the reader, so this is the
    // one place that sees them all.
    reject_wire_shaping_serde_params(field)?;

    let json_name = read_field_json_name(field)?;
    let serde_rename = read_field_rename(field)?;

    if let (Some(json_name), Some(serde_rename)) = (&json_name, &serde_rename) {
        if json_name != serde_rename {
            return Err(syn::Error::new_spanned(
                field,
                format!(
                    "`#[json_name(\"{}\")]` and `#[serde(rename = \"{}\")]` name the same field \
                     differently. serde still reads this model if it travels inside a \
                     `RawDataTyped<T>`, so the two names would be two wire formats for one field. \
                     Keep one, or make them agree.",
                    json_name, serde_rename
                ),
            ));
        }
    }

    if let Some(json_name) = json_name {
        return Ok(json_name);
    }

    if let Some(renamed) = serde_rename {
        // An explicit rename wins outright — serde does NOT then apply rename_all to it.
        return Ok(renamed);
    }

    // serde strips a raw identifier's `r#` before applying any rule, so `r#type` is keyed `type`.
    let field_name = field_name.strip_prefix("r#").unwrap_or(field_name);

    match rename_all {
        Some(rule) => Ok(rule.apply_to_field(field_name)),
        None => Ok(field_name.to_string()),
    }
}
