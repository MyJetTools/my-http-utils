//! Nested arrays in the schema: `Vec<Vec<T>>` and deeper.
//!
//! A model with a `Vec<Vec<T>>` field used to panic while its controller was being **registered**
//! — i.e. on service start-up, before the first log line was ever sent — because `ArrayElement`
//! had no variant able to hold another array. These tests pin both halves of the fix: the nesting
//! is now representable and recursive, and what still is not representable panics with a message
//! that names the struct and the field.

use std::collections::{BTreeMap, HashMap};

use my_http_utils::macros::*;
use my_http_utils::schema::data_types::{
    ArrayElement, DataTypeProvider, HttpDataType, SchemaFieldCtx,
};

use super::parse_tests::FakeRequest;

// ---- models -----------------------------------------------------------------

#[derive(Debug, serde::Serialize, serde::Deserialize, MyHttpObjectStructure)]
struct Interval {
    start_minute: u16,
    end_minute: u16,
}

#[derive(Clone, Copy, MyHttpStringEnum)]
enum Color {
    #[http_enum_case(id = "0", value = "red", description = "Red", default)]
    Red,
    #[http_enum_case(id = "1", value = "green", description = "Green")]
    Green,
}

/// The shapes that already worked and must keep working, byte for byte.
#[derive(MyHttpObjectStructure)]
struct Flat {
    objects: Vec<Interval>,
    strings: Vec<String>,
    colors: Vec<Color>,
}

/// The shape that used to panic.
#[derive(MyHttpObjectStructure)]
struct Hours {
    days: Vec<Vec<Interval>>,
}

#[derive(MyHttpObjectStructure)]
struct DeepNesting {
    objects: Vec<Vec<Vec<Interval>>>,
    strings: Vec<Vec<String>>,
    colors: Vec<Vec<Color>>,
    optional: Option<Vec<Vec<Interval>>>,
}

#[derive(MyHttpObjectStructure)]
struct Dictionaries {
    of_simple: HashMap<String, String>,
    of_object: HashMap<String, Interval>,
    of_enum: HashMap<String, Color>,
    of_array: HashMap<String, Vec<Interval>>,
    of_nested_array: HashMap<String, Vec<Vec<Interval>>>,
}

// ---- helpers ----------------------------------------------------------------

fn field_of(data_type: &HttpDataType, name: &str) -> HttpDataType {
    match data_type {
        HttpDataType::Object(obj) => obj
            .main
            .fields
            .iter()
            .find(|f| f.name == name)
            .unwrap_or_else(|| panic!("no field `{}`", name))
            .data_type
            .clone(),
        _ => panic!("expected an object, got {:?}", data_type),
    }
}

fn field_required(data_type: &HttpDataType, name: &str) -> bool {
    match data_type {
        HttpDataType::Object(obj) => obj.main.fields.iter().find(|f| f.name == name).unwrap().required,
        _ => panic!("expected an object"),
    }
}

/// Peels `count` array levels off `data_type` and returns what is left, asserting the shape all
/// the way down: the outer type is an `ArrayOf`, and every level below it an `ArrayElement::ArrayOf`.
fn peel_arrays(data_type: &HttpDataType, count: usize) -> ArrayElement {
    let mut element = match data_type {
        HttpDataType::ArrayOf(element) => element.clone(),
        _ => panic!("expected ArrayOf, got {:?}", data_type),
    };

    for level in 1..count {
        element = match element {
            ArrayElement::ArrayOf(inner) => *inner,
            other => panic!("expected another array at level {}, got {:?}", level, other),
        };
    }

    element
}

fn object_id(element: &ArrayElement) -> String {
    match element {
        ArrayElement::Object(obj) => obj.main.struct_id.clone(),
        _ => panic!("expected an object element, got {:?}", element),
    }
}

// ---- the shapes that already worked ------------------------------------------

#[test]
fn flat_arrays_are_unchanged() {
    let flat = Flat::get_data_type();

    assert_eq!(object_id(&peel_arrays(&field_of(&flat, "objects"), 1)), "Interval");

    assert!(matches!(
        peel_arrays(&field_of(&flat, "strings"), 1),
        ArrayElement::SimpleType(my_http_utils::schema::data_types::HttpSimpleType::String)
    ));

    assert!(matches!(
        peel_arrays(&field_of(&flat, "colors"), 1),
        ArrayElement::Enum(_)
    ));

    // No array level was silently added to a flat `Vec`.
    for name in ["objects", "strings", "colors"] {
        assert_eq!(peel_arrays(&field_of(&flat, name), 1).nested_array_depth(), 0);
    }
}

// ---- the shape that used to panic --------------------------------------------

#[test]
fn vec_of_vec_of_object_is_an_array_of_arrays() {
    let element = peel_arrays(&field_of(&Hours::get_data_type(), "days"), 2);
    assert_eq!(object_id(&element), "Interval");
}

#[test]
fn nesting_is_recursive_to_any_depth() {
    let deep = DeepNesting::get_data_type();

    assert_eq!(object_id(&peel_arrays(&field_of(&deep, "objects"), 3)), "Interval");

    assert!(matches!(
        peel_arrays(&field_of(&deep, "strings"), 2),
        ArrayElement::SimpleType(my_http_utils::schema::data_types::HttpSimpleType::String)
    ));

    assert!(matches!(
        peel_arrays(&field_of(&deep, "colors"), 2),
        ArrayElement::Enum(_)
    ));
}

#[test]
fn option_of_nested_array_keeps_the_nesting() {
    // `required` is not asserted here: the derive already reports every `Vec` field as not
    // required, so it would hold whether or not the `Option` were seen at all. What this pins is
    // that unwrapping the `Option` does not cost an array level.
    let deep = DeepNesting::get_data_type();
    assert_eq!(object_id(&peel_arrays(&field_of(&deep, "optional"), 2)), "Interval");
    assert!(!field_required(&deep, "optional"));
}

#[test]
fn innermost_and_depth_walk_the_nesting() {
    let outer = match field_of(&Hours::get_data_type(), "days") {
        HttpDataType::ArrayOf(element) => element,
        other => panic!("expected ArrayOf, got {:?}", other),
    };

    assert_eq!(outer.nested_array_depth(), 1);
    assert_eq!(object_id(outer.innermost()), "Interval");

    // A flat element is its own innermost, at depth 0.
    let flat = match field_of(&Flat::get_data_type(), "objects") {
        HttpDataType::ArrayOf(element) => element,
        other => panic!("expected ArrayOf, got {:?}", other),
    };
    assert_eq!(flat.nested_array_depth(), 0);
    assert_eq!(object_id(flat.innermost()), "Interval");
}

// ---- dictionaries ------------------------------------------------------------

#[test]
fn dictionaries_of_flat_values_are_unchanged() {
    let dict = Dictionaries::get_data_type();

    assert!(matches!(
        field_of(&dict, "of_simple"),
        HttpDataType::DictionaryOf(ArrayElement::SimpleType(
            my_http_utils::schema::data_types::HttpSimpleType::String
        ))
    ));

    match field_of(&dict, "of_object") {
        HttpDataType::DictionaryOf(element) => assert_eq!(object_id(&element), "Interval"),
        other => panic!("expected DictionaryOf, got {:?}", other),
    }

    match field_of(&dict, "of_array") {
        HttpDataType::DictionaryOfArray(element) => {
            assert_eq!(element.nested_array_depth(), 0);
            assert_eq!(object_id(&element), "Interval");
        }
        other => panic!("expected DictionaryOfArray, got {:?}", other),
    }
}

#[test]
fn dictionary_of_enum_is_described_instead_of_panicking() {
    assert!(matches!(
        field_of(&Dictionaries::get_data_type(), "of_enum"),
        HttpDataType::DictionaryOf(ArrayElement::Enum(_))
    ));
}

#[test]
fn dictionary_of_nested_array_carries_the_nesting_into_the_element() {
    let dict = Dictionaries::get_data_type();

    match field_of(&dict, "of_nested_array") {
        HttpDataType::DictionaryOfArray(element) => {
            // A dictionary of an array of arrays: one array level lives in `DictionaryOfArray`
            // itself, the nested one in the element.
            assert_eq!(element.nested_array_depth(), 1);
            assert_eq!(object_id(element.innermost()), "Interval");
        }
        other => panic!("expected DictionaryOfArray, got {:?}", other),
    }
}

#[test]
fn btree_map_describes_itself_exactly_like_hash_map() {
    // `BTreeMap` has no `JsonValueWriter`, so it cannot be a field of an object structure — it is
    // reached directly. The two map providers share one body, and this pins that they agree.
    let ctx = SchemaFieldCtx::new("Dictionaries", "of_nested_array");

    let btree = BTreeMap::<String, Vec<Vec<Interval>>>::get_data_type_in_field(ctx);
    let hash = HashMap::<String, Vec<Vec<Interval>>>::get_data_type_in_field(ctx);

    assert_eq!(format!("{:?}", btree), format!("{:?}", hash));

    match btree {
        HttpDataType::DictionaryOfArray(element) => {
            assert_eq!(element.nested_array_depth(), 1);
            assert_eq!(object_id(element.innermost()), "Interval");
        }
        other => panic!("expected DictionaryOfArray, got {:?}", other),
    }
}

#[test]
#[should_panic(expected = "Field `Report::buckets`")]
fn btree_map_panic_names_the_field_too() {
    let _ = BTreeMap::<String, HashMap<String, String>>::get_data_type_in_field(
        SchemaFieldCtx::new("Report", "buckets"),
    );
}

// ---- what is still not representable, and how it says so ---------------------

#[derive(MyHttpObjectStructure)]
struct ArrayOfDictionary {
    filler: String,
    slots: Vec<HashMap<String, String>>,
}

#[test]
#[should_panic(expected = "Field `ArrayOfDictionary::slots`")]
fn array_of_dictionary_panic_names_the_struct_and_the_field() {
    let _ = ArrayOfDictionary::get_data_type();
}

#[derive(MyHttpObjectStructure)]
struct NestedArrayOfDictionary {
    slots: Vec<Vec<HashMap<String, String>>>,
}

#[test]
#[should_panic(expected = "Field `NestedArrayOfDictionary::slots`")]
fn the_context_survives_the_nesting() {
    // Two `Vec` levels between the field and the type that has no representation — the field is
    // still the thing named.
    let _ = NestedArrayOfDictionary::get_data_type();
}

#[derive(MyHttpObjectStructure)]
struct DictionaryOfDictionary {
    slots: HashMap<String, HashMap<String, String>>,
}

#[test]
#[should_panic(expected = "Field `DictionaryOfDictionary::slots`")]
fn dictionary_of_dictionary_panic_names_the_struct_and_the_field() {
    let _ = DictionaryOfDictionary::get_data_type();
}

#[test]
#[should_panic(expected = "<unknown struct>::<unknown field>")]
fn a_provider_called_outside_a_field_says_so() {
    // Reached directly rather than through a model's derive: there is no field to name, and the
    // message says that rather than naming a wrong one.
    let _ = Vec::<HashMap<String, String>>::get_data_type();
}

// ---- input models: schema + the wire ------------------------------------------

#[derive(MyHttpInput)]
struct ScheduleModel {
    #[http_body(name = "days", description = "opening hours per day")]
    days: Vec<Vec<Interval>>,
}

#[test]
fn input_model_body_param_carries_the_nesting() {
    let params = ScheduleModel::get_input_params();
    assert_eq!(params.len(), 1);

    let element = peel_arrays(&params[0].field.data_type, 2);
    assert_eq!(object_id(&element), "Interval");
}

#[test]
fn nested_array_body_parses_off_the_wire() {
    // The schema is only half of it — the same field has to survive the JSON round trip.
    let body = r#"{"days":[[{"start_minute":540,"end_minute":720}],[{"start_minute":0,"end_minute":60},{"start_minute":120,"end_minute":180}]]}"#;

    let request = FakeRequest::default().body("application/json", body.as_bytes().to_vec());
    let model = ScheduleModel::parse(&request).unwrap();

    assert_eq!(model.days.len(), 2);
    assert_eq!(model.days[0].len(), 1);
    assert_eq!(model.days[0][0].start_minute, 540);
    assert_eq!(model.days[0][0].end_minute, 720);
    assert_eq!(model.days[1].len(), 2);
    assert_eq!(model.days[1][1].start_minute, 120);
}

// ---- the panic names the field you would go and edit ---------------------------

#[derive(serde::Serialize, MyHttpObjectStructure)]
struct RenamedField {
    #[serde(rename = "slots")]
    slot_list: Vec<HashMap<String, String>>,
}

#[test]
#[should_panic(expected = "Field `RenamedField::slot_list`")]
fn the_panic_names_the_rust_field_not_the_wire_key() {
    // The wire key is "slots"; the line to open says `slot_list`. Naming the wire key here would
    // send the reader grepping for a string that is not in the source.
    let _ = RenamedField::get_data_type();
}

// ---- the context must survive every wrapper on the way down --------------------
//
// Each of these fails if some provider on the path drops `SchemaFieldCtx` and falls back to the
// defaulted, context-free `get_data_type()` — the message would then say `<unknown struct>` and
// the operator would be back to a start-up crash naming nothing. They are the mutation guard for
// the threading itself, not for the nesting.

#[derive(serde::Serialize, MyHttpObjectStructure)]
struct OptionalUnrepresentable {
    maybe: Option<Vec<HashMap<String, String>>>,
}

#[test]
#[should_panic(expected = "Field `OptionalUnrepresentable::maybe`")]
fn the_context_survives_an_option_wrapper() {
    // The derive describes an `Option<T>` by unwrapping it and asking `T` — the context has to be
    // handed over at that unwrap, not dropped with the `Option`.
    let _ = OptionalUnrepresentable::get_data_type();
}

#[derive(serde::Serialize, MyHttpObjectStructure)]
struct DictionaryOfUnrepresentable {
    buckets: HashMap<String, Vec<HashMap<String, String>>>,
}

#[test]
#[should_panic(expected = "Field `DictionaryOfUnrepresentable::buckets`")]
fn the_context_survives_a_dictionary_wrapper() {
    // The failure is two containers below the field: HashMap -> Vec -> HashMap. The dictionary
    // provider has to pass the context into its VALUE type, not just use it for its own panic.
    let _ = DictionaryOfUnrepresentable::get_data_type();
}

#[test]
#[should_panic(expected = "Field `Payload::body`")]
fn the_context_survives_raw_data_typed() {
    // `#[http_body_raw] body: RawDataTyped<T>` describes itself as `T` — and has to forward the
    // context while doing so.
    let _ = my_http_utils::http_input::RawDataTyped::<Vec<HashMap<String, String>>>
        ::get_data_type_in_field(SchemaFieldCtx::new("Payload", "body"));
}

// ---- the other direction on the wire ------------------------------------------

struct FixedRnd;
impl my_http_utils::schema::client::RandomStringGenerator for FixedRnd {
    fn generate_random_string(_len: usize) -> String {
        "TESTBOUNDARY0001".to_string()
    }
}

#[test]
fn the_client_writes_a_nested_array_body() {
    use my_http_utils::schema::client::THttpRequestBuilder;

    let model = ScheduleModel {
        days: vec![
            vec![Interval { start_minute: 540, end_minute: 720 }],
            vec![
                Interval { start_minute: 0, end_minute: 60 },
                Interval { start_minute: 120, end_minute: 180 },
            ],
            vec![],
        ],
    };

    let body = model.get_body::<FixedRnd>().unwrap().into_vec();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    // Two array levels on the wire, and an empty inner array stays an empty array rather than
    // collapsing into the outer one.
    assert_eq!(json["days"].as_array().unwrap().len(), 3);
    assert_eq!(json["days"][0].as_array().unwrap().len(), 1);
    assert_eq!(json["days"][0][0]["start_minute"], 540);
    assert_eq!(json["days"][1].as_array().unwrap().len(), 2);
    assert_eq!(json["days"][1][1]["end_minute"], 180);
    assert_eq!(json["days"][2].as_array().unwrap().len(), 0);
}
