// SPDX-License-Identifier: GPL-2.0-only
//
// ScroogeDLP - Data Loss Prevention system
// Copyright (C) 2026 SQTX <sssqtx@gmail.com>
//
// This program is free software; you can redistribute it and/or modify
// it under the terms of the GNU General Public License version 2 as
// published by the Free Software Foundation.

//! Wewnętrzne root CA dla ScroogeDLP.
//!
//! Manager generuje root CA przy quickstart (lub wczytuje istniejące),
//! a następnie podpisuje CSR-e przesyłane przez agentów w trakcie enrollment.
//!
//! API jest synchroniczne — operacje CA są rzadkie (jednorazowy bootstrap
//! + jedno podpisanie per enrollment), nie ma sensu zaciągać tokio.
//!
//! # Cykl życia
//!
//! ```no_run
//! use scrooge_common::ca::RootCa;
//!
//! // Quickstart - wygenerowanie nowego CA i zapis na dysk.
//! let ca = RootCa::generate("ScroogeDLP Root CA").unwrap();
//! ca.save_to_files("/etc/scrooge/ca.pem", "/etc/scrooge/ca.key").unwrap();
//!
//! // Manager startup - wczytanie istniejącego CA.
//! let ca = RootCa::load_from_files(
//!     "/etc/scrooge/ca.pem",
//!     "/etc/scrooge/ca.key",
//! ).unwrap();
//!
//! // Enrollment - podpisanie CSR przesłanego przez agenta.
//! let signed_pem = ca.sign_csr("-----BEGIN CERTIFICATE REQUEST-----\n...").unwrap();
//! ```

use std::path::Path;

use rcgen::{
    BasicConstraints, Certificate, CertificateParams, CertificateSigningRequestParams,
    DistinguishedName, DnType, ExtendedKeyUsagePurpose, IsCa, KeyPair, KeyUsagePurpose, SanType,
};

use crate::error::CaError;

/// Root CA managera — cert + key pair w pamięci.
pub struct RootCa {
    cert: Certificate,
    key_pair: KeyPair,
    cert_pem: String,
    key_pem: String,
}

impl RootCa {
    /// Generuje nowe self-signed root CA z `subject_cn` jako Common Name.
    ///
    /// Algorytm: ECDSA P-256 / SHA-256 (default rcgen — szybkie i wystarczające).
    pub fn generate(subject_cn: &str) -> Result<Self, CaError> {
        let mut params = CertificateParams::new(Vec::<String>::new())?;
        params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        params.key_usages = vec![
            KeyUsagePurpose::KeyCertSign,
            KeyUsagePurpose::CrlSign,
            KeyUsagePurpose::DigitalSignature,
        ];

        let mut dn = DistinguishedName::new();
        dn.push(DnType::CommonName, subject_cn);
        dn.push(DnType::OrganizationName, "ScroogeDLP");
        params.distinguished_name = dn;

        let key_pair = KeyPair::generate()?;
        let cert = params.self_signed(&key_pair)?;
        let cert_pem = cert.pem();
        let key_pem = key_pair.serialize_pem();

        Ok(Self {
            cert,
            key_pair,
            cert_pem,
            key_pem,
        })
    }

    /// Wczytuje istniejące CA z dwóch plików PEM (cert + private key).
    ///
    /// **Uwaga implementacyjna:** rcgen 0.13 nie roundtrip-uje X.509 bajt-w-bajt
    /// po reloadzie. Wczytywany jest klucz prywatny + params z cert PEM, a cert
    /// jest re-issuowany z tymi samymi DN i kluczem — cryptographic identity
    /// (klucz publiczny + DN) zostaje zachowana, więc podpisywanie CSR działa
    /// tak samo. Klienci pinujący do public key nie zauważają różnicy.
    pub fn load_from_files(
        cert_path: impl AsRef<Path>,
        key_path: impl AsRef<Path>,
    ) -> Result<Self, CaError> {
        let cert_path = cert_path.as_ref();
        let key_path = key_path.as_ref();

        let cert_pem = std::fs::read_to_string(cert_path).map_err(|source| CaError::Io {
            path: cert_path.display().to_string(),
            source,
        })?;
        let key_pem = std::fs::read_to_string(key_path).map_err(|source| CaError::Io {
            path: key_path.display().to_string(),
            source,
        })?;

        let key_pair = KeyPair::from_pem(&key_pem)?;
        let params = CertificateParams::from_ca_cert_pem(&cert_pem)?;
        let cert = params.self_signed(&key_pair)?;
        let cert_pem_reissued = cert.pem();

        Ok(Self {
            cert,
            key_pair,
            cert_pem: cert_pem_reissued,
            key_pem,
        })
    }

    /// Zapisuje cert PEM i private key PEM do podanych plików.
    pub fn save_to_files(
        &self,
        cert_path: impl AsRef<Path>,
        key_path: impl AsRef<Path>,
    ) -> Result<(), CaError> {
        let cert_path = cert_path.as_ref();
        let key_path = key_path.as_ref();

        std::fs::write(cert_path, &self.cert_pem).map_err(|source| CaError::Io {
            path: cert_path.display().to_string(),
            source,
        })?;
        std::fs::write(key_path, &self.key_pem).map_err(|source| CaError::Io {
            path: key_path.display().to_string(),
            source,
        })?;
        Ok(())
    }

    /// Cert root CA jako PEM string (do wysyłki do agenta w `EnrollResponse.ca_chain_pem`).
    #[must_use]
    pub fn cert_pem(&self) -> &str {
        &self.cert_pem
    }

    /// Podpisuje CSR przesłany przez agenta.
    ///
    /// `csr_pem` musi być PEM-encoded PKCS#10 CertificateSigningRequest.
    /// Zwraca PEM podpisanego cert agenta. Subject CN pozostaje taki jaki
    /// agent wpisał w CSR (zwykle hostname).
    ///
    /// **Dla `Enroll` RPC używaj [`Self::sign_csr_with_cn`]** — manager
    /// jest autorytatywnym źródłem `agent_id` (UUID) i powinien wymusić
    /// CN w wystawianym cercie, ignorując co agent wpisał w CSR.
    pub fn sign_csr(&self, csr_pem: &str) -> Result<String, CaError> {
        let csr = CertificateSigningRequestParams::from_pem(csr_pem)?;
        let signed = csr.signed_by(&self.cert, &self.key_pair)?;
        Ok(signed.pem())
    }

    /// Podpisuje CSR z **wymuszonym Subject CN** (= `agent_id` UUID).
    ///
    /// Manager wywołuje to przy `Enroll` RPC — agent_id generowany jest
    /// server-side po walidacji tokena, agent nie wie go zanim odbierze
    /// `EnrollResponse`. CN agenta w CSR (zwykle hostname) jest celowo
    /// nadpisywany, żeby manager `extract_agent_id` z mTLS peer cert
    /// dostał deterministycznie UUID.
    pub fn sign_csr_with_cn(&self, csr_pem: &str, subject_cn: &str) -> Result<String, CaError> {
        let mut csr = CertificateSigningRequestParams::from_pem(csr_pem)?;
        let mut dn = DistinguishedName::new();
        dn.push(DnType::CommonName, subject_cn);
        dn.push(DnType::OrganizationName, "ScroogeDLP Agent");
        csr.params.distinguished_name = dn;
        let signed = csr.signed_by(&self.cert, &self.key_pair)?;
        Ok(signed.pem())
    }
}

/// Generuje keypair + CSR (PEM) dla managera (server cert podpisywany przez
/// własne root CA przy quickstart).
///
/// `sans` zawiera listę nazw — DNS-like (np. `"localhost"`, `"scrooge-manager"`)
/// trafia do SAN.DnsName, a parsowalne IP (np. `"127.0.0.1"`) do SAN.IpAddress.
///
/// Zwraca `(csr_pem, key_pem)`. Cert podpisuje się potem przez [`RootCa::sign_csr`].
pub fn generate_server_csr(
    common_name: &str,
    sans: &[String],
) -> Result<(String, String), CaError> {
    let key_pair = KeyPair::generate()?;
    let (dns, ips): (Vec<_>, Vec<_>) = sans
        .iter()
        .partition(|s| s.parse::<std::net::IpAddr>().is_err());

    let mut params = CertificateParams::new(dns.into_iter().cloned().collect::<Vec<_>>())?;
    for ip in ips {
        if let Ok(addr) = ip.parse::<std::net::IpAddr>() {
            params.subject_alt_names.push(SanType::IpAddress(addr));
        }
    }

    let mut dn = DistinguishedName::new();
    dn.push(DnType::CommonName, common_name);
    dn.push(DnType::OrganizationName, "ScroogeDLP");
    params.distinguished_name = dn;
    params.key_usages = vec![
        KeyUsagePurpose::DigitalSignature,
        KeyUsagePurpose::KeyEncipherment,
    ];
    params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];

    let csr = params.serialize_request(&key_pair)?;
    let csr_pem = csr.pem()?;
    let key_pem = key_pair.serialize_pem();
    Ok((csr_pem, key_pem))
}

/// Generuje keypair + CSR (PEM) dla agenta enrollment'u.
///
/// Zwraca `(csr_pem, key_pem)`. Klucz prywatny musi zostać zapisany lokalnie
/// przez agenta — CSR przesyłany do managera, gdzie [`RootCa::sign_csr`] zwraca
/// podpisany cert.
pub fn generate_agent_csr(common_name: &str) -> Result<(String, String), CaError> {
    let key_pair = KeyPair::generate()?;
    let mut params = CertificateParams::new(vec![common_name.to_string()])?;
    let mut dn = DistinguishedName::new();
    dn.push(DnType::CommonName, common_name);
    dn.push(DnType::OrganizationName, "ScroogeDLP Agent");
    params.distinguished_name = dn;

    let csr = params.serialize_request(&key_pair)?;
    let csr_pem = csr.pem()?;
    let key_pem = key_pair.serialize_pem();
    Ok((csr_pem, key_pem))
}

impl std::fmt::Debug for RootCa {
    // Nie eksponujemy klucza prywatnego w Debug - bezpieczeństwo trumphs over
    // debug ergonomics.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RootCa")
            .field("cert_pem_len", &self.cert_pem.len())
            .finish_non_exhaustive()
    }
}

// ──────────────────────────────────────────────────────────────────────────
// Tests
// ──────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Wygenerowane CA ma niepusty cert PEM i można je zapisać do plików.
    #[test]
    fn generates_root_ca() {
        let ca = RootCa::generate("Test Root CA").unwrap();
        assert!(ca.cert_pem().contains("-----BEGIN CERTIFICATE-----"));
        assert!(ca.cert_pem().contains("-----END CERTIFICATE-----"));
    }

    /// Round-trip: generate → save → load → sign CSR.
    #[test]
    fn round_trip_save_load_and_sign_csr() {
        let dir = tempfile::tempdir().unwrap();
        let cert_path = dir.path().join("ca.pem");
        let key_path = dir.path().join("ca.key");

        // Generate + save.
        let ca_gen = RootCa::generate("Roundtrip CA").unwrap();
        ca_gen.save_to_files(&cert_path, &key_path).unwrap();

        // Load.
        let ca_loaded = RootCa::load_from_files(&cert_path, &key_path).unwrap();
        assert!(ca_loaded.cert_pem().contains("-----BEGIN CERTIFICATE-----"));

        // Generate a CSR jakby był od agenta i podpisz przez loaded CA.
        let agent_key = KeyPair::generate().unwrap();
        let agent_params = CertificateParams::new(vec!["test-agent.local".to_string()]).unwrap();
        let csr_pem = agent_params
            .serialize_request(&agent_key)
            .unwrap()
            .pem()
            .unwrap();

        let signed_pem = ca_loaded.sign_csr(&csr_pem).unwrap();
        assert!(signed_pem.contains("-----BEGIN CERTIFICATE-----"));
        assert!(signed_pem.contains("-----END CERTIFICATE-----"));
    }

    #[test]
    fn load_fails_on_missing_file() {
        let err =
            RootCa::load_from_files("/nonexistent/cert.pem", "/nonexistent/key.pem").unwrap_err();
        assert!(matches!(err, CaError::Io { .. }));
    }

    #[test]
    fn sign_csr_fails_on_invalid_pem() {
        let ca = RootCa::generate("Test CA").unwrap();
        let err = ca.sign_csr("not a real CSR").unwrap_err();
        assert!(matches!(err, CaError::Rcgen(_)));
    }

    #[test]
    fn debug_does_not_leak_key() {
        let ca = RootCa::generate("Secret CA").unwrap();
        let dbg = format!("{ca:?}");
        assert!(!dbg.contains("PRIVATE KEY"));
        assert!(!dbg.contains("BEGIN"));
    }
}
