// SPDX-License-Identifier: GPL-2.0-only
//
// TargetsSection — selektor agentów którym policy ma być pushed. Schema:
//
//   targets:
//     match:
//       os: ["linux", "macos", "windows"]
//       tags: { department: finance, env: prod }
//       groups: ["frontend-laptops"]
//       agent_ids: ["uuid-1", "uuid-2"]
//     exclude: { hostnames, agent_ids }  ← form NIE pokrywa (YAML mode only)
//
// Form UX: segmented control "Match by" → All | Tags | Groups | Specific
// agents (XOR). Backend schema dopuszcza miks (YAML mode), ale dla form
// jednoznaczność: jeden tryb na raz.
//
// OS filter zawsze widoczny niezależnie od trybu (multi-checkbox).

import { helpTip } from '../ui/HelpTip.js';

const HEADER_TIP =
  'Selektor — którzy agenci dostaną tę policy:\n\n'
  + '• All — wszyscy zarejestrowani (default, "global policy")\n'
  + '• Tags — agenci z pasującymi tagami (wymagany ALL pairs, np. '
  + 'department=finance + env=prod)\n'
  + '• Groups — agenci przypisani do nazwanej grupy w dashboardzie\n'
  + '• Specific agents — konkretni agenci po UUID (najsilniejszy match)\n\n'
  + 'OS filter (linux/macos/windows) działa NIEZALEŻNIE — zawężasz '
  + 'powyższy match do wybranych systemów.';

const MODE_LABELS = {
  all: 'All',
  tags: 'Tags',
  groups: 'Groups',
  agent_ids: 'Specific agents',
};

const OS_OPTIONS = ['linux', 'macos', 'windows'];

export class TargetsSection {
  /**
   * @param {{ mount: HTMLElement, initialState: object, onChange: (state) => void }} opts
   */
  constructor({ mount, initialState, onChange }) {
    this.root = mount;
    this.state = { ...defaultTargetsState(), ...initialState };
    this.onChange = onChange;
  }

  render() {
    while (this.root.firstChild) this.root.removeChild(this.root.firstChild);

    // Header.
    const head = document.createElement('div');
    head.className = 'flex items-center mb-2';
    const label = document.createElement('div');
    label.className = 'text-xs uppercase tracking-wide text-gray-500 font-medium';
    label.textContent = 'Targets';
    head.appendChild(label);
    head.appendChild(helpTip(HEADER_TIP));
    this.root.appendChild(head);

    // Segmented "Match by".
    const segWrap = document.createElement('div');
    segWrap.className = 'flex flex-col mb-2';
    const segLabel = document.createElement('div');
    segLabel.className = 'text-[11px] text-gray-600 mb-1';
    segLabel.textContent = 'Match by';
    segWrap.appendChild(segLabel);

    const seg = document.createElement('div');
    seg.className = 'inline-flex rounded-md border border-gray-300 overflow-hidden text-xs';
    for (const m of Object.keys(MODE_LABELS)) {
      const btn = document.createElement('button');
      btn.type = 'button';
      btn.textContent = MODE_LABELS[m];
      const active = this.state.mode === m;
      btn.className = 'px-3 py-1 font-medium transition '
        + (active ? 'bg-blue-600 text-white' : 'bg-white text-gray-700 hover:bg-gray-50');
      btn.addEventListener('click', () => this._setMode(m));
      seg.appendChild(btn);
    }
    segWrap.appendChild(seg);
    this.root.appendChild(segWrap);

    // Mode-specific input.
    const modeBody = document.createElement('div');
    modeBody.className = 'mb-2';
    if (this.state.mode === 'tags') {
      const ti = this._csvInput(
        'tags (np. department=finance, env=prod) — wymagane ALL pairs',
        tagsToCsv(this.state.tags),
      );
      ti.addEventListener('input', () =>
        this._emit({ tags: csvToTags(ti.value) }));
      modeBody.appendChild(ti);
    } else if (this.state.mode === 'groups') {
      const gi = this._csvInput('groups (csv, np. frontend-laptops, sales-vms)',
        (this.state.groups ?? []).join(', '));
      gi.addEventListener('input', () =>
        this._emit({ groups: csvSplit(gi.value) }));
      modeBody.appendChild(gi);
    } else if (this.state.mode === 'agent_ids') {
      const ai = this._csvInput('agent UUIDs (csv, np. 11111111-2222-..., 33333333-...)',
        (this.state.agent_ids ?? []).join(', '));
      ai.addEventListener('input', () =>
        this._emit({ agent_ids: csvSplit(ai.value) }));
      modeBody.appendChild(ai);
    } else {
      // 'all' mode — brak inputu.
      const hint = document.createElement('div');
      hint.className = 'text-xs text-gray-500 italic px-2 py-1';
      hint.textContent = 'Policy zostanie pushed do WSZYSTKICH agentów '
        + '(z opcjonalnym OS filter poniżej).';
      modeBody.appendChild(hint);
    }
    this.root.appendChild(modeBody);

    // OS filter (zawsze widoczne).
    const osWrap = document.createElement('div');
    osWrap.className = 'flex flex-col';
    const osLabel = document.createElement('div');
    osLabel.className = 'text-[11px] text-gray-600 mb-1 flex items-center';
    const osLblText = document.createElement('span');
    osLblText.textContent = 'OS filter';
    osLabel.appendChild(osLblText);
    osLabel.appendChild(helpTip(
      'Zawęź match do wybranych systemów. Brak zaznaczonego = brak filtru '
      + '(wszystkie OS-y).',
    ));
    osWrap.appendChild(osLabel);

    const osRow = document.createElement('div');
    osRow.className = 'flex gap-3 text-xs';
    for (const osName of OS_OPTIONS) {
      osRow.appendChild(this._osCheckbox(osName));
    }
    osWrap.appendChild(osRow);
    this.root.appendChild(osWrap);
  }

  _osCheckbox(osName) {
    const wrap = document.createElement('label');
    wrap.className = 'inline-flex items-center gap-1 cursor-pointer select-none';
    const cb = document.createElement('input');
    cb.type = 'checkbox';
    cb.checked = (this.state.os ?? []).includes(osName);
    cb.className = 'rounded';
    cb.addEventListener('change', () => {
      const current = new Set(this.state.os ?? []);
      if (cb.checked) current.add(osName); else current.delete(osName);
      this._emit({ os: [...current] });
    });
    const txt = document.createElement('span');
    txt.textContent = osName;
    wrap.appendChild(cb);
    wrap.appendChild(txt);
    return wrap;
  }

  _csvInput(placeholder, value) {
    const i = document.createElement('input');
    i.type = 'text';
    i.placeholder = placeholder;
    i.value = value;
    i.className = 'w-full px-2 py-1 border border-gray-300 rounded text-xs '
      + 'focus:outline-none focus:ring-2 focus:ring-blue-500';
    return i;
  }

  _setMode(newMode) {
    if (newMode === this.state.mode) return;
    // Przełączamy mode, ale NIE czyścimy poprzednich wartości — user może
    // chcieć wrócić bez utraty. Backend dostaje tylko aktywny mode w YAML.
    this.state = { ...this.state, mode: newMode };
    this._emit({});
    this.render();
  }

  _emit(patch) {
    this.state = { ...this.state, ...patch };
    if (this.onChange) this.onChange(this.state);
  }

  getState() { return this.state; }

  destroy() {
    while (this.root.firstChild) this.root.removeChild(this.root.firstChild);
    this.root = null;
  }
}

export function defaultTargetsState() {
  return {
    mode: 'all',
    os: [],
    tags: {},
    groups: [],
    agent_ids: [],
  };
}

// ── helpery csv/tags ──────────────────────────────────────────────────────

function csvSplit(s) {
  return s.split(',').map((x) => x.trim()).filter((x) => x.length > 0);
}

/** "department=finance, env=prod" → { department: "finance", env: "prod" } */
function csvToTags(s) {
  const out = {};
  for (const pair of csvSplit(s)) {
    const eq = pair.indexOf('=');
    if (eq <= 0) continue; // pomiń malformed pairs
    const k = pair.slice(0, eq).trim();
    const v = pair.slice(eq + 1).trim();
    if (k) out[k] = v;
  }
  return out;
}

function tagsToCsv(obj) {
  if (!obj || typeof obj !== 'object') return '';
  return Object.entries(obj).map(([k, v]) => `${k}=${v}`).join(', ');
}
