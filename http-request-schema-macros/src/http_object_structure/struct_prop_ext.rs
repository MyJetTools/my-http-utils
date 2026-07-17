use types_reader::StructProperty;

use crate::field_key::RenameAllRule;

pub trait StructPropertyExt {
    /// The field's key on the wire.
    ///
    /// `rename_all` is the container's `#[serde(rename_all = "..")]`, read once per struct by
    /// [`crate::field_key::read_container_rename_all`] and threaded in. The key has to
    /// match what serde expects, because an object structure is written into a request body by the
    /// generated `JsonValueWriter` but read back out of it with serde.
    fn get_name(&self, rename_all: Option<RenameAllRule>) -> Result<String, syn::Error>;
}

impl<'s> StructPropertyExt for StructProperty<'s> {
    fn get_name(&self, rename_all: Option<RenameAllRule>) -> Result<String, syn::Error> {
        crate::field_key::resolve_field_key(self.field, self.name.as_str(), rename_all)
    }
}
