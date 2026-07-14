use types_reader::StructProperty;

pub trait StructPropertyExt {
    fn get_name(&self) -> Result<&str, syn::Error>;
}

impl<'s> StructPropertyExt for StructProperty<'s> {
    fn get_name(&self) -> Result<&str, syn::Error> {
        if let Ok(value) = self.attrs.get_named_param("serde", "rename") {
            return Ok(value.unwrap_as_value()?.as_string()?);
        }

        Ok(self.name.as_str())
    }
}
