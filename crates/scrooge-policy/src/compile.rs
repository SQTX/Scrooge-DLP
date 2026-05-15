// SPDX-License-Identifier: GPL-2.0-only
//
// ScroogeDLP - Data Loss Prevention system
// Copyright (C) 2026 SQTX <sssqtx@gmail.com>

//! Kompilacja [`Policy`] do binarnej formy do push'a do agentów.
//!
//! **Dlaczego MessagePack a nie JSON/YAML?**
//! - **Mniejszy rozmiar** — ~30-40% kompresja vs JSON. Ważne przy push'u
//!   przez gRPC Stream gdzie payload limit ma znaczenie.
//! - **Deterministyczny output** — ten sam policy struct daje zawsze ten
//!   sam payload (msgpack jest binarny, nie ma white-space variability).
//!   Pozwala SHA-256 hash'om matchować się przy delta-push (manager wysyła
//!   tylko polityki których agent nie ma — porównanie po hash).
//! - **Zerocopy decoding** na agencie — rmp-serde umie zerocopy parsować
//!   `&[u8]` → struct (mniejszy memory footprint w agencie).

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{Policy, PolicyError};

/// Compiled policy gotowy do push'a do agenta.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CompiledPolicy {
    /// Nazwa policy (z `metadata.name`) — używana jako klucz w
    /// `policy_assignments.policy_name`.
    pub name: String,

    /// Wersja z `metadata.version` w momencie compile.
    pub version: u32,

    /// MessagePack-encoded `Policy` struct. Agent decoduje przez
    /// `rmp_serde::from_slice::<Policy>(...)`.
    pub msgpack: Vec<u8>,

    /// SHA-256 hash `msgpack` bytes, hex-encoded. Manager wysyła hash w
    /// pakiecie `PolicyAck` po push'u i porównuje z lokalnym — jeśli się
    /// zgadza, agent na pewno ma aktualną wersję.
    pub hash: String,
}

impl CompiledPolicy {
    /// Kompiluje `Policy` → MessagePack + SHA-256 hash.
    pub fn from_policy(policy: &Policy) -> Result<Self, PolicyError> {
        let msgpack = rmp_serde::to_vec(policy)?;
        let hash = sha256_hex(&msgpack);
        Ok(Self {
            name: policy.metadata.name.clone(),
            version: policy.metadata.version,
            msgpack,
            hash,
        })
    }

    /// Decode `msgpack` z powrotem do `Policy`. Agent używa do ładowania
    /// otrzymanej policy do silnika detekcji.
    pub fn decode(&self) -> Result<Policy, rmp_serde::decode::Error> {
        rmp_serde::from_slice(&self.msgpack)
    }
}

/// SHA-256 + hex format (64 znaki, lowercase).
fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}
