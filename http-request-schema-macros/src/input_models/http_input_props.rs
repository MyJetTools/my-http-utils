use types_reader::{MacrosAttribute, PropertyType, StructProperty};

use super::InputField;
use crate::attributes::*;

pub struct HttpInputProperties<'s> {
    pub header_fields: Option<Vec<InputField<'s>>>,
    pub query_string_fields: Option<Vec<InputField<'s>>>,
    pub body_fields: Option<Vec<InputField<'s>>>,
    pub form_data_fields: Option<Vec<InputField<'s>>>,
    pub body_raw_field: Option<InputField<'s>>,
    /// The one `#[http_body_as_stream]` field, if any — the body is handed to the model as a
    /// chunk stream instead of being materialised.
    pub body_as_stream_field: Option<InputField<'s>>,
    pub path_fields: Option<Vec<InputField<'s>>>,
}

impl<'s> HttpInputProperties<'s> {
    pub fn new(props: &'s [StructProperty]) -> Result<Self, syn::Error> {
        let mut body_fields = Vec::with_capacity(props.len());
        let mut query_string_fields = Vec::with_capacity(props.len());
        let mut header_fields = Vec::with_capacity(props.len());

        let mut path_fields = Vec::with_capacity(props.len());

        let mut form_data_fields = Vec::with_capacity(props.len());

        let mut body_raw_field = None;

        let mut body_as_stream_field = None;

        for struct_property in props {
            if struct_property.attrs.has_attr(IgnoreAttribute::NAME) {
                continue;
            }

            let attr: Option<HttpQueryAttribute> = struct_property.try_get_attribute()?;

            if let Some(attr) = attr {
                query_string_fields.push(InputField::new(struct_property, attr));
                continue;
            }

            let attr: Option<HttpPathAttribute> = struct_property.try_get_attribute()?;

            if let Some(attr) = attr {
                path_fields.push(InputField::new(struct_property, attr));
                continue;
            }

            let attr: Option<HttpHeaderAttribute> = struct_property.try_get_attribute()?;

            if let Some(attr) = attr {
                header_fields.push(InputField::new(struct_property, attr));
                continue;
            }

            let attr: Option<HttpFormDataAttribute> = struct_property.try_get_attribute()?;

            if let Some(attr) = attr {
                form_data_fields.push(InputField::new(struct_property, attr));
                continue;
            }

            let attr: Option<HttpBodyAttribute> = struct_property.try_get_attribute()?;

            if let Some(attr) = attr {
                body_fields.push(InputField::new(struct_property, attr));
                continue;
            }

            let attr: Option<HttpBodyRawAttribute> = struct_property.try_get_attribute()?;

            if let Some(attr) = attr {
                if body_raw_field.is_some() {
                    struct_property.throw_error("#[http_body_raw] can be used on only one field")?;
                }

                body_raw_field = Some(InputField::new(struct_property, attr));

                continue;
            }

            let attr: Option<HttpBodyAsStreamAttribute> = struct_property.try_get_attribute()?;

            if let Some(attr) = attr {
                if body_as_stream_field.is_some() {
                    struct_property
                        .throw_error("#[http_body_as_stream] can be used on only one field")?;
                }

                body_as_stream_field = Some(InputField::new(struct_property, attr));

                continue;
            }
        }

        let result = Self {
            body_fields: into_option(body_fields),
            header_fields: into_option(header_fields),
            query_string_fields: into_option(query_string_fields),
            path_fields: into_option(path_fields),
            body_raw_field,
            body_as_stream_field,
            form_data_fields: into_option(form_data_fields),
        };

        result.self_check()?;

        Ok(result)
    }

    fn self_check(&self) -> Result<(), syn::Error> {
        // A model may use only ONE body kind: #[http_body], #[http_form_data], #[http_body_raw]
        // or #[http_body_as_stream]. Report a single, clear "choose one" error if more than one
        // is used. Streaming is exclusive with the rest for a hard reason: the body cannot be
        // materialised and streamed at the same time.
        let mut body_kinds: Vec<&'static str> = Vec::new();
        if self.body_fields.is_some() {
            body_kinds.push("#[http_body]");
        }
        if self.form_data_fields.is_some() {
            body_kinds.push("#[http_form_data]");
        }
        if self.body_raw_field.is_some() {
            body_kinds.push("#[http_body_raw]");
        }
        if self.body_as_stream_field.is_some() {
            body_kinds.push("#[http_body_as_stream]");
        }
        if body_kinds.len() > 1 {
            // Point the error at some body field so the message is actionable.
            let span = self
                .body_as_stream_field
                .as_ref()
                .map(|f| f.property.field)
                .or_else(|| self.body_raw_field.as_ref().map(|f| f.property.field))
                .or_else(|| {
                    self.form_data_fields
                        .as_ref()
                        .and_then(|f| f.first())
                        .map(|f| f.property.field)
                })
                .or_else(|| {
                    self.body_fields
                        .as_ref()
                        .and_then(|f| f.first())
                        .map(|f| f.property.field)
                })
                .unwrap();

            return Err(syn::Error::new_spanned(
                span,
                format!(
                    "a model can use only one body kind, but found {}. \
                     Choose one of: #[http_body], #[http_form_data], #[http_body_raw], \
                     #[http_body_as_stream]",
                    body_kinds.join(" + ")
                ),
            ));
        }

        if let Some(body_fields) = &self.body_fields {
            check_duplicated(body_fields)?;
        }

        if let Some(form_data_fields) = &self.form_data_fields {
            check_duplicated(form_data_fields)?;
        }

        if let Some(header_fields) = &self.header_fields {
            check_duplicated(header_fields)?;
        }

        if let Some(query_string_fields) = &self.query_string_fields {
            check_duplicated(query_string_fields)?;
        }

        // `HttpBodyAsStream` is always produced by the transport (`empty()` when there is nothing
        // to stream), so there is no "absent" case an Option could carry — and `parse` has no
        // meaningful `None` to fall back to.
        if let Some(body_as_stream_field) = &self.body_as_stream_field {
            if body_as_stream_field.property.ty.is_option() {
                return Err(syn::Error::new_spanned(
                    body_as_stream_field.property.field,
                    "#[http_body_as_stream] field can not be Option",
                ));
            }
        }

        if let Some(path_fields) = &self.path_fields {
            for path_field in path_fields {
                if path_field.property.ty.is_option() {
                    let err = syn::Error::new_spanned(
                        path_field.property.field,
                        "Path field can not be optional",
                    );
                    return Err(err);
                }
            }
        }

        // A `Vec<T>` form-data field is one part per item, so `Vec<u8>` would be one part per
        // byte — never what is meant, and it would compile. Say what to use instead.
        if let Some(form_data_fields) = &self.form_data_fields {
            for field in form_data_fields {
                let ty = match &field.property.ty {
                    PropertyType::OptionOf(inner) => inner.as_ref(),
                    other => other,
                };
                if let PropertyType::VecOf(inner) = ty {
                    if matches!(inner.as_ref(), PropertyType::U8) {
                        return field.throw_error(
                            "`Vec<u8>` in #[http_form_data] would be sent as one part per byte. \
                             Use `RawData` for the content of a part, or `FileContent` to keep \
                             its file name and content type as well",
                        );
                    }
                }
            }
        }

        Ok(())
    }

    pub fn get_all(&'s self) -> Vec<&'s InputField<'s>> {
        let mut result = Vec::new();

        if let Some(header_fields) = &self.header_fields {
            result.extend(header_fields);
        }

        if let Some(query_string_fields) = &self.query_string_fields {
            result.extend(query_string_fields);
        }

        if let Some(body_fields) = &self.body_fields {
            result.extend(body_fields);
        }

        if let Some(form_data_fields) = &self.form_data_fields {
            result.extend(form_data_fields);
        }

        if let Some(body_raw_field) = &self.body_raw_field {
            result.push(body_raw_field);
        }

        if let Some(body_as_stream_field) = &self.body_as_stream_field {
            result.push(body_as_stream_field);
        }

        if let Some(path_fields) = &self.path_fields {
            result.extend(path_fields);
        }

        result
    }
}

/// The codegen keys off "does this model have fields of that kind at all", so each group is an
/// `Option<Vec<_>>` rather than a possibly empty `Vec` - an empty group collapses to `None`.
fn into_option<T>(items: Vec<T>) -> Option<Vec<T>> {
    if items.is_empty() {
        None
    } else {
        Some(items)
    }
}

fn check_duplicated(items: &[InputField]) -> Result<(), syn::Error> {
    for i in 0..items.len() {
        for j in 0..items.len() {
            if i == j {
                continue;
            }

            let one = items.get(i).unwrap();
            let another = items.get(j).unwrap();

            if one.get_input_field_name()? == another.get_input_field_name()? {
                if j > i {
                    return another.throw_error("Duplicated field name")?;
                } else {
                    return one.throw_error("Duplicated field name")?;
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use quote::quote;

    use super::HttpInputProperties;

    /// The model's fields through the checks the derive runs, as the first error's message.
    fn check(model: proc_macro2::TokenStream) -> Result<(), String> {
        let ast: syn::DeriveInput = syn::parse2(model).unwrap();
        let fields = types_reader::StructProperty::read(&ast).map_err(|e| e.to_string())?;
        HttpInputProperties::new(&fields)
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    #[test]
    fn vec_of_u8_in_form_data_is_rejected_with_what_to_use_instead() {
        for ty in [quote!(Vec<u8>), quote!(Option<Vec<u8>>)] {
            let model = quote!(struct M { #[http_form_data(description = "")] f: #ty });
            let err = check(model).unwrap_err();
            assert!(err.contains("one part per byte"), "{}", err);
            assert!(err.contains("RawData"), "{}", err);
            assert!(err.contains("FileContent"), "{}", err);
        }
    }

    #[test]
    fn other_form_data_field_types_are_left_to_their_trait() {
        check(quote! {
            struct M {
                #[http_form_data(description = "")]
                cert: FileContent,
                #[http_form_data(description = "")]
                raw: RawData,
                #[http_form_data(description = "")]
                files: Vec<FileContent>,
                #[http_form_data(description = "")]
                counts: Vec<u32>,
                #[http_form_data(description = "")]
                note: Option<String>,
            }
        })
        .unwrap();
    }

    #[test]
    fn vec_of_u8_outside_form_data_is_untouched() {
        check(quote!(
            struct M {
                #[http_body_raw(description = "")]
                data: Vec<u8>,
            }
        ))
        .unwrap();
    }
}
