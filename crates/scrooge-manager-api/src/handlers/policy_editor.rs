// SPDX-License-Identifier: GPL-2.0-only
//
// ScroogeDLP - Data Loss Prevention system
// Copyright (C) 2026 SQTX <sssqtx@gmail.com>

//! Serwowanie wydzielonego policy editora (vanilla JS module).
//!
//! Pliki są osadzone przy compile time przez `include_str!` — manager pozostaje
//! self-contained (zero filesystem deps), spójnie z `DASHBOARD_HTML` w `lib.rs`.

use axum::{
    extract::Path,
    http::{header, StatusCode},
    response::{IntoResponse, Response},
};

// Module entry + root controller.
const INDEX_JS: &str = include_str!("../../static/policy-editor/index.js");
const POLICY_EDITOR_JS: &str = include_str!("../../static/policy-editor/PolicyEditor.js");

// Modes.
const YAML_MODE_JS: &str = include_str!("../../static/policy-editor/modes/YamlMode.js");
const FORM_MODE_JS: &str = include_str!("../../static/policy-editor/modes/FormMode.js");

// UI helpers.
const VALIDATION_PANEL_JS: &str =
    include_str!("../../static/policy-editor/ui/ValidationPanel.js");
const MODE_SWITCH_JS: &str = include_str!("../../static/policy-editor/ui/ModeSwitch.js");
const CONFIRM_DIALOG_JS: &str =
    include_str!("../../static/policy-editor/ui/ConfirmDialog.js");
const HELP_TIP_JS: &str = include_str!("../../static/policy-editor/ui/HelpTip.js");

// Codec.
const FORM_TO_YAML_JS: &str = include_str!("../../static/policy-editor/codec/formToYaml.js");
const YAML_TO_FORM_JS: &str = include_str!("../../static/policy-editor/codec/yamlToForm.js");
const SUPPORTABILITY_JS: &str =
    include_str!("../../static/policy-editor/codec/supportability.js");

// Form sections.
const METADATA_SECTION_JS: &str =
    include_str!("../../static/policy-editor/sections/MetadataSection.js");
const RULE_SECTION_JS: &str =
    include_str!("../../static/policy-editor/sections/RuleSection.js");
const TARGETS_SECTION_JS: &str =
    include_str!("../../static/policy-editor/sections/TargetsSection.js");
const SOURCE_LIST_EDITOR_JS: &str =
    include_str!("../../static/policy-editor/sections/SourceListEditor.js");
const DESTINATION_LIST_EDITOR_JS: &str =
    include_str!("../../static/policy-editor/sections/DestinationListEditor.js");
const CONDITIONS_SECTION_JS: &str =
    include_str!("../../static/policy-editor/sections/ConditionsSection.js");

// API wrappers.
const API_POLICIES_JS: &str = include_str!("../../static/policy-editor/api/policies.js");
const API_AUTH_JS: &str = include_str!("../../static/policy-editor/api/auth.js");

/// `GET /static/policy-editor/*path` — serwuje embedded JS module.
///
/// Whitelist'a ścieżek (match na pełnym `path`). Nieznane ścieżki → 404.
/// Wszystkie odpowiedzi mają `Content-Type: application/javascript` —
/// browsery wymagają tego dla `<script type="module">`.
pub async fn asset(Path(path): Path<String>) -> Response {
    let body: &'static str = match path.as_str() {
        "index.js" => INDEX_JS,
        "PolicyEditor.js" => POLICY_EDITOR_JS,
        "modes/YamlMode.js" => YAML_MODE_JS,
        "modes/FormMode.js" => FORM_MODE_JS,
        "ui/ValidationPanel.js" => VALIDATION_PANEL_JS,
        "ui/ModeSwitch.js" => MODE_SWITCH_JS,
        "ui/ConfirmDialog.js" => CONFIRM_DIALOG_JS,
        "ui/HelpTip.js" => HELP_TIP_JS,
        "codec/formToYaml.js" => FORM_TO_YAML_JS,
        "codec/yamlToForm.js" => YAML_TO_FORM_JS,
        "codec/supportability.js" => SUPPORTABILITY_JS,
        "sections/MetadataSection.js" => METADATA_SECTION_JS,
        "sections/RuleSection.js" => RULE_SECTION_JS,
        "sections/TargetsSection.js" => TARGETS_SECTION_JS,
        "sections/SourceListEditor.js" => SOURCE_LIST_EDITOR_JS,
        "sections/DestinationListEditor.js" => DESTINATION_LIST_EDITOR_JS,
        "sections/ConditionsSection.js" => CONDITIONS_SECTION_JS,
        "api/policies.js" => API_POLICIES_JS,
        "api/auth.js" => API_AUTH_JS,
        _ => return (StatusCode::NOT_FOUND, "not found").into_response(),
    };
    (
        [(
            header::CONTENT_TYPE,
            "application/javascript; charset=utf-8",
        )],
        body,
    )
        .into_response()
}
