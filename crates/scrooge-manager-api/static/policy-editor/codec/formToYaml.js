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

    // Sources / destinations żyją na poziomie formState, nie rule —
    // ale w YAMLu wjeżdżają wewnątrz rules.outbound[0]. Emit'ujemy tutaj.
    emitSources(lines, formState.sources ?? []);
    emitDestinations(lines, formState.destinations ?? []);

    // TODO(2F.7): conditions
  }

  return lines.join('\n') + '\n';
}

function emitSources(lines, sources) {
  if (!sources.length) return;
  lines.push('      sources:');
  for (const src of sources) emitSourceLike(lines, src, 8);
}

function emitDestinations(lines, dests) {
  if (!dests.length) return;
  lines.push('      destinations:');
  for (const d of dests) emitSourceLike(lines, d, 8);
}

/**
 * Source i Destination mają identyczny YAML shape: `- type: X` plus pola.
 * Wspólny helper. `indent` = liczba spacji przed `-`.
 */
function emitSourceLike(lines, item, indent) {
  const pad = ' '.repeat(indent);
  const pad2 = ' '.repeat(indent + 2);
  lines.push(`${pad}- type: ${item.type}`);
  for (const [k, v] of Object.entries(item)) {
    if (k === 'type') continue;
    if (Array.isArray(v)) {
      if (v.length === 0) continue; // pomiń puste tablice — backend default to []
      const items = v.map(yamlString).join(', ');
      lines.push(`${pad2}${k}: [${items}]`);
    } else if (typeof v === 'boolean') {
      lines.push(`${pad2}${k}: ${v}`);
    } else if (v !== undefined && v !== null && v !== '') {
      lines.push(`${pad2}${k}: ${yamlString(String(v))}`);
    }
  }
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
