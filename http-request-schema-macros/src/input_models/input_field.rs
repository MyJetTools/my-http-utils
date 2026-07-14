use types_reader::rust_extensions::StrOrString;
use types_reader::StructProperty;

use super::HttpFieldAttribute;

#[derive(Clone)]
pub struct InputField<'s> {
    pub property: &'s StructProperty<'s>,
    pub attr: HttpFieldAttribute<'s>,
}

impl<'s> InputField<'s> {
    pub fn new<T: Into<HttpFieldAttribute<'s>>>(property: &'s StructProperty<'s>, attr: T) -> Self {
        Self {
            property,
            attr: attr.into(),
        }
    }

    pub fn get_input_field_name(&self) -> Result<&str, syn::Error> {
        if let Some(value) = self.attr.get_name() {
            Ok(value)
        } else {
            Ok(&self.property.name)
        }
    }

    pub fn get_description(&self) -> &str {
        self.attr.description()
    }

    pub fn throw_error<TResult>(
        &self,
        message: impl Into<StrOrString<'s>>,
    ) -> Result<TResult, syn::Error> {
        let message: StrOrString<'s> = message.into();
        let err = syn::Error::new_spanned(self.property.field, message.as_str());
        Err(err)
    }
}
