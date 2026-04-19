//! Opaque refresh token generation and hashing.

use rand::RngCore;
use sha2::{Digest, Sha256};

/// Generate a cryptographically random 32-byte opaque refresh token string
/// encoded as lowercase hex (64 characters).
pub fn generate_refresh_token() -> String {
    let mut bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    bytes.iter().fold(String::with_capacity(64), |mut s, b| {
        use std::fmt::Write as _;
        write!(s, "{b:02x}").expect("write to String is infallible");
        s
    })
}

/// Return the SHA-256 hex digest of a refresh token string.
///
/// Only the hash is stored in the database — the plaintext token is never persisted.
pub fn hash_refresh_token(token: &str) -> String {
    let hash = Sha256::digest(token.as_bytes());
    hash.iter().fold(String::with_capacity(64), |mut s, b| {
        use std::fmt::Write as _;
        write!(s, "{b:02x}").expect("write to String is infallible");
        s
    })
}
