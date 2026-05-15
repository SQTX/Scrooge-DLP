// SPDX-License-Identifier: GPL-2.0-only
//
// ScroogeDLP - Data Loss Prevention system
// Copyright (C) 2026 SQTX <sssqtx@gmail.com>
//
// This program is free software; you can redistribute it and/or modify
// it under the terms of the GNU General Public License version 2 as
// published by the Free Software Foundation.

//! `scrooge-policy` — parsowanie, walidacja i kompilacja polityk DLP w YAML.
//!
//! **Flow:**
//! 1. Admin pisze policy w YAML (dashboard editor lub `kubectl`-style import)
//! 2. Manager parsuje YAML → [`Policy`] (serde_yaml)
//! 3. Walidacja struktury + compile regex'ów + sanity check pól ([`Policy::validate`])
//! 4. Compile do binarnej formy (MessagePack) z SHA-256 hash ([`CompiledPolicy`])
//! 5. Manager push'uje `CompiledPolicy` do agentów przez gRPC Stream
//! 6. Agent dekoduje msgpack lokalnie, ładuje do silnika detekcji
//!
//! **Źródło prawdy:** PostgreSQL po stronie managera. YAML jest formatem
//! import/export (dashboard) i postaci na agentach (gdy agent ma trzymać
//! cache offline). Po push'u agent trzyma `CompiledPolicy` (msgpack) +
//! hash w `data_dir/policies/`.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use thiserror::Error;

// ────────────────────────────────────────────────────────────────────────────
// API surface
// ────────────────────────────────────────────────────────────────────────────

pub use compile::CompiledPolicy;
pub use rules::{Action, Conditions, Destination, Rule, Severity, Source};

mod compile;
mod rules;

#[cfg(test)]
mod tests;

// ────────────────────────────────────────────────────────────────────────────
// Policy — top-level YAML document
// ────────────────────────────────────────────────────────────────────────────

/// Pełen `Policy` z YAML. Mapuje 1:1 na schemat z `PROJECT_BRIEF.md`.
///
/// `apiVersion` musi być `"scroogedlp.io/v1"`, `kind` musi być `"Policy"`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Policy {
    #[serde(rename = "apiVersion")]
    pub api_version: String,

    pub kind: String,

    pub metadata: Metadata,

    /// Selector decydujący którzy agenci dostaną tę policy. Domyślnie pusta
    /// = match wszystkich (przydatne dla "global default" policy).
    #[serde(default)]
    pub targets: Targets,

    /// Wyższy priorytet wygrywa przy konflikcie reguł między politykami
    /// applied do tego samego agenta. Default 0.
    #[serde(default)]
    pub priority: i32,

    /// Inbound + outbound regułki — agent enforce'uje sekwencyjnie.
    #[serde(default)]
    pub rules: PolicyRules,

    /// Ścieżki które agent ma monitorować dla file events. Bez wpisu —
    /// agent nie monitoruje plików w ramach tej policy.
    #[serde(default)]
    pub monitored_paths: Vec<MonitoredPath>,

    /// Nazwy klasyfikatorów (wbudowanych w agent) aktywowanych przez tę
    /// policy. Np. `"credit-card-numbers"`, `"polish-pesel"`. Definicja
    /// wzorców jest po stronie agenta (regex/walidator).
    #[serde(default)]
    pub classifiers: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Metadata {
    /// Unikalna nazwa policy (slug, [a-z0-9-]+). Manager używa jako klucz
    /// w DB `policies.name`.
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    /// Numer wersji bumpowany ręcznie przez admina przy każdym save.
    /// Manager też trzyma własny `version` w DB; ten z YAML to intencja
    /// admina ("chcę to być wersja 12").
    #[serde(default = "default_version")]
    pub version: u32,
}

fn default_version() -> u32 {
    1
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Targets {
    /// Pozytywny selector. Pusty = match wszystkich agentów.
    #[serde(default, rename = "match")]
    pub match_: TargetMatch,
    /// Negatywny override — agenty pasujące tu są wykluczone nawet
    /// jeśli pasują do `match`.
    #[serde(default)]
    pub exclude: TargetExclude,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct TargetMatch {
    /// `["windows", "macos", "linux"]` lub podzbiór.
    #[serde(default)]
    pub os: Vec<String>,
    /// Etykiety agenta typu `department: finance`. Match wymaga ALL pairs.
    #[serde(default)]
    pub tags: HashMap<String, String>,
    /// Grupa to logical assignment (admin tworzy w dashboardzie).
    #[serde(default)]
    pub groups: Vec<String>,
    /// Konkretni agenci po `agent_id` (UUID) — najsilniejszy match.
    #[serde(default)]
    pub agent_ids: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct TargetExclude {
    #[serde(default)]
    pub hostnames: Vec<String>,
    #[serde(default)]
    pub agent_ids: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct PolicyRules {
    /// Reguły dla danych przychodzących do monitorowanych lokalizacji
    /// (USB → folder, network download → folder, email → folder).
    #[serde(default)]
    pub inbound: Vec<Rule>,
    /// Reguły dla danych wychodzących (folder → USB, folder → cloud,
    /// folder → clipboard, folder → print).
    #[serde(default)]
    pub outbound: Vec<Rule>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MonitoredPath {
    pub path: String,
    #[serde(default = "default_true")]
    pub recursive: bool,
    /// Eventy do śledzenia: `create / modify / delete / rename / access`.
    /// Domyślnie wszystkie poza `access` (które generuje hałas).
    #[serde(default = "default_file_events")]
    pub events: Vec<String>,
    /// Opcjonalne filtry glob. `include` musi pasować, `exclude` wyklucza.
    #[serde(default)]
    pub file_patterns: FilePatterns,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct FilePatterns {
    #[serde(default)]
    pub include: Vec<String>,
    #[serde(default)]
    pub exclude: Vec<String>,
}

fn default_true() -> bool {
    true
}

fn default_file_events() -> Vec<String> {
    vec![
        "create".to_string(),
        "modify".to_string(),
        "delete".to_string(),
        "rename".to_string(),
    ]
}

// ────────────────────────────────────────────────────────────────────────────
// Errors
// ────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum PolicyError {
    #[error("YAML parse error: {0}")]
    Yaml(#[from] serde_yaml::Error),

    #[error("validation: {0}")]
    Validation(String),

    #[error("msgpack encode error: {0}")]
    Msgpack(#[from] rmp_serde::encode::Error),

    #[error("regex compile error in `{field}`: {source}")]
    RegexCompile {
        field: String,
        #[source]
        source: regex::Error,
    },
}

// ────────────────────────────────────────────────────────────────────────────
// Parsing + walidacja
// ────────────────────────────────────────────────────────────────────────────

impl Policy {
    /// Parsuje YAML i waliduje strukturę. Najszybsza ścieżka — admin
    /// dashboard wywoła to przy save oraz dry-run `/policies/validate`.
    pub fn from_yaml(yaml: &str) -> Result<Self, PolicyError> {
        let policy: Self = serde_yaml::from_str(yaml)?;
        policy.validate()?;
        Ok(policy)
    }

    /// Sanity check pól — nie pozwala zapisać policy która nie ma sensu.
    /// W przyszłości można dorzucić warnings (np. "rule X nie ma żadnych
    /// `sources` — czy na pewno?").
    pub fn validate(&self) -> Result<(), PolicyError> {
        if self.api_version != "scroogedlp.io/v1" {
            return Err(PolicyError::Validation(format!(
                "unsupported apiVersion `{}` (expected `scroogedlp.io/v1`)",
                self.api_version
            )));
        }
        if self.kind != "Policy" {
            return Err(PolicyError::Validation(format!(
                "unsupported kind `{}` (expected `Policy`)",
                self.kind
            )));
        }
        if self.metadata.name.is_empty() {
            return Err(PolicyError::Validation(
                "metadata.name cannot be empty".to_string(),
            ));
        }
        if !self
            .metadata
            .name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        {
            return Err(PolicyError::Validation(format!(
                "metadata.name `{}` must match [a-z0-9-]+ (slug-style)",
                self.metadata.name
            )));
        }

        // Walidacja reguł — kompilacja regex'ów + uniqueness id.
        let mut rule_ids = std::collections::HashSet::new();
        for rule in self.rules.inbound.iter().chain(self.rules.outbound.iter()) {
            if !rule_ids.insert(rule.id.clone()) {
                return Err(PolicyError::Validation(format!(
                    "duplicate rule id `{}` in policy `{}`",
                    rule.id, self.metadata.name
                )));
            }
            rule.validate()?;
        }

        Ok(())
    }

    /// Compile do binarnej formy + SHA-256 hash do delta-push'a do agentów.
    pub fn compile(&self) -> Result<CompiledPolicy, PolicyError> {
        CompiledPolicy::from_policy(self)
    }
}
