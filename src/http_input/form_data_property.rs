use rust_extensions::date_time::DateTimeAsMicroseconds;

use crate::body::FormDataBody;
use crate::form_data_reader::FormDataHeader;

use super::core::data_src::SRC_FORM_DATA;
use super::HttpParseError;

/// The type of a `#[http_form_data]` field.
///
/// A `multipart/form-data` part is a header — the same one for every part, see
/// [`FormDataHeader`] — plus content. The type takes what it needs of them: [`String`], the numbers,
/// `bool` and `DateTimeAsMicroseconds` read the content as text and leave the header alone,
/// [`RawData`](super::RawData) takes the content as it is, and [`FileContent`](super::FileContent)
/// takes the file name and the content type from the header as well.
///
/// The `MyHttpInput` derive reads and writes a `#[http_form_data]` field through this trait on both
/// sides: the server half builds the field out of a part ([`Self::from_form_data_content`]), and
/// the client half sends it as one ([`Self::write_form_data`]). That is what keeps the two in step.
/// `Option<T>` and `Vec<T>` are not implementations of their own; the derive maps them onto `T`.
/// `Option` is a part that may be absent, and `Vec` is one part per item, all under the field's
/// name.
///
/// The enums of `MyHttpStringEnum` / `MyHttpIntegerEnum` and the types of `#[http_input_field]`
/// get an implementation from their derive. A type of your own needs one, written by hand, plus a
/// `DataTypeProvider` for the server's schema (what Swagger shows):
///
/// ```
/// use my_http_utils::body::FormDataBody;
/// use my_http_utils::form_data_reader::FormDataHeader;
/// use my_http_utils::http_input::{FormDataProperty, HttpParseError};
///
/// /// An uploaded PEM certificate: text, and it has to look like one.
/// pub struct PemCertificate(String);
///
/// impl FormDataProperty for PemCertificate {
///     fn from_form_data_content(
///         header: FormDataHeader<'_>,
///         content: &[u8],
///     ) -> Result<Self, HttpParseError> {
///         let text = String::from_form_data_content(header, content)?;
///         if !text.starts_with("-----BEGIN CERTIFICATE-----") {
///             return Err(HttpParseError::Validation(format!(
///                 "'{}' is not a PEM certificate",
///                 header.name
///             )));
///         }
///         Ok(Self(text))
///     }
///
///     fn write_form_data(&self, name: &'static str, form_data: FormDataBody) -> FormDataBody {
///         form_data.append_form_data_file(name, "certificate.pem", "application/x-pem-file", self.0.as_bytes())
///     }
/// }
/// ```
#[diagnostic::on_unimplemented(
    message = "`{Self}` can not be the type of a `#[http_form_data]` field",
    label = "`{Self}` does not implement `FormDataProperty`",
    note = "use `FileContent` for a file with its name and content type, `RawData` for the content only, or `String` for the content as text — or implement `my_http_utils::http_input::FormDataProperty` for `{Self}`"
)]
pub trait FormDataProperty: Sized {
    /// Server: the value out of one part.
    ///
    /// `header.name` is the field's name, for an error to carry. A form-data model also takes an
    /// `x-www-form-urlencoded` or JSON body, and a value read out of one of those has no part
    /// header: it comes with the name only ([`FormDataHeader::new`]), and with its text as the
    /// content.
    fn from_form_data_content(
        header: FormDataHeader<'_>,
        content: &[u8],
    ) -> Result<Self, HttpParseError>;

    /// Client: appends the value to `form_data` as one part named `name`.
    fn write_form_data(&self, name: &'static str, form_data: FormDataBody) -> FormDataBody;
}

/// The content as text — where every text-valued type starts. A file part counts too: its content
/// is text when it is (an uploaded `.pem`, `.csv`, …).
fn content_as_str<'c>(
    header: &FormDataHeader<'_>,
    content: &'c [u8],
) -> Result<&'c str, HttpParseError> {
    std::str::from_utf8(content).map_err(|_| {
        // Not the bytes themselves: a binary upload would end up whole in the error message.
        HttpParseError::cannot_parse(
            header.name,
            SRC_FORM_DATA,
            format!("<{} bytes that are not UTF-8 text>", content.len()),
        )
    })
}

impl FormDataProperty for String {
    fn from_form_data_content(
        header: FormDataHeader<'_>,
        content: &[u8],
    ) -> Result<Self, HttpParseError> {
        content_as_str(&header, content).map(str::to_string)
    }

    fn write_form_data(&self, name: &'static str, form_data: FormDataBody) -> FormDataBody {
        form_data.append_form_data_field(name, self)
    }
}

/// The types read with `str::parse` and written with `Display`.
macro_rules! impl_form_data_property_with_parse {
    ($($t:ty),+ $(,)?) => {
        $(
            impl FormDataProperty for $t {
                fn from_form_data_content(
                    header: FormDataHeader<'_>,
                    content: &[u8],
                ) -> Result<Self, HttpParseError> {
                    let text = content_as_str(&header, content)?;
                    text.parse()
                        .map_err(|_| HttpParseError::cannot_parse(header.name, SRC_FORM_DATA, text))
                }

                fn write_form_data(&self, name: &'static str, form_data: FormDataBody) -> FormDataBody {
                    form_data.append_form_data_field(name, self)
                }
            }
        )+
    };
}

impl_form_data_property_with_parse!(u8, i8, u16, i16, u32, i32, u64, i64, usize, isize, f32, f64);

/// `true` / `1` and `false` / `0`, case-insensitive — what a query / header / body `bool` accepts.
impl FormDataProperty for bool {
    fn from_form_data_content(
        header: FormDataHeader<'_>,
        content: &[u8],
    ) -> Result<Self, HttpParseError> {
        let text = content_as_str(&header, content)?;
        if text == "1" || text.eq_ignore_ascii_case("true") {
            return Ok(true);
        }
        if text == "0" || text.eq_ignore_ascii_case("false") {
            return Ok(false);
        }
        Err(HttpParseError::cannot_parse(
            header.name,
            SRC_FORM_DATA,
            text,
        ))
    }

    fn write_form_data(&self, name: &'static str, form_data: FormDataBody) -> FormDataBody {
        form_data.append_form_data_field(name, self)
    }
}

/// Written as `to_rfc3339_utc()` — the spelling every other part of a request uses for a date.
impl FormDataProperty for DateTimeAsMicroseconds {
    fn from_form_data_content(
        header: FormDataHeader<'_>,
        content: &[u8],
    ) -> Result<Self, HttpParseError> {
        let text = content_as_str(&header, content)?;
        DateTimeAsMicroseconds::from_str(text)
            .ok_or_else(|| HttpParseError::cannot_parse(header.name, SRC_FORM_DATA, text))
    }

    fn write_form_data(&self, name: &'static str, form_data: FormDataBody) -> FormDataBody {
        form_data.append_form_data_field(name, self.to_rfc3339_utc())
    }
}
