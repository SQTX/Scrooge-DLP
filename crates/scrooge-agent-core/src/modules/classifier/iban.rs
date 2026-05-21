// SPDX-License-Identifier: GPL-2.0-only
//
// ScroogeDLP - Data Loss Prevention system
// Copyright (C) 2026 SQTX <sssqtx@gmail.com>

//! `IbanClassifier` — wykrywa IBAN (ISO 13616).
//!
//! IBAN: 2 litery (country) + 2 cyfry (check) + alphanumeric BBAN (do 30 chars).
//! Polski IBAN = 28 chars (PL + 26 cyfr).
//!
//! Walidacja:
//! 1. Usuń spacje.
//! 2. Sprawdź długość per-country (tu: 15-34 chars).
//! 3. Przenieś 4 pierwsze chars na koniec.
//! 4. Litera → 2 cyfry (A=10, B=11, …, Z=35).
//! 5. Wynikowy ogromny integer mod 97 == 1.
//!
//! Redact: `PL...XXXX` — country + 4 ostatnie znaki, środek gwiazdki.

use regex::Regex;

use super::{Classifier, Match};

pub const NAME: &str = "iban";

/// IBAN może mieć spacje co 4 znaki. Regex tolerant: 2 litery + 2 cyfry +
/// alphanumeric z opcjonalnymi spacjami, do 34 chars effective.
const IBAN_REGEX: &str = r"\b[A-Z]{2}\d{2}(?:[ ]?[A-Z0-9]){11,30}\b";

#[derive(Debug)]
pub struct IbanClassifier {
    re: Regex,
}

impl Default for IbanClassifier {
    fn default() -> Self {
        Self::new()
    }
}

impl IbanClassifier {
    #[must_use]
    pub fn new() -> Self {
        Self {
            re: Regex::new(IBAN_REGEX).expect("IBAN_REGEX musi się kompilować"),
        }
    }
}

impl Classifier for IbanClassifier {
    fn name(&self) -> &str {
        NAME
    }

    fn scan(&self, text: &str) -> Vec<Match> {
        let mut out = Vec::new();
        for m in self.re.find_iter(text) {
            let raw = m.as_str();
            let stripped: String = raw.chars().filter(|c| !c.is_whitespace()).collect();
            if !(15..=34).contains(&stripped.len()) {
                continue;
            }
            if !iban_check(&stripped) {
                continue;
            }
            out.push(Match {
                classifier: NAME.to_string(),
                start: m.start(),
                end: m.end(),
                redacted: redact(&stripped),
                last4: stripped[stripped.len() - 4..].to_string(),
            });
        }
        out
    }
}

/// Mod-97 walidacja. `iban` = ciągłe znaki bez spacji, uppercase.
fn iban_check(iban: &str) -> bool {
    // Move first 4 chars to end.
    let (head, tail) = iban.split_at(4);
    let rearranged: String = format!("{tail}{head}");

    // Convert: letter → 2 digits (A=10, …, Z=35). Iteracyjny mod 97
    // żeby uniknąć big-int (BigInt nie ma w stdlib, można by dorzucić
    // crate, ale iteracyjny mod jest standardowym sposobem dla IBAN).
    let mut remainder: u64 = 0;
    for c in rearranged.chars() {
        let val = if c.is_ascii_digit() {
            u64::from(c as u8 - b'0')
        } else if c.is_ascii_uppercase() {
            u64::from(c as u8 - b'A') + 10
        } else {
            return false; // nieoczekiwany znak
        };
        // Każda iteracja: remainder = (remainder * base + val) mod 97
        // base = 10 dla cyfry, 100 dla litery (bo 2 cyfry).
        if val < 10 {
            remainder = (remainder * 10 + val) % 97;
        } else {
            remainder = (remainder * 100 + val) % 97;
        }
    }
    remainder == 1
}

fn redact(iban: &str) -> String {
    let last4 = &iban[iban.len() - 4..];
    let country = &iban[..2];
    format!("{country}**...{last4}")
}

// ──────────────────────────────────────────────────────────────────────────
// Testy
// ──────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_valid_polish_iban() {
        let c = IbanClassifier::new();
        // PL61 1090 1014 0000 0712 1981 2874 — przykład z dokumentów PL.
        let matches = c.scan("Konto: PL61109010140000071219812874");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].classifier, "iban");
        assert_eq!(matches[0].last4, "2874");
    }

    #[test]
    fn detects_with_spaces() {
        let c = IbanClassifier::new();
        let matches = c.scan("PL61 1090 1014 0000 0712 1981 2874");
        assert_eq!(matches.len(), 1);
    }

    #[test]
    fn detects_de_iban() {
        let c = IbanClassifier::new();
        // DE89370400440532013000 — Deutsche Bank test IBAN.
        let matches = c.scan("DE89370400440532013000");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].last4, "3000");
    }

    #[test]
    fn rejects_invalid_checksum() {
        let c = IbanClassifier::new();
        // PL62... — zmieniony check digit (oryginał PL61).
        let matches = c.scan("PL62109010140000071219812874");
        assert!(matches.is_empty());
    }

    #[test]
    fn redact_shows_country_and_last4() {
        assert_eq!(redact("PL61109010140000071219812874"), "PL**...2874");
        assert_eq!(redact("DE89370400440532013000"), "DE**...3000");
    }

    #[test]
    fn iban_check_table() {
        assert!(iban_check("PL61109010140000071219812874"));
        assert!(iban_check("DE89370400440532013000"));
        assert!(iban_check("GB82WEST12345698765432")); // GB test IBAN
        assert!(!iban_check("PL62109010140000071219812874"));
    }

    #[test]
    fn no_match_plain_text() {
        let c = IbanClassifier::new();
        assert!(c.scan("Hello world").is_empty());
        assert!(c.scan("XX12 not valid").is_empty());
    }
}
