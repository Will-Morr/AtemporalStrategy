//! Server library: the sim thread adapter, match controller, archive and protocol routes are
//! shared with the native peripheral (`crates/runner`), which reuses the revision store and
//! static serving while never running the authoritative match.
pub mod adapter;
pub mod archive;
pub mod controller;
pub mod ws;
