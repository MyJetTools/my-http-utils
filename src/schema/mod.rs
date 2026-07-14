// The client request builder is always available; the OpenAPI/Swagger description (data types,
// input parameters, results) is a server concern, gated behind `server` so browser clients don't
// carry it.
pub mod client;

#[cfg(feature = "server")]
pub mod data_types;
#[cfg(feature = "server")]
pub mod in_parameters;
#[cfg(feature = "server")]
pub mod out_results;
