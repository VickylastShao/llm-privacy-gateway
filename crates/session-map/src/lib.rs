#![forbid(unsafe_code)]

mod crypto;
mod error;
mod store;
mod token;

pub use error::MapError;
pub use store::{EncryptedRecord, MappingBackend, MemoryBackend, PseudonymMap, Pseudonymizer};
pub use token::token_for;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
