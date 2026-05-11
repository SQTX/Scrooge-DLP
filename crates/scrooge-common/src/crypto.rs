// SPDX-License-Identifier: GPL-2.0-only
//
// ScroogeDLP - Data Loss Prevention system
// Copyright (C) 2026 SQTX <sssqtx@gmail.com>
//
// This program is free software; you can redistribute it and/or modify
// it under the terms of the GNU General Public License version 2 as
// published by the Free Software Foundation.

//! Helpery kryptograficzne.
//!
//! - [`hash_password`] / [`verify_password`] — bcrypt z [`BCRYPT_COST`] = 12.
//! - Stałe [`AES_KEY_LEN`] / [`AES_NONCE_LEN`] dla AES-256-GCM. Pełna
//!   implementacja szyfrowania quarantine dojdzie w Milestone 7 (`mod_response`).

use crate::error::CryptoError;

/// Bcrypt cost — 12 to dobry kompromis security / performance
/// (~250 ms na hash na typowym CPU).
pub const BCRYPT_COST: u32 = 12;

/// Długość klucza AES-256 w bajtach.
pub const AES_KEY_LEN: usize = 32;

/// Długość nonce GCM w bajtach.
pub const AES_NONCE_LEN: usize = 12;

/// Hashuje password przez bcrypt z cost [`BCRYPT_COST`].
pub fn hash_password(password: &str) -> Result<String, CryptoError> {
    Ok(bcrypt::hash(password, BCRYPT_COST)?)
}

/// Weryfikuje password przeciwko bcrypt hashowi (constant-time przez bcrypt crate).
pub fn verify_password(password: &str, hash: &str) -> Result<bool, CryptoError> {
    Ok(bcrypt::verify(password, hash)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bcrypt_roundtrip() {
        let hash = hash_password("hunter2").unwrap();
        assert!(verify_password("hunter2", &hash).unwrap());
        assert!(!verify_password("wrong-password", &hash).unwrap());
    }

    #[test]
    fn bcrypt_produces_different_hashes_for_same_password() {
        // Bcrypt używa losowej soli - dwa hashe tego samego password są różne.
        let h1 = hash_password("same-input").unwrap();
        let h2 = hash_password("same-input").unwrap();
        assert_ne!(h1, h2);
        assert!(verify_password("same-input", &h1).unwrap());
        assert!(verify_password("same-input", &h2).unwrap());
    }

    #[test]
    fn aes_constants_match_aes_256_gcm_spec() {
        assert_eq!(AES_KEY_LEN, 32, "AES-256 = 256 bits = 32 bytes");
        assert_eq!(AES_NONCE_LEN, 12, "GCM standard nonce = 96 bits = 12 bytes");
    }
}
