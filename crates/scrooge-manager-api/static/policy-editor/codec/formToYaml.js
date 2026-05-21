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
import { defaultTargetsState } from '../sections/TargetsSection.js';

const DEFAULT_FORM_STATE = {
  metadata: { name: 'my-policy', description: '', priority: 100 },
  targets: defaultTargetsState(),
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

  // Targets — emit'ujemy tylko aktywny match mode (form UX XOR).
  emitTargets(lines, formState.targets);

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

    // Sources / destinations / conditions żyją na poziomie formState,
    // ale w YAMLu wjeżdżają wewnątrz rules.outbound[0]. Emit'ujemy tutaj.
    emitSources(lines, formState.sources ?? []);
    emitDestinations(lines, formState.destinations ?? []);
    emitConditions(lines, formState.conditions);
  }

  return lines.join('\n') + '\n';
}

function emitTargets(lines, t) {
  if (!t) return;
  const os = Array.isArray(t.os) ? t.os : [];
  const mode = t.mode ?? 'all';
  // Mode='all' bez OS = brak `targets:` block (match wszystkich = default).
  const hasMode = mode === 'tags' ? hasKeys(t.tags)
    : mode === 'groups' ? (t.groups ?? []).length > 0
    : mode === 'agent_ids' ? (t.agent_ids ?? []).length > 0
    : false;
  if (!hasMode && os.length === 0) return;

  lines.push('targets:');
  lines.push('  match:');
  if (os.length > 0) {
    lines.push(`    os: [${os.map(yamlString).join(', ')}]`);
  }
  if (mode === 'tags' && hasKeys(t.tags)) {
    lines.push('    tags:');
    for (const [k, v] of Object.entries(t.tags)) {
      lines.push(`      ${yamlString(k)}: ${yamlString(String(v))}`);
    }
  } else if (mode === 'groups' && (t.groups ?? []).length > 0) {
    lines.push(`    groups: [${t.groups.map(yamlString).join(', ')}]`);
  } else if (mode === 'agent_ids' && (t.agent_ids ?? []).length > 0) {
    lines.push(`    agent_ids: [${t.agent_ids.map(yamlString).join(', ')}]`);
  }
}

function hasKeys(obj) {
  return !!obj && typeof obj === 'object' && Object.keys(obj).length > 0;
}

function emitConditions(lines, cond) {
  if (!cond) return;
  const hasAny =
    cond.file_size_min || cond.file_size_max
    || (Array.isArray(cond.file_extensions) && cond.file_extensions.length)
    || cond.filename_regex || cond.path_regex;
  if (!hasAny) return;
  lines.push('      conditions:');
  if (cond.file_size_min) {
    lines.push(`        file_size_min: ${yamlString(String(cond.file_size_min))}`);
  }
  if (cond.file_size_max) {
    lines.push(`        file_size_max: ${yamlString(String(cond.file_size_max))}`);
  }
  if (Array.isArray(cond.file_extensions) && cond.file_extensions.length) {
    const items = cond.file_extensions.map(yamlString).join(', ');
    lines.push(`        file_extensions: [${items}]`);
  }
  if (cond.filename_regex) {
    lines.push(`        filename_regex: ${yamlString(cond.filename_regex)}`);
  }
  if (cond.path_regex) {
    lines.push(`        path_regex: ${yamlString(cond.path_regex)}`);
  }
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
