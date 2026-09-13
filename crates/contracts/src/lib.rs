//! Provisional version 2 interchange types. Simulation policy lives outside these records.
pub mod draft;
pub mod golden;
pub mod identity;
pub mod locks;
pub mod scoring;
pub mod timed;
mod types;
pub use types::*;
pub const SCHEMA_VERSION: u32 = 2;
pub type Error = String;
pub type Result<T> = std::result::Result<T, Error>;

/// Export explicit numeric bounds instead of depending on optional JSON-Schema formats.
pub fn json_schema() -> serde_json::Value {
    let mut schema = serde_json::to_value(schemars::schema_for!(ContractCatalog))
        .expect("schema is serializable");
    fn bound(value: &mut serde_json::Value) {
        match value {
            serde_json::Value::Object(map) => {
                let maximum = match map.get("format").and_then(|v| v.as_str()) {
                    Some("uint8") => Some(u8::MAX as u64),
                    Some("uint16") => Some(u16::MAX as u64),
                    Some("uint32") => Some(u32::MAX as u64),
                    _ => None,
                };
                if let Some(maximum) = maximum {
                    map.insert("maximum".into(), maximum.into());
                }
                map.values_mut().for_each(bound);
            }
            serde_json::Value::Array(values) => values.iter_mut().for_each(bound),
            _ => {}
        }
    }
    bound(&mut schema);
    schema
}
