// SPDX-License-Identifier: GPL-2.0-only
//
// Reguły co Standard form scope pokrywa. Form mode obsługuje:
//   - metadata + priority
//   - DOKŁADNIE 1 rule w rules.outbound (sources/destinations/conditions/...)
//   - NIC z: rules.inbound, targets.match.*, targets.exclude.*,
//     monitored_paths, classifiers

/**
 * @param {object} parsed — sparsowana Policy (JSON z backend Validate).
 * @returns {string[]} — lista user-friendly opisów unsupported features.
 *   Pusta = polityka jest form-friendly.
 */
export function listUnsupported(parsed) {
  const out = [];
  if (!parsed) return out;

  const rules = parsed.rules ?? {};
  const outbound = rules.outbound ?? [];
  const inbound = rules.inbound ?? [];

  if (outbound.length > 1) {
    out.push(`wiele reguł outbound (${outbound.length})`);
  }
  if (inbound.length > 0) {
    out.push(`reguły inbound (${inbound.length})`);
  }

  // Targets form mode (Sub-faza 2F.14) pokrywa match.os + JEDEN z
  // {tags, groups, agent_ids}. Miks dwóch lub exclude — nadal unsupported.
  const targets = parsed.targets ?? {};
  const tm = targets['match'] ?? {};
  const matchModes = [
    hasKeys(tm.tags) ? 'tags' : null,
    (tm.groups ?? []).length > 0 ? 'groups' : null,
    (tm.agent_ids ?? []).length > 0 ? 'agent_ids' : null,
  ].filter(Boolean);
  if (matchModes.length > 1) {
    out.push(`miks targets.match (${matchModes.join(' + ')}) — form pokrywa tylko jeden tryb`);
  }
  const te = targets.exclude ?? {};
  if ((te.hostnames ?? []).length || (te.agent_ids ?? []).length) {
    out.push('targets.exclude');
  }

  if ((parsed.monitored_paths ?? []).length > 0) {
    out.push(`monitored_paths (${parsed.monitored_paths.length})`);
  }
  if ((parsed.classifiers ?? []).length > 0) {
    out.push(`classifiers (${parsed.classifiers.length})`);
  }

  return out;
}

export function isFormFriendly(parsed) {
  return listUnsupported(parsed).length === 0
    && (parsed?.rules?.outbound ?? []).length === 1;
}

function hasKeys(obj) {
  return !!obj && typeof obj === 'object' && Object.keys(obj).length > 0;
}
