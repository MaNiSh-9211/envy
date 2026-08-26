//! envy core engine — reusable library.
//!
//! Embed envy's schema parsing, layering, validation, vault resolution,
//! encryption-at-rest, leak scanning and SDK generation directly into your
//! own Rust tooling:
//!
//! ```no_run
//! use envy::{discovery, resolver, schema::EnvySchema};
//!
//! let cwd = std::env::current_dir().unwrap();
//! let schema_path = discovery::find_schema_upward(&cwd).expect("no envy.yaml found");
//! let schema = EnvySchema::load(&schema_path).unwrap();
//!
//! let layers = resolver::Layers { base: &Default::default(), overlay: None };
//! let resolved = resolver::resolve(&schema, &layers, &resolver::Options::default());
//! println!("{:?}", resolved.values);
//! ```

pub mod crypto;
pub mod discovery;
pub mod gencode;
pub mod git;
pub mod leakscan;
pub mod local;
pub mod prompt;
pub mod resolver;
pub mod schema;
pub mod store;
pub mod suggest;
pub mod vault;

pub use resolver::{Options as ResolveOptions, Resolved};
pub use schema::{EnvySchema, VarSpec};
