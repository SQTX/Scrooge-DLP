// SPDX-License-Identifier: GPL-2.0-only
//
// ScroogeDLP - Data Loss Prevention system
// Copyright (C) 2026 SQTX <sssqtx@gmail.com>

//! `PeselClassifier` — wykrywa polskie numery PESEL.
//!
//! PESEL = 11 cyfr. Walidacja mod-11 z wagami [1,3,7,9,1,3,7,9,1,3]:
//!   sum = Σ digit[i] * weight[i] mod 10
//!   checksum = (10 - sum) mod 10  ==  digit[10]
//!
//! Redact: `*****-*-XX-XX` (pierwsze 7 zamaskowane, ostatnie 4 widoczne —
//! kompromis dedup vs privacy; ostatnie cyfry niosą datę urodzenia + płeć).

use regex::Regex;

use super::{Classifier, Match};

pub const NAME: &str = "polish_pesel";

/// Goly 11-cyfrowy ciąg z word boundaries. Brak separator support'u w PESEL
/// (zawsze pisany ciągiem).
const PESEL_REGEX: &str = r"\b\d{11}\b";
const WEIGHTS: [u32; 10] = [1, 3, 7, 9, 1, 3, 7, 9, 1, 3];

#[derive(Debug)]
pub struct PeselClassifier {
    re: Regex,
}

impl Default for PeselClassifier {
    fn default() -> Self {
        Self::new()
    }
}

impl PeselClassifier {
    #[must_use]
    pub fn new() -> Self {
        Self {
            re: Regex::new(PESEL_REGEX).expect("PESEL_REGEX musi się kompilować"),
        }
    }
}

impl Classifier for PeselClassifier {
    fn name(&self) -> &str {
        NAME
    }

    fn scan(&self, text: &str) -> Vec<Match> {
        let mut out = Vec::new();
        for m in self.re.find_iter(text) {
            let s = m.as_str();
            if !pesel_check(s) {
                continue;
            }
            out.push(Match {
                classifier: NAME.to_string(),
                start: m.start(),
                end: m.end(),
                redacted: redact(s),
                last4: s[s.len() - 4..].to_string(),
            });
        }
        out
    }
}

fn pesel_check(digits: &str) -> bool {
    if digits.len() != 11 {
        return false;
    }
    let bytes = digits.as_bytes();
    let mut sum = 0u32;
    for (i, w) in WEIGHTS.iter().enumerate() {
        let d = u32::from(bytes[i] - b'0');
        if d > 9 {
            return false;
        }
        sum += d * w;
    }
    let checksum = (10 - (sum % 10)) % 10;
    let last = u32::from(bytes[10] - b'0');
    checksum == last
}

/// `87010120000` → `*****-*-XX-00` style — ostatnie 4 widoczne, reszta gwiazdki.
fn redact(digits: &str) -> String {
    format!("*******{}", &digits[digits.len() - 4..])
}

// ──────────────────────────────────────────────────────────────────────────
// Testy
// ──────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_valid_pesel() {
        let c = PeselClassifier::new();
        // 44051401359 — przykład PESEL z dokumentacji (mężczyzna ur. 1944).
        let matches = c.scan("PESEL: 44051401359");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].classifier, "polish_pesel");
        assert_eq!(matches[0].last4, "1359");
        assert_eq!(matches[0].redacted, "*******1359");
    }

    #[test]
    fn rejects_invalid_checksum() {
        let c = PeselClassifier::new();
        // 44051401358 — zmieniony last digit (oryginał 9).
        let matches = c.scan("44051401358");
        assert!(matches.is_empty());
    }

    #[test]
    fn rejects_wrong_length() {
        let c = PeselClassifier::new();
        assert!(c.scan("123456789").is_empty()); // 9 cyfr
        assert!(c.scan("123456789012").is_empty()); // 12 cyfr
    }

    #[test]
    fn detects_multiple() {
        let c = PeselClassifier::new();
        let matches = c.scan("dwa: 44051401359 i 02070803628");
        assert_eq!(matches.len(), 2);
    }

    #[test]
    fn no_match_plain_text() {
        let c = PeselClassifier::new();
        assert!(c.scan("Hello world").is_empty());
    }

    #[test]
    fn pesel_check_table() {
        assert!(pesel_check("44051401359"));
        assert!(pesel_check("02070803628"));
        assert!(!pesel_check("44051401358"));
        assert!(!pesel_check("11111111111")); // wszystkie 1 — checksum nie pasuje
    }
}
