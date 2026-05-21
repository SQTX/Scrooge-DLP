// SPDX-License-Identifier: GPL-2.0-only
//
// ScroogeDLP - Data Loss Prevention system
// Copyright (C) 2026 SQTX <sssqtx@gmail.com>

//! Klasyfikatory treści — wykrywają wrażliwe dane (numery kart, PESEL, IBAN…)
//! w tekście wynoszonym z endpointu (clipboard, file content, network payload).
//!
//! Wszystkie klasyfikatory są **pure functions** — `scan(&str) -> Vec<Match>`
//! bez I/O, bez state. Łatwe do testowania, bezpieczne thread-safe.
//!
//! Phase 3 MVP: 1 klasyfikator (`LuhnClassifier`). Reszta (PESEL, IBAN, NIP)
//! dochodzi iteracyjnie.

pub mod luhn;

use std::sync::Arc;

/// Pojedyncze trafienie klasyfikatora w skanowanej zawartości.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Match {
    /// Id klasyfikatora który zmatchował (np. `"credit_card"`).
    pub classifier: String,
    /// Pozycja w wejściowym tekście (byte offsets, jak `regex::Match`).
    pub start: usize,
    pub end: usize,
    /// Bezpieczna do zalogowania reprezentacja — np. `"**** **** **** 9010"`.
    /// NIGDY nie zawiera pełnego sekretu.
    pub redacted: String,
    /// Ostatnie 4 cyfry (do dedup po stronie UI) — częściowe ujawnienie,
    /// kompromis między użytecznością a privacy.
    pub last4: String,
}

/// Trait implementowany przez każdy klasyfikator.
///
/// Implementacje MUSZĄ być `Send + Sync + Debug` — registry trzyma wszystkie
/// klasyfikatory za `Arc` i wywołuje `scan()` w tle z różnych task'ów; Debug
/// pozwala tracing'owi dump'ować registry w diagnostyce.
pub trait Classifier: Send + Sync + std::fmt::Debug {
    /// Stabilna nazwa (slug) — używana w event payload i konfiguracji polityk.
    fn name(&self) -> &str;

    /// Skanuje tekst, zwraca wszystkie trafienia. Może być pusty.
    fn scan(&self, text: &str) -> Vec<Match>;
}

/// Rejestr klasyfikatorów — agent ładuje jego instancję raz przy starcie
/// i przekazuje do modułów które skanują treść (clipboard, file content).
#[derive(Debug, Default)]
pub struct ClassifierRegistry {
    items: Vec<Arc<dyn Classifier>>,
}

impl ClassifierRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Tworzy registry z domyślnymi klasyfikatorami dla Phase 3 MVP.
    /// Obecnie: tylko Luhn (karty kredytowe).
    #[must_use]
    pub fn with_defaults() -> Self {
        let mut r = Self::new();
        r.register(Arc::new(luhn::LuhnClassifier::new()));
        r
    }

    pub fn register(&mut self, c: Arc<dyn Classifier>) {
        self.items.push(c);
    }

    /// Uruchamia wszystkie klasyfikatory na tekście, agreguje matches.
    pub fn scan_all(&self, text: &str) -> Vec<Match> {
        let mut out = Vec::new();
        for c in &self.items {
            out.extend(c.scan(text));
        }
        out
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.items.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}
