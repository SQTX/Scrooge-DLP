// SPDX-License-Identifier: GPL-2.0-only
//
// ScroogeDLP - Data Loss Prevention system
// Copyright (C) 2026 SQTX <sssqtx@gmail.com>

//! `LuhnClassifier` — wykrywa numery kart kredytowych w tekście.
//!
//! Pipeline:
//! 1. **Regex pre-filter** — szuka sekwencji 13-19 cyfr z opcjonalnymi
//!    separatorami (spacja, myślnik). Tanio odrzuca tekst który na pewno
//!    nie zawiera karty.
//! 2. **Luhn checksum** — walidacja mod-10. Eliminuje false positives
//!    (np. losowy ciąg 16 cyfr).
//!
//! Redacted format: `**** **** **** 9010` — placeholder bez separator'ów
//! z oryginału (deterministyczny, łatwy do dedup).

use regex::Regex;

use super::{Classifier, Match};

/// Slug klasyfikatora — używany w event payload i schemie polityk.
pub const NAME: &str = "credit_card";

/// Regex matchujący 13-19 cyfr z opcjonalnymi separatorami między grupami.
/// `\b` na obu końcach żeby nie matchować w środku dłuższych ciągów cyfr.
const CARD_REGEX: &str = r"\b(?:\d[ -]?){12,18}\d\b";

#[derive(Debug)]
pub struct LuhnClassifier {
    re: Regex,
}

impl Default for LuhnClassifier {
    fn default() -> Self {
        Self::new()
    }
}

impl LuhnClassifier {
    /// Panika tylko gdy hardcoded regex powyżej nie kompiluje — to bug
    /// dewelopera, nie runtime input. Test `luhn_classifier_new_doesnt_panic`
    /// łapie to przy CI.
    #[must_use]
    pub fn new() -> Self {
        Self {
            re: Regex::new(CARD_REGEX).expect("CARD_REGEX musi się kompilować"),
        }
    }
}

impl Classifier for LuhnClassifier {
    fn name(&self) -> &str {
        NAME
    }

    fn scan(&self, text: &str) -> Vec<Match> {
        let mut out = Vec::new();
        for m in self.re.find_iter(text) {
            let raw = m.as_str();
            let digits: String = raw.chars().filter(char::is_ascii_digit).collect();
            if !(13..=19).contains(&digits.len()) {
                continue;
            }
            if !luhn_check(&digits) {
                continue;
            }
            out.push(Match {
                classifier: NAME.to_string(),
                start: m.start(),
                end: m.end(),
                redacted: redact(&digits),
                last4: digits[digits.len() - 4..].to_string(),
            });
        }
        out
    }
}

/// Mod-10 walidacja. `digits` musi zawierać same cyfry ascii.
fn luhn_check(digits: &str) -> bool {
    let mut sum = 0u32;
    // Iterujemy od ostatniej cyfry, podwajamy co drugą (zaczynając od
    // przedostatniej — indeks 1 od końca).
    for (i, c) in digits.chars().rev().enumerate() {
        let d = c.to_digit(10).unwrap_or(0);
        let val = if i % 2 == 1 {
            let dbl = d * 2;
            if dbl > 9 {
                dbl - 9
            } else {
                dbl
            }
        } else {
            d
        };
        sum += val;
    }
    sum % 10 == 0
}

/// `4532015112830366` → `**** **** **** 0366`. Zawsze format 4-grupowy.
fn redact(digits: &str) -> String {
    let last4 = &digits[digits.len() - 4..];
    format!("**** **** **** {last4}")
}

// ──────────────────────────────────────────────────────────────────────────
// Testy
// ──────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn luhn_classifier_new_doesnt_panic() {
        let _ = LuhnClassifier::new();
    }

    #[test]
    fn detects_valid_visa() {
        let c = LuhnClassifier::new();
        let matches = c.scan("kontakt: 4532015112830366 zadzwoń jutro");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].classifier, "credit_card");
        assert_eq!(matches[0].last4, "0366");
        assert_eq!(matches[0].redacted, "**** **** **** 0366");
    }

    #[test]
    fn detects_with_dashes() {
        let c = LuhnClassifier::new();
        let matches = c.scan("karta 4532-0151-1283-0366 wystawiona");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].last4, "0366");
    }

    #[test]
    fn detects_with_spaces() {
        let c = LuhnClassifier::new();
        let matches = c.scan("4532 0151 1283 0366");
        assert_eq!(matches.len(), 1);
    }

    #[test]
    fn detects_amex_15_digits() {
        let c = LuhnClassifier::new();
        // 378282246310005 — Amex test number z PCI-DSS test data.
        let matches = c.scan("amex: 378282246310005");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].last4, "0005");
    }

    #[test]
    fn rejects_invalid_checksum() {
        let c = LuhnClassifier::new();
        // 4532015112830367 — last digit zmieniony (oryginał kończył się na 6).
        let matches = c.scan("4532015112830367");
        assert!(matches.is_empty(), "checksum-invalid card not matched");
    }

    #[test]
    fn rejects_too_short() {
        let c = LuhnClassifier::new();
        let matches = c.scan("12345");
        assert!(matches.is_empty());
    }

    #[test]
    fn rejects_too_long() {
        let c = LuhnClassifier::new();
        let matches = c.scan("12345678901234567890");
        assert!(matches.is_empty());
    }

    #[test]
    fn detects_multiple_in_one_text() {
        let c = LuhnClassifier::new();
        // 2x Visa 4532015112830366 oddzielone tekstem.
        let text = "primary 4532015112830366 backup 4485275742308327";
        let matches = c.scan(text);
        assert_eq!(matches.len(), 2);
    }

    #[test]
    fn no_match_in_plain_text() {
        let c = LuhnClassifier::new();
        let matches = c.scan("Hello world, nothing sensitive here.");
        assert!(matches.is_empty());
    }

    #[test]
    fn luhn_check_known_valid() {
        assert!(luhn_check("4532015112830366"));
        assert!(luhn_check("5500000000000004"));
        assert!(luhn_check("378282246310005"));
    }

    #[test]
    fn luhn_check_known_invalid() {
        assert!(!luhn_check("4532015112830367"));
        assert!(!luhn_check("1234567890123456"));
    }

    #[test]
    fn redact_format_consistent() {
        assert_eq!(redact("4532015112830366"), "**** **** **** 0366");
        assert_eq!(redact("378282246310005"), "**** **** **** 0005");
    }
}
