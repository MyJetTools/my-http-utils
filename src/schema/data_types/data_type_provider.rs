use std::collections::{BTreeMap, HashMap};

use rust_extensions::date_time::DateTimeAsMicroseconds;

use super::{ArrayElement, HttpDataType, HttpObjectStructure, HttpSimpleType, SchemaFieldCtx};

pub trait DataTypeProvider {
    fn get_data_type() -> HttpDataType;

    /// [`Self::get_data_type`], told which field of which struct is being described.
    ///
    /// This is what the derives call, and what container providers (`Vec`, `HashMap`, `BTreeMap`)
    /// pass down to their element type, so that a type with no schema representation can name the
    /// declaring field instead of just printing the element type it choked on. See
    /// [`SchemaFieldCtx`] for why that matters — the schema is built at controller registration,
    /// so a panic here is a start-up crash-loop, not a failed request.
    ///
    /// Defaulted, so every existing implementation keeps working: a leaf type's schema does not
    /// depend on where it is used, and only the containers need to override this.
    fn get_data_type_in_field(_ctx: SchemaFieldCtx) -> HttpDataType {
        Self::get_data_type()
    }

    fn get_http_data_structure() -> HttpObjectStructure {
        panic!("Type does not provide HttpObjectStructure")
    }

    fn get_generic_type() -> Option<String> {
        None
    }
}

impl DataTypeProvider for u8 {
    fn get_data_type() -> HttpDataType {
        HttpDataType::SimpleType(HttpSimpleType::Byte)
    }
}

impl DataTypeProvider for i8 {
    fn get_data_type() -> HttpDataType {
        HttpDataType::SimpleType(HttpSimpleType::Byte)
    }
}

impl DataTypeProvider for u16 {
    fn get_data_type() -> HttpDataType {
        HttpDataType::SimpleType(HttpSimpleType::Integer)
    }
}

impl DataTypeProvider for i16 {
    fn get_data_type() -> HttpDataType {
        HttpDataType::SimpleType(HttpSimpleType::Integer)
    }
}

impl DataTypeProvider for u32 {
    fn get_data_type() -> HttpDataType {
        HttpDataType::SimpleType(HttpSimpleType::Integer)
    }
}

impl DataTypeProvider for i32 {
    fn get_data_type() -> HttpDataType {
        HttpDataType::SimpleType(HttpSimpleType::Integer)
    }
}

impl DataTypeProvider for u64 {
    fn get_data_type() -> HttpDataType {
        HttpDataType::SimpleType(HttpSimpleType::Long)
    }
}

impl DataTypeProvider for i64 {
    fn get_data_type() -> HttpDataType {
        HttpDataType::SimpleType(HttpSimpleType::Long)
    }
}

impl DataTypeProvider for usize {
    fn get_data_type() -> HttpDataType {
        HttpDataType::SimpleType(HttpSimpleType::Long)
    }
}

impl DataTypeProvider for isize {
    fn get_data_type() -> HttpDataType {
        HttpDataType::SimpleType(HttpSimpleType::Long)
    }
}

impl DataTypeProvider for f32 {
    fn get_data_type() -> HttpDataType {
        HttpDataType::SimpleType(HttpSimpleType::Double)
    }
}

impl DataTypeProvider for f64 {
    fn get_data_type() -> HttpDataType {
        HttpDataType::SimpleType(HttpSimpleType::Double)
    }
}

impl DataTypeProvider for bool {
    fn get_data_type() -> HttpDataType {
        HttpDataType::SimpleType(HttpSimpleType::Boolean)
    }
}

impl DataTypeProvider for String {
    fn get_data_type() -> HttpDataType {
        HttpDataType::SimpleType(HttpSimpleType::String)
    }
}

impl DataTypeProvider for &str {
    fn get_data_type() -> HttpDataType {
        HttpDataType::SimpleType(HttpSimpleType::String)
    }
}

impl DataTypeProvider for DateTimeAsMicroseconds {
    fn get_data_type() -> HttpDataType {
        HttpDataType::SimpleType(HttpSimpleType::DateTime)
    }
}

impl<T: DataTypeProvider> DataTypeProvider for Vec<T> {
    fn get_data_type() -> HttpDataType {
        Self::get_data_type_in_field(SchemaFieldCtx::unknown())
    }

    fn get_data_type_in_field(ctx: SchemaFieldCtx) -> HttpDataType {
        // The context goes down to `T`, not just into the panic below: a `Vec<Vec<HashMap<..>>>`
        // fails two levels in, and the field that declared it is the only useful thing to print.
        let data_type = T::get_data_type_in_field(ctx);
        match data_type {
            HttpDataType::SimpleType(tp) => HttpDataType::ArrayOf(ArrayElement::SimpleType(tp)),
            HttpDataType::Object(obj) => HttpDataType::ArrayOf(ArrayElement::Object(obj)),
            HttpDataType::Enum(item) => HttpDataType::ArrayOf(ArrayElement::Enum(item)),
            // `Vec<Vec<T>>`: the element is itself an array, so it nests as one more array level.
            // Recursive by construction, so `Vec<Vec<Vec<T>>>` and deeper come out of the same arm.
            HttpDataType::ArrayOf(inner) => HttpDataType::ArrayOf(inner.into_array_of()),
            // Left: `DictionaryOf` / `DictionaryOfArray` (a `Vec<HashMap<..>>`) and `None`.
            // `ArrayElement` has no dictionary variant, so these genuinely have no representation
            // — but the message has to be actionable on its own, see `SchemaFieldCtx`.
            data_type => panic!(
                "Field `{}` of type `{}` can not be described in the HTTP schema: an array element \
                 can only be a simple type, an object, an enum or another array, but this one \
                 resolved to {:?}",
                ctx,
                std::any::type_name::<Self>(),
                data_type
            ),
        }
    }

    fn get_generic_type() -> Option<String> {
        T::get_generic_type().map(|generic_type| format!("array_of_{}", generic_type.as_str()))
    }

    fn get_http_data_structure() -> HttpObjectStructure {
        T::get_http_data_structure()
    }
}

impl<TValue: DataTypeProvider> DataTypeProvider for HashMap<String, TValue> {
    fn get_data_type() -> HttpDataType {
        Self::get_data_type_in_field(SchemaFieldCtx::unknown())
    }

    fn get_data_type_in_field(ctx: SchemaFieldCtx) -> HttpDataType {
        dictionary_data_type::<Self, TValue>(ctx)
    }
}

impl<TValue: DataTypeProvider> DataTypeProvider for BTreeMap<String, TValue> {
    fn get_data_type() -> HttpDataType {
        Self::get_data_type_in_field(SchemaFieldCtx::unknown())
    }

    fn get_data_type_in_field(ctx: SchemaFieldCtx) -> HttpDataType {
        dictionary_data_type::<Self, TValue>(ctx)
    }
}

/// The shared body of the `HashMap` / `BTreeMap` providers — the two describe the same wire shape
/// (a JSON object keyed by string), so they must not be able to drift apart.
///
/// `TSelf` is only there to name the whole map type in the panic; `TValue` is what is described.
fn dictionary_data_type<TSelf: ?Sized, TValue: DataTypeProvider>(
    ctx: SchemaFieldCtx,
) -> HttpDataType {
    let data_type = TValue::get_data_type_in_field(ctx);
    match data_type {
        HttpDataType::SimpleType(tp) => HttpDataType::DictionaryOf(ArrayElement::SimpleType(tp)),
        HttpDataType::Object(obj) => HttpDataType::DictionaryOf(ArrayElement::Object(obj)),
        HttpDataType::Enum(item) => HttpDataType::DictionaryOf(ArrayElement::Enum(item)),
        // A dictionary of arrays. The element carries its own nesting, so a
        // `HashMap<String, Vec<Vec<T>>>` arrives here as `ArrayElement::ArrayOf(..)` and renders
        // as a dictionary of an array of arrays — the same recursion as in the type.
        HttpDataType::ArrayOf(item) => HttpDataType::DictionaryOfArray(item),
        // Left: a dictionary of dictionaries, and `None`. `ArrayElement` has no dictionary
        // variant, so there is nothing to put in one.
        data_type => panic!(
            "Field `{}` of type `{}` can not be described in the HTTP schema: a dictionary value \
             can only be a simple type, an object, an enum or an array, but this one resolved to \
             {:?}",
            ctx,
            std::any::type_name::<TSelf>(),
            data_type
        ),
    }
}
