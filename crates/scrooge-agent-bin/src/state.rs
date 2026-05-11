// SPDX-License-Identifier: GPL-2.0-only
//
// ScroogeDLP - Data Loss Prevention system
// Copyright (C) 2026 SQTX <sssqtx@gmail.com>

//! Persystencja stanu agenta po enrollment:
//! - `agent.cert` — cert podpisany przez manager CA (PEM)
//! - `agent.key`  — klucz prywatny agenta (PEM, chmod 600 na Unix)
//! - `agent.id`   — UUID przydzielone przez manager
//!
//! Cały stan żyje w `agent.data_dir` z konfigu.

use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use uuid::Uuid;

const CERT_FILENAME: &str = "agent.cert";
const KEY_FILENAME: &str = "agent.key";
const ID_FILENAME: &str = "agent.id";

#[derive(Debug, Clone)]
pub(crate) struct AgentState {
    pub agent_id: Uuid,
    pub cert_pem: String,
    pub key_pem: String,
}

impl AgentState {
    /// Próbuje wczytać stan z `data_dir`. `Ok(None)` jeśli któryś z plików
    /// nie istnieje — wtedy bin idzie ścieżką enrollment'u.
    pub(crate) fn load(data_dir: &Path) -> Result<Option<Self>> {
        let cert_path = data_dir.join(CERT_FILENAME);
        let key_path = data_dir.join(KEY_FILENAME);
        let id_path = data_dir.join(ID_FILENAME);

        if !cert_path.exists() || !key_path.exists() || !id_path.exists() {
            return Ok(None);
        }

        let cert_pem = fs::read_to_string(&cert_path)
            .with_context(|| format!("reading {}", cert_path.display()))?;
        let key_pem = fs::read_to_string(&key_path)
            .with_context(|| format!("reading {}", key_path.display()))?;
        let id_raw = fs::read_to_string(&id_path)
            .with_context(|| format!("reading {}", id_path.display()))?;
        let agent_id = Uuid::parse_str(id_raw.trim()).context("parsing agent.id")?;

        Ok(Some(Self {
            agent_id,
            cert_pem,
            key_pem,
        }))
    }

    /// Zapisuje stan do `data_dir`. Tworzy katalog jeśli nie istnieje.
    /// Na Unix ustawia `chmod 600` na klucz prywatny.
    pub(crate) fn save(&self, data_dir: &Path) -> Result<()> {
        fs::create_dir_all(data_dir).with_context(|| format!("creating {}", data_dir.display()))?;

        let cert_path: PathBuf = data_dir.join(CERT_FILENAME);
        let key_path: PathBuf = data_dir.join(KEY_FILENAME);
        let id_path: PathBuf = data_dir.join(ID_FILENAME);

        fs::write(&cert_path, &self.cert_pem)
            .with_context(|| format!("writing {}", cert_path.display()))?;
        fs::write(&key_path, &self.key_pem)
            .with_context(|| format!("writing {}", key_path.display()))?;
        fs::write(&id_path, self.agent_id.to_string())
            .with_context(|| format!("writing {}", id_path.display()))?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&key_path, fs::Permissions::from_mode(0o600))
                .with_context(|| format!("chmod 600 {}", key_path.display()))?;
        }

        Ok(())
    }
}
