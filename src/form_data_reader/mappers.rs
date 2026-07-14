use std::collections::HashMap;

use rust_extensions::date_time::DateTimeAsMicroseconds;
use serde::de::DeserializeOwned;

use crate::form_data_reader::FormDataItem;

use super::ReadingFromDataError;

impl<'s> TryInto<String> for &'s FormDataItem<'s> {
    type Error = ReadingFromDataError;
    fn try_into(self) -> Result<String, Self::Error> {
        match self {
            FormDataItem::ValueAsString { value, .. } => Ok(value.to_string()),
            FormDataItem::File { name, .. } => Err(file_conversion_error(name, "String")),
        }
    }
}

impl<'s> TryInto<&'s str> for &'s FormDataItem<'s> {
    type Error = ReadingFromDataError;
    fn try_into(self) -> Result<&'s str, Self::Error> {
        match self {
            FormDataItem::ValueAsString { value, .. } => Ok(*value),
            FormDataItem::File { name, .. } => Err(file_conversion_error(name, "str")),
        }
    }
}

impl<'s> TryInto<bool> for &'s FormDataItem<'s> {
    type Error = ReadingFromDataError;
    fn try_into(self) -> Result<bool, Self::Error> {
        match self {
            FormDataItem::ValueAsString { value, name } => to_bool(name, value),
            FormDataItem::File { name, .. } => Err(file_conversion_error(name, "bool")),
        }
    }
}

/// Generates the near-identical `TryInto<$t>` impls for the primitive numeric types.
macro_rules! impl_try_into_simple {
    ($($t:ty),+ $(,)?) => {
        $(
            impl<'s> TryInto<$t> for &'s FormDataItem<'s> {
                type Error = ReadingFromDataError;
                fn try_into(self) -> Result<$t, Self::Error> {
                    match self {
                        FormDataItem::ValueAsString { value, name } => {
                            to_simple_value(name, value)
                        }
                        FormDataItem::File { name, .. } => {
                            Err(file_conversion_error(name, stringify!($t)))
                        }
                    }
                }
            }
        )+
    };
}

impl_try_into_simple!(u8, i8, u16, i16, u32, i32, u64, i64, f32, f64, usize, isize);

impl<'s> TryInto<DateTimeAsMicroseconds> for &'s FormDataItem<'s> {
    type Error = ReadingFromDataError;
    fn try_into(self) -> Result<DateTimeAsMicroseconds, Self::Error> {
        match self {
            FormDataItem::ValueAsString { value, name } => to_date_time(name, value),
            FormDataItem::File { name, .. } => {
                Err(file_conversion_error(name, "DateTimeAsMicroseconds"))
            }
        }
    }
}

impl<'s, TValue> TryInto<HashMap<String, TValue>> for &'s FormDataItem<'s>
where
    TValue: DeserializeOwned,
{
    type Error = ReadingFromDataError;

    fn try_into(self) -> Result<HashMap<String, TValue>, Self::Error> {
        match self {
            FormDataItem::ValueAsString { value, name } => to_json(name, value),
            FormDataItem::File { name, content, .. } => to_json_from_slice(name, content),
        }
    }
}

// NOTE: this blanket impl treats the file/field bytes as JSON. In particular a
// `Vec<u8>` request deserializes the content as a JSON array of numbers rather than
// returning the raw bytes — callers that need raw bytes should read the
// `FormDataItem::File { content, .. }` slice directly.
impl<'s, TValue> TryInto<Vec<TValue>> for &'s FormDataItem<'s>
where
    TValue: DeserializeOwned,
{
    type Error = ReadingFromDataError;

    fn try_into(self) -> Result<Vec<TValue>, Self::Error> {
        match self {
            FormDataItem::ValueAsString { value, name } => to_json(name, value),
            FormDataItem::File { name, content, .. } => to_json_from_slice(name, content),
        }
    }
}

fn file_conversion_error(field: &str, target: &str) -> ReadingFromDataError {
    ReadingFromDataError::ValidationError {
        field: field.to_string(),
        error: format!("Field contains a File which is not possible to convert to {target}"),
    }
}

fn to_bool(param_name: &str, value: &str) -> Result<bool, ReadingFromDataError> {
    if value == "1" || value.eq_ignore_ascii_case("true") {
        return Ok(true);
    }

    if value == "0" || value.eq_ignore_ascii_case("false") {
        return Ok(false);
    }

    Err(ReadingFromDataError::ValidationError {
        field: param_name.to_string(),
        error: "Can not convert value to boolean".into(),
    })
}

fn to_simple_value<T: std::str::FromStr>(
    param_name: &str,
    value: &str,
) -> Result<T, ReadingFromDataError> {
    if let Ok(result) = value.parse() {
        return Ok(result);
    }

    Err(ReadingFromDataError::ValidationError {
        field: param_name.to_string(),
        error: "Can not convert value to simple value".into(),
    })
}

fn to_date_time(
    param_name: &str,
    value: &str,
) -> Result<DateTimeAsMicroseconds, ReadingFromDataError> {
    if let Some(result) = DateTimeAsMicroseconds::from_str(value) {
        return Ok(result);
    }

    Err(ReadingFromDataError::ValidationError {
        field: param_name.to_string(),
        error: "Can not convert value to DateTime".into(),
    })
}

fn to_json<TResult: DeserializeOwned>(
    param_name: &str,
    value: &str,
) -> Result<TResult, ReadingFromDataError> {
    serde_json::from_str(value).map_err(|err| ReadingFromDataError::ValidationError {
        field: param_name.to_string(),
        error: format!("Can not deserialize from json. Err: {err:?}"),
    })
}

fn to_json_from_slice<TResult: DeserializeOwned>(
    param_name: &str,
    value: &[u8],
) -> Result<TResult, ReadingFromDataError> {
    serde_json::from_slice(value).map_err(|err| ReadingFromDataError::ValidationError {
        field: param_name.to_string(),
        error: format!("Can not deserialize from json. Err: {err:?}"),
    })
}

#[cfg(test)]
mod tests {
    use crate::form_data_reader::FormDataItem;

    fn value(name: &'static str, value: &'static str) -> FormDataItem<'static> {
        FormDataItem::ValueAsString { name, value }
    }

    fn file() -> FormDataItem<'static> {
        FormDataItem::File {
            name: "f",
            file_name: "a.txt",
            content_type: "text/plain",
            content: b"[1,2,3]",
        }
    }

    #[test]
    fn test_simple_conversions() {
        let v = value("n", "42");
        let as_u32: u32 = (&v).try_into().unwrap();
        assert_eq!(as_u32, 42);
        let as_i64: i64 = (&v).try_into().unwrap();
        assert_eq!(as_i64, 42);
    }

    #[test]
    fn test_bool_conversion() {
        let t: bool = (&value("n", "true")).try_into().unwrap();
        let f: bool = (&value("n", "0")).try_into().unwrap();
        assert!(t);
        assert!(!f);
    }

    #[test]
    fn test_file_to_number_error_message_is_typed() {
        let f = file();
        let err: Result<u32, _> = (&f).try_into();
        match err.unwrap_err() {
            crate::form_data_reader::ReadingFromDataError::ValidationError { error, .. } => {
                assert!(error.contains("convert to u32"), "got: {error}");
            }
            _ => panic!("unexpected error"),
        }
    }

    #[test]
    fn test_vec_from_json() {
        let v = value("n", "[1,2,3]");
        let parsed: Vec<i32> = (&v).try_into().unwrap();
        assert_eq!(parsed, vec![1, 2, 3]);
    }
}
