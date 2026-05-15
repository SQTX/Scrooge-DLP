// SPDX-License-Identifier: GPL-2.0-only
//
// ScroogeDLP - Data Loss Prevention system
// Copyright (C) 2026 SQTX <sssqtx@gmail.com>

//! Unit testy parser'a + walidatora + compile'a polityk.

use super::{
    rules::{parse_size, Action, Severity, Source},
    *,
};

/// Sample policy z PROJECT_BRIEF.md (finance-strict, skrót).
const SAMPLE_VALID: &str = r#"
apiVersion: scroogedlp.io/v1
kind: Policy
metadata:
  name: finance-strict
  description: "Strict policy for finance department"
  version: 12

targets:
  match:
    os: ["windows", "macos"]
    tags:
      department: finance
  exclude:
    hostnames: ["finance-test-vm-01"]

priority: 100

rules:
  outbound:
    - id: finance-no-usb-export
      name: "Block finance files copied to USB"
      enabled: true
      severity: high
      sources:
        - type: directory
          paths: ["C:\\Finance\\**"]
          recursive: true
      destinations:
        - type: usb
          except_serials: ["AA1234567890"]
        - type: clipboard
      conditions:
        file_size_min: "1KB"
        file_extensions: ["xlsx", "pdf"]
        filename_regex: '^.*\.(xlsx|pdf)$'
      action: block
      notify_user: true
      message: "Kopiowanie plikow finansowych poza firme jest zabronione"
      forward_to_siem: true

monitored_paths:
  - path: "C:\\Finance"
    recursive: true
    events: [create, modify, delete]
    file_patterns:
      include: ["*.xlsx", "*.docx", "*.pdf"]
      exclude: ["~$*", "*.tmp"]

classifiers:
  - credit-card-numbers
  - polish-pesel
"#;

#[test]
fn parse_valid_policy_succeeds() {
    let p = Policy::from_yaml(SAMPLE_VALID).expect("parsing valid policy");
    assert_eq!(p.metadata.name, "finance-strict");
    assert_eq!(p.metadata.version, 12);
    assert_eq!(p.priority, 100);
    assert_eq!(p.rules.outbound.len(), 1);
    assert!(p.rules.inbound.is_empty());
    assert_eq!(p.classifiers.len(), 2);
}

#[test]
fn rule_action_severity_parsed() {
    let p = Policy::from_yaml(SAMPLE_VALID).unwrap();
    let rule = &p.rules.outbound[0];
    assert_eq!(rule.severity, Severity::High);
    assert_eq!(rule.action, Action::Block);
    assert!(rule.notify_user);
    assert!(rule.forward_to_siem);
}

#[test]
fn rule_sources_tagged_correctly() {
    let p = Policy::from_yaml(SAMPLE_VALID).unwrap();
    let rule = &p.rules.outbound[0];
    match &rule.sources[0] {
        Source::Directory { paths, recursive } => {
            assert_eq!(paths, &vec!["C:\\Finance\\**".to_string()]);
            assert!(*recursive);
        },
        other => panic!("expected Directory source, got {other:?}"),
    }
}

#[test]
fn missing_api_version_fails() {
    let yaml = r"
kind: Policy
metadata:
  name: test
";
    let err = Policy::from_yaml(yaml).unwrap_err();
    // serde wymaga apiVersion bo nie ma default — to YAML parse error
    assert!(
        matches!(err, PolicyError::Yaml(_)),
        "expected YAML error, got {err:?}"
    );
}

#[test]
fn wrong_api_version_fails_validation() {
    let yaml = r"
apiVersion: scroogedlp.io/v2
kind: Policy
metadata:
  name: test
";
    let err = Policy::from_yaml(yaml).unwrap_err();
    assert!(
        matches!(err, PolicyError::Validation(ref s) if s.contains("apiVersion")),
        "expected Validation error mentioning apiVersion, got {err:?}"
    );
}

#[test]
fn duplicate_rule_id_rejected() {
    let yaml = r#"
apiVersion: scroogedlp.io/v1
kind: Policy
metadata:
  name: dup-test
rules:
  outbound:
    - id: r1
      name: "First"
      severity: low
      action: log_only
    - id: r1
      name: "Second (duplicate)"
      severity: low
      action: log_only
"#;
    let err = Policy::from_yaml(yaml).unwrap_err();
    assert!(
        matches!(err, PolicyError::Validation(ref s) if s.contains("duplicate rule id")),
        "expected duplicate id error, got {err:?}"
    );
}

#[test]
fn invalid_regex_rejected() {
    let yaml = r#"
apiVersion: scroogedlp.io/v1
kind: Policy
metadata:
  name: bad-regex
rules:
  outbound:
    - id: r1
      name: "Bad regex"
      severity: low
      action: log_only
      conditions:
        filename_regex: '[unclosed'
"#;
    let err = Policy::from_yaml(yaml).unwrap_err();
    assert!(
        matches!(err, PolicyError::RegexCompile { .. }),
        "expected RegexCompile error, got {err:?}"
    );
}

#[test]
fn invalid_name_slug_rejected() {
    let yaml = r"
apiVersion: scroogedlp.io/v1
kind: Policy
metadata:
  name: Bad_Name_With_Underscores
";
    let err = Policy::from_yaml(yaml).unwrap_err();
    assert!(
        matches!(err, PolicyError::Validation(ref s) if s.contains("slug-style")),
        "expected slug validation error, got {err:?}"
    );
}

#[test]
fn parse_size_units() {
    assert_eq!(parse_size("1024").unwrap(), 1024);
    assert_eq!(parse_size("1KB").unwrap(), 1024);
    assert_eq!(parse_size("2 MB").unwrap(), 2 * 1024 * 1024);
    assert_eq!(parse_size("1GB").unwrap(), 1024 * 1024 * 1024);
    assert!(parse_size("1XB").is_err());
}

#[test]
fn compile_to_msgpack_then_decode_roundtrip() {
    let original = Policy::from_yaml(SAMPLE_VALID).unwrap();
    let compiled = original.compile().expect("compile");
    assert_eq!(compiled.name, "finance-strict");
    assert_eq!(compiled.version, 12);
    assert_eq!(compiled.hash.len(), 64, "SHA-256 hex = 64 chars");
    assert!(compiled.hash.chars().all(|c| c.is_ascii_hexdigit()));
    assert!(!compiled.msgpack.is_empty());

    // Round-trip: msgpack → Policy musi dać identyczny obiekt
    let decoded = compiled.decode().expect("decode");
    assert_eq!(decoded, original);
}

#[test]
fn compile_is_deterministic() {
    // Ten sam Policy musi dać identyczny hash przy każdym compile.
    let p1 = Policy::from_yaml(SAMPLE_VALID).unwrap();
    let p2 = Policy::from_yaml(SAMPLE_VALID).unwrap();
    let c1 = p1.compile().unwrap();
    let c2 = p2.compile().unwrap();
    assert_eq!(c1.hash, c2.hash);
    assert_eq!(c1.msgpack, c2.msgpack);
}

#[test]
fn compile_hash_changes_when_policy_changes() {
    let p1 = Policy::from_yaml(SAMPLE_VALID).unwrap();
    let mut p2 = p1.clone();
    p2.metadata.version = 13; // bump

    let h1 = p1.compile().unwrap().hash;
    let h2 = p2.compile().unwrap().hash;
    assert_ne!(h1, h2, "różne version → różne hash");
}

#[test]
fn minimal_policy_with_defaults() {
    // Najmniejszy poprawny policy — same wymagane pola.
    let yaml = r"
apiVersion: scroogedlp.io/v1
kind: Policy
metadata:
  name: minimal
";
    let p = Policy::from_yaml(yaml).expect("minimal policy");
    assert_eq!(p.metadata.version, 1); // default
    assert_eq!(p.priority, 0); // default
    assert!(p.rules.inbound.is_empty());
    assert!(p.rules.outbound.is_empty());
    assert!(p.classifiers.is_empty());
}
