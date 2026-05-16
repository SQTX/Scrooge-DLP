// SPDX-License-Identifier: GPL-2.0-only
//
// Cienki wrapper na REST endpointy polityk. Każda funkcja zwraca parsed JSON
// albo throw'uje z czytelnym error message.

import { authHeaders } from './auth.js';

/**
 * GET /api/v1/policies/{name} — pełna polityka z content_yaml.
 * @param {string} name
 */
export async function getPolicy(name) {
  const r = await fetch('/api/v1/policies/' + encodeURIComponent(name), {
    headers: await authHeaders(),
  });
  if (!r.ok) throw new Error('GET policy failed: HTTP ' + r.status);
  return r.json();
}

/**
 * POST /api/v1/policies/validate — dry-run walidacji YAMLa.
 * Zwraca `{ valid, errors, hash?, name?, version?, parsed? }`.
 * @param {string} yaml
 */
export async function validate(yaml) {
  const r = await fetch('/api/v1/policies/validate', {
    method: 'POST',
    headers: await authHeaders(),
    body: JSON.stringify({ content_yaml: yaml }),
  });
  if (!r.ok) throw new Error('validate failed: HTTP ' + r.status);
  return r.json();
}

/**
 * POST /api/v1/policies — utwórz nową politykę.
 * @param {string} yaml
 * @param {string} reason
 */
export async function create(yaml, reason = 'created via dashboard') {
  const r = await fetch('/api/v1/policies', {
    method: 'POST',
    headers: await authHeaders(),
    body: JSON.stringify({ content_yaml: yaml, change_reason: reason }),
  });
  const body = await r.json();
  if (!r.ok) throw new Error(body.message || ('HTTP ' + r.status));
  return body;
}

/**
 * PUT /api/v1/policies/{name} — update istniejącej polityki.
 * @param {string} name
 * @param {string} yaml
 * @param {string} reason
 */
export async function update(name, yaml, reason = 'edit via dashboard') {
  const r = await fetch('/api/v1/policies/' + encodeURIComponent(name), {
    method: 'PUT',
    headers: await authHeaders(),
    body: JSON.stringify({ content_yaml: yaml, change_reason: reason }),
  });
  const body = await r.json();
  if (!r.ok) throw new Error(body.message || ('HTTP ' + r.status));
  return body;
}
