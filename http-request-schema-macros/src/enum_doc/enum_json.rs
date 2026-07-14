use types_reader::{EnumCase, MacrosAttribute};

use crate::attributes::EnumCaseAttribute;

pub struct EnumJson<'s> {
    pub src: EnumCase<'s>,
    pub attr: EnumCaseAttribute,
}

impl<'s> EnumJson<'s> {
    pub fn new(src: EnumCase<'s>) -> Result<Self, syn::Error> {
        let attr: Option<EnumCaseAttribute> = src.try_get_attribute()?;

        if attr.is_none() {
            return Err(syn::Error::new_spanned(
                src.get_name_ident(),
                format!(
                    "Enum case does not have #[{}] attribute",
                    EnumCaseAttribute::NAME
                )
                .as_str(),
            ));
        }

        Ok(Self {
            src,
            attr: attr.unwrap(),
        })
    }

    pub fn get_id(&self) -> Result<isize, syn::Error> {
        if let Some(id) = self.attr.id.as_ref() {
            match id.parse() {
                Ok(id) => return Ok(id),
                Err(_) => {
                    let err =
                        syn::Error::new_spanned(self.src.get_name_ident(), "Id must be a number");
                    return Err(err);
                }
            };
        }
        let err = syn::Error::new_spanned(self.src.get_name_ident(), "[id] is not found");
        Err(err)
    }

    pub fn get_enum_case_value(&self) -> String {
        self.src.get_name_ident().to_string()
    }

    pub fn get_enum_case_str_value(&self) -> Result<String, syn::Error> {
        if let Some(value) = self.attr.value.as_ref() {
            return Ok(value.to_string());
        }

        Ok(self.src.get_name_ident().to_string())
    }

    pub fn description(&self) -> &str {
        self.attr.description.as_str()
    }
}
