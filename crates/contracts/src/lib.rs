//! Version 1 interchange types. Simulation policy lives outside these records.
pub mod identity;
pub mod scoring;
mod types;
pub use types::*;
pub const SCHEMA_VERSION: u32 = 1;
pub type Error = String;
pub type Result<T> = std::result::Result<T, Error>;
