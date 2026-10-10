//! RP-08 / C.7: envelope-encrypted secrets (AES-256-GCM DEK + KEK wrap).
//!
//! Plaintext exists only in process memory for the shortest call span. The SQLite
//! blob never contains recoverable secret bytes without the external KEK.

mod envelope;
mod kek;
mod store;

pub use envelope::{decrypt, encrypt, rewrap, SealedSecret};
pub use kek::{MasterKey, MASTER_KEY_BYTES};
pub use store::{delete_secret, get_secret, put_secret, register_key, rewrap_all, SecretRecord};
