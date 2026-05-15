// SPDX-License-Identifier: GPL-2.0-only
//
// ScroogeDLP - Data Loss Prevention system
// Copyright (C) 2026 SQTX <sssqtx@gmail.com>

//! Reguły DLP — `Source` (skąd dane), `Destination` (gdzie idą), `Action`
//! (co agent ma zrobić), `Conditions` (filtry).
//!
//! Source/Destination używają `#[serde(tag = "type", rename_all = "snake_case")]`
//! żeby YAML wyglądał naturalnie:
//!
//! ```yaml
//! sources:
//!   - type: usb
//!   - type: network_download
//!     domains_except: ["sharepoint.company.com"]
//! ```

use serde::{Deserialize, Serialize};

use crate::PolicyError;

// ────────────────────────────────────────────────────────────────────────────
// Rule
// ────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Rule {
    /// Stabilny id reguły (admin-defined, slug). Używany w eventach
    /// (`triggered_rule_id`) i logach.
    pub id: String,

    /// Human-readable name dla dashboardu.
    pub name: String,

    #[serde(default = "default_true")]
    pub enabled: bool,

    pub severity: Severity,

    /// Skąd dane pochodzą (USB, network, folder, file matching classifier).
    /// Empty = match anything (rzadkie, zwykle określamy).
    #[serde(default)]
    pub sources: Vec<Source>,

    /// Gdzie dane mają iść (folder, USB, cloud upload, clipboard, print).
    /// Empty = nieograniczone (też rzadkie).
    #[serde(default)]
    pub destinations: Vec<Destination>,

    /// Dodatkowe filtry — file size, extension, regex na nazwie/ścieżce.
    #[serde(default)]
    pub conditions: Option<Conditions>,

    pub action: Action,

    /// Czy event z tej reguły ma być forward'owany do Wazuh (RFC 5424 syslog).
    /// Default false — admin musi explicit włączyć dla high-severity.
    #[serde(default)]
    pub forward_to_siem: bool,

    /// Czy pokazać desktop notification na endpoincie gdy akcja = block.
    #[serde(default)]
    pub notify_user: bool,

    /// Treść notification (HTML niedozwolony, plain UTF-8).
    #[serde(default)]
    pub message: Option<String>,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    /// Loguj event, nie blokuj. Najbezpieczniejszy default dla nowych reguł.
    LogOnly,
    /// Log + alert do dashboardu (czerwona kropka) + opcjonalnie forward
    /// do SIEM.
    LogAndAlert,
    /// Powiadom usera (desktop notification) ale nie blokuj. Edukacja.
    WarnUser,
    /// Faktyczne zablokowanie operacji (np. cancel file write, clear
    /// clipboard, kill upload).
    Block,
    /// Block + przenieś plik do quarantine (encrypted, agent-side).
    Quarantine,
    /// Wymuszony lock workstation (Win+L) — ostatecznie wysokie naruszenie.
    LockWorkstation,
    /// Kill proces który próbował operacji.
    KillProcess,
}

// ────────────────────────────────────────────────────────────────────────────
// Source — skąd dane pochodzą
// ────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Source {
    /// Plik na zewnętrznym storage (USB drive, SD card).
    Usb {
        #[serde(default)]
        except_serials: Vec<String>,
    },

    /// Plik pobierany z sieci (HTTP/HTTPS download).
    NetworkDownload {
        #[serde(default)]
        domains: Vec<String>,
        #[serde(default)]
        domains_except: Vec<String>,
    },

    /// Załącznik z email klienta (POP3/IMAP/Outlook).
    EmailAttachment,

    /// Lokalny folder.
    Directory {
        paths: Vec<String>,
        #[serde(default = "super::default_true")]
        recursive: bool,
    },

    /// Plik którego zawartość pasuje do nazwanego klasyfikatora
    /// (`credit-card-numbers`, `polish-pesel`, `iban-numbers`, etc.).
    /// Klasyfikator skanuje zawartość przed sprawdzaniem rule.
    FileMatch { classifiers: Vec<String> },

    /// Wszystkie źródła (wildcard). Używać ostrożnie — może generować
    /// dużo false-positives.
    Any,
}

// ────────────────────────────────────────────────────────────────────────────
// Destination — gdzie dane idą
// ────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Destination {
    /// Lokalny folder.
    Directory {
        paths: Vec<String>,
        #[serde(default = "super::default_true")]
        recursive: bool,
    },

    /// USB drive / external storage.
    Usb {
        /// Whitelist'a serial numbers — np. firmowe zaszyfrowane pendrive'y.
        #[serde(default)]
        except_serials: Vec<String>,
    },

    /// Upload przez sieć (HTTP/HTTPS POST/PUT, FTP, etc.).
    NetworkUpload {
        /// Domains które są blokowane. `["*.dropbox.com"]` itp.
        #[serde(default)]
        domains: Vec<String>,
    },

    /// Schowek systemowy.
    Clipboard,

    /// Drukarka (system print spooler).
    Print,

    /// Wszystkie zewnętrzne destynacje (USB + network + clipboard + print).
    /// Skrót dla "leaked from endpoint".
    AnyExternal,

    /// Wszystkie destynacje (wildcard).
    Any,
}

// ────────────────────────────────────────────────────────────────────────────
// Conditions — filtry post-match
// ────────────────────────────────────────────────────────────────────────────

/// Filtry stosowane PO matchu source+destination. Jeśli nie pasują, reguła
/// nie wpada w action (akcja zostaje pominięta).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Conditions {
    /// Minimalny rozmiar pliku w bajtach (lub z suffix'em: `1KB`, `2MB`).
    /// Parser konwertuje do bytes przy walidacji.
    #[serde(default)]
    pub file_size_min: Option<FileSize>,

    /// Maksymalny rozmiar.
    #[serde(default)]
    pub file_size_max: Option<FileSize>,

    /// Rozszerzenia (bez kropki: `["xlsx", "pdf"]`).
    #[serde(default)]
    pub file_extensions: Vec<String>,

    /// Regex matching `filename` (bez ścieżki).
    #[serde(default)]
    pub filename_regex: Option<String>,

    /// Regex matching pełnej ścieżki.
    #[serde(default)]
    pub path_regex: Option<String>,
}

/// File size w bytes. YAML akceptuje liczbę (`1024`) lub string z
/// suffix'em (`"1KB"`, `"2MB"`, `"512 B"`). Wewnętrznie zawsze trzymamy
/// `u64` — dzięki temu msgpack roundtrip jest deterministyczny (nie
/// zaszyte `enum` discriminant'y, tylko goła liczba).
///
/// Akceptowane jednostki: B/KB/MB/GB (case-insensitive).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileSize(pub u64);

impl Serialize for FileSize {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_u64(self.0)
    }
}

impl<'de> Deserialize<'de> for FileSize {
    fn deserialize<D: serde::Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
        use serde::de::Error;
        // Akceptuj liczbę LUB string. `serde_yaml::Value` to najprostszy
        // pośrednik — pozwala sniff'ować typ.
        let value = serde_yaml::Value::deserialize(de)?;
        match value {
            serde_yaml::Value::Number(n) => n
                .as_u64()
                .ok_or_else(|| D::Error::custom("file_size must be non-negative"))
                .map(Self),
            serde_yaml::Value::String(s) => parse_size(&s).map_err(D::Error::custom).map(Self),
            other => Err(D::Error::custom(format!(
                "file_size must be number or string, got {other:?}"
            ))),
        }
    }
}

/// `"1KB"` → 1024, `"512"` → 512, `"2 MB"` → 2097152.
pub(crate) fn parse_size(s: &str) -> Result<u64, String> {
    let s = s.trim();
    // Znajdź gdzie kończą się cyfry.
    let split_at = s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len());
    let (num_part, unit_part) = s.split_at(split_at);
    let num: u64 = num_part
        .parse()
        .map_err(|e| format!("invalid number `{num_part}`: {e}"))?;
    let multiplier = match unit_part.trim().to_uppercase().as_str() {
        "" | "B" => 1,
        "KB" | "K" => 1024,
        "MB" | "M" => 1024 * 1024,
        "GB" | "G" => 1024 * 1024 * 1024,
        other => return Err(format!("unknown size unit `{other}` (use B/KB/MB/GB)")),
    };
    Ok(num * multiplier)
}

impl FileSize {
    #[must_use]
    pub fn as_bytes(self) -> u64 {
        self.0
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Walidacja rule
// ────────────────────────────────────────────────────────────────────────────

impl Rule {
    /// Compile regex'ów + sanity check pól. Wywoływane przez [`crate::Policy::validate`].
    pub fn validate(&self) -> Result<(), PolicyError> {
        if self.id.is_empty() {
            return Err(PolicyError::Validation(
                "rule.id cannot be empty".to_string(),
            ));
        }
        if !self
            .id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        {
            return Err(PolicyError::Validation(format!(
                "rule.id `{}` must match [a-z0-9-]+",
                self.id
            )));
        }

        // Action = block / quarantine / lock_workstation / kill_process bez
        // notification message? Dobry hint dla admina, ale nie blocker.
        // (W przyszłości: warnings layer w response.)

        if let Some(conds) = &self.conditions {
            if let Some(re) = &conds.filename_regex {
                regex::Regex::new(re).map_err(|e| PolicyError::RegexCompile {
                    field: format!("rule[{}].conditions.filename_regex", self.id),
                    source: e,
                })?;
            }
            if let Some(re) = &conds.path_regex {
                regex::Regex::new(re).map_err(|e| PolicyError::RegexCompile {
                    field: format!("rule[{}].conditions.path_regex", self.id),
                    source: e,
                })?;
            }
        }

        Ok(())
    }
}
