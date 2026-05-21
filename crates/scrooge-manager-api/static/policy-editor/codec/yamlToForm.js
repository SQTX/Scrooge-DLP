// SPDX-License-Identifier: GPL-2.0-only
//
// yamlToForm — parsuje YAML przez backend Validate (serde_yaml jako single
// source of truth), bierze response.parsed (Sub-faza 2F.1) i mapuje na
// formState. Plus listę unsupported features (supportability.js).

import { validate } from '../api/policies.js';
import { listUnsupported } from './supportability.js';
import { defaultFormState } from './formToYaml.js';

/**
 * @param {string} yamlText
 * @returns {Promise<
 *   | { ok: true, formState: object, unsupported: string[], ghostYaml: string|null }
 *   | { ok: false, error: string }
 * >}
 */
export async function yamlToForm(yamlText) {
  let resp;
  try {
    resp = await validate(yamlText);
  } catch (err) {
    return { ok: false, error: 'walidacja nie powiodła się: ' + err.message };
  }
  if (!resp.valid) {
    return {
      ok: false,
      error: (resp.errors && resp.errors[0]) || 'YAML invalid (brak szczegółów)',
    };
  }
  if (!resp.parsed) {
    // Backend valid=true ale brak parsed — starsza wersja serwera bez 2F.1.
    return {
      ok: false,
      error: 'backend nie zwrócił sparsowanej polityki (zaktualizuj managera do >= 2F.1)',
    };
  }

  const formState = parsedToFormState(resp.parsed);
  const unsupported = listUnsupported(resp.parsed);

  return {
    ok: true,
    formState,
    unsupported,
    // Ghost YAML — surowy tekst zachowany żeby Save mógł zapisać oryginał
    // (z unsupported sections). 2F.13: implementacja merge'a w Save flow.
    ghostYaml: unsupported.length > 0 ? yamlText : null,
  };
}

/**
 * Mapuje parsed Policy (z backendu) na nasze formState. Bierze tylko to
 * co Standard form obsługuje — reszta zostaje w ghost (zachowana w YAMLu).
 */
function parsedToFormState(parsed) {
  const def = defaultFormState();
  const md = parsed.metadata ?? {};
  const outbound = parsed?.rules?.outbound ?? [];
  const firstRule = outbound[0] ?? null;

  const fs = {
    metadata: {
      name: md.name ?? def.metadata.name,
      description: md.description ?? '',
      priority: Number.isFinite(parsed.priority) ? parsed.priority : 100,
    },
    targets: parsedTargetsToForm(parsed.targets),
    rule: firstRule ? {
      id: firstRule.id ?? 'my-rule',
      name: firstRule.name ?? '',
      severity: firstRule.severity ?? 'medium',
      action: firstRule.action ?? 'log_only',
      message: firstRule.message ?? '',
      notify_user: !!firstRule.notify_user,
      forward_to_siem: !!firstRule.forward_to_siem,
    } : def.rule,
    sources: firstRule?.sources ? firstRule.sources.map(normalizeItem) : [],
    destinations: firstRule?.destinations ? firstRule.destinations.map(normalizeItem) : [],
    conditions: firstRule?.conditions ? normalizeConditions(firstRule.conditions) : null,
  };
  return fs;
}

function parsedTargetsToForm(t) {
  if (!t || typeof t !== 'object') {
    return { mode: 'all', os: [], tags: {}, groups: [], agent_ids: [] };
  }
  const tm = t['match'] ?? {};
  const os = Array.isArray(tm.os) ? [...tm.os] : [];
  const tags = (tm.tags && typeof tm.tags === 'object') ? { ...tm.tags } : {};
  const groups = Array.isArray(tm.groups) ? [...tm.groups] : [];
  const agentIds = Array.isArray(tm.agent_ids) ? [...tm.agent_ids] : [];
  // Wybierz mode na podstawie tego co wypełnione. Jeśli >1 trybów (miks) —
  // supportability flaguje jako unsupported, mode w form pokaże pierwszy.
  let mode = 'all';
  if (Object.keys(tags).length > 0) mode = 'tags';
  else if (groups.length > 0) mode = 'groups';
  else if (agentIds.length > 0) mode = 'agent_ids';
  return { mode, os, tags, groups, agent_ids: agentIds };
}

function normalizeItem(it) {
  // Defensywne: backend zwraca dokładnie struct'ne pola, ale upewniamy się
  // że arrays są arrays (a nie undefined).
  const out = { type: it.type };
  for (const [k, v] of Object.entries(it)) {
    if (k === 'type') continue;
    out[k] = v;
  }
  return out;
}

function normalizeConditions(c) {
  // file_size_min/max przychodzą jako u64 (liczby bajtów). Form pokazuje
  // string — konwertujemy na najbardziej czytelną jednostkę.
  return {
    file_size_min: c.file_size_min != null ? prettySize(c.file_size_min) : '',
    file_size_max: c.file_size_max != null ? prettySize(c.file_size_max) : '',
    file_extensions: Array.isArray(c.file_extensions) ? [...c.file_extensions] : [],
    filename_regex: c.filename_regex ?? '',
    path_regex: c.path_regex ?? '',
  };
}

/**
 * `1024` → `"1KB"`, `2097152` → `"2MB"`, `512` → `"512B"`.
 * Trzymamy się exact-only (bez zaokrąglania) żeby Save → Validate dawało
 * ten sam YAML.
 */
function prettySize(bytes) {
  const n = Number(bytes);
  if (!Number.isFinite(n) || n < 0) return '';
  const GB = 1024 * 1024 * 1024;
  const MB = 1024 * 1024;
  const KB = 1024;
  if (n % GB === 0 && n >= GB) return (n / GB) + 'GB';
  if (n % MB === 0 && n >= MB) return (n / MB) + 'MB';
  if (n % KB === 0 && n >= KB) return (n / KB) + 'KB';
  return n + 'B';
}
