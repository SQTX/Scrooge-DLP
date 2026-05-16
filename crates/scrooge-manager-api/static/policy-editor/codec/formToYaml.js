// SPDX-License-Identifier: GPL-2.0-only
//
// Deterministyczny generator YAMLa ze stanu form mode'a. Manualne stringi,
// zero zewnętrznych libów. Kolejność pól ustalona — ten sam formState
// zawsze daje ten sam YAML (bit-equal).
//
// Sub-faza 2F.4: dodano emit'owanie rule (id, name, severity, action,
// message, notify_user, forward_to_siem). Sources/destinations/conditions
// wjadą w 2F.5-7.

import { defaultRuleState } from '../sections/RuleSection.js';

const DEFAULT_FORM_STATE = {
  metadata: { name: 'my-policy', description: '', priority: 100 },
  rule: defaultRuleState(),
  sources: [],
  destinations: [],
  conditions: null,
};

/**
 * Zwraca świeży default stanu form mode'a (używane gdy otwieramy "+ New policy").
 */
export function defaultFormState() {
  return JSON.parse(JSON.stringify(DEFAULT_FORM_STATE));
}

/**
 * @param {object} formState — patrz DEFAULT_FORM_STATE.
 * @param {string|null} ghostYaml — opcjonalny "raw YAML" z unsupported features
 *        z poprzedniego YAML→Form switcha (do zachowania przez Save).
 *        W obecnej skeleton wersji nie używany.
 * @returns {string} YAML
 */
export function formToYaml(formState, _ghostYaml = null) {
  const md = formState.metadata ?? {};
  const lines = [
    'apiVersion: scroogedlp.io/v1',
    'kind: Policy',
    'metadata:',
    `  name: ${yamlString(md.name ?? 'my-policy')}`,
  ];
  if (md.description) {
    lines.push(`  description: ${yamlString(md.description)}`);
  }
  lines.push(`priority: ${Number.isFinite(md.priority) ? md.priority : 100}`);

  // ── Rule ─────────────────────────────────────────────────────────────
  const r = formState.rule;
  if (r && r.id) {
    lines.push('rules:');
    lines.push('  outbound:');
    lines.push(`    - id: ${yamlString(r.id)}`);
    if (r.name) lines.push(`      name: ${yamlString(r.name)}`);
    lines.push(`      severity: ${r.severity ?? 'medium'}`);
    lines.push(`      action: ${r.action ?? 'log_only'}`);
    if (r.message) lines.push(`      message: ${yamlString(r.message)}`);
    if (r.notify_user) lines.push('      notify_user: true');
    if (r.forward_to_siem) lines.push('      forward_to_siem: true');
    // TODO(2F.5-7): sources / destinations / conditions
  }

  return lines.join('\n') + '\n';
}

/**
 * Cytuje string YAML-bezpiecznie. Konserwatywnie — zawsze "double-quote" gdy
 * zawiera znaki specjalne lub spacje; goły identifier inaczej.
 */
function yamlString(s) {
  if (typeof s !== 'string' || s === '') return '""';
  // Bezpieczne goły: tylko [a-zA-Z0-9_-./]
  if (/^[A-Za-z0-9_./-]+$/.test(s)) return s;
  // Quote'uj — escape " i \
  const escaped = s.replace(/\\/g, '\\\\').replace(/"/g, '\\"');
  return `"${escaped}"`;
}
