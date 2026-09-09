/// Which field of which struct a [`DataTypeProvider`](super::DataTypeProvider) is describing.
///
/// It exists for one reason: the schema of a model is built when a controller is **registered**,
/// i.e. in the service's start-up path — not while serving a request. A Rust type that has no
/// schema representation therefore does not fail a request, it panics the process before it has
/// served (or logged) anything, and the service restart-loops. A message that prints only the
/// offending *element type* is useless there: in a 30-field model it does not say which field to
/// look at. So every panic on that path is handed this, and names `Struct::field`.
///
/// The context is threaded *through* the nested providers, not just the outermost one: the derive
/// hands it to the field's own type, `Vec<T>` passes it down to `T`, and so on — so a
/// `Vec<Vec<HashMap<..>>>` still reports the field that declared it.
///
/// `field_name` is the field's **Rust** name, not its key on the wire: the question the message
/// has to answer is "which line do I open", and a `#[serde(rename)]`d field cannot be found in the
/// source by its wire name.
#[derive(Clone, Copy, Debug)]
pub struct SchemaFieldCtx {
    struct_id: &'static str,
    field_name: &'static str,
}

impl SchemaFieldCtx {
    pub fn new(struct_id: &'static str, field_name: &'static str) -> Self {
        Self {
            struct_id,
            field_name,
        }
    }

    /// For a `get_data_type()` call made outside of any field — there is nothing to name. Used
    /// when a caller reaches a provider directly rather than through a model's derive.
    pub fn unknown() -> Self {
        Self {
            struct_id: "<unknown struct>",
            field_name: "<unknown field>",
        }
    }

    pub fn struct_id(&self) -> &'static str {
        self.struct_id
    }

    pub fn field_name(&self) -> &'static str {
        self.field_name
    }
}

impl std::fmt::Display for SchemaFieldCtx {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}::{}", self.struct_id, self.field_name)
    }
}
