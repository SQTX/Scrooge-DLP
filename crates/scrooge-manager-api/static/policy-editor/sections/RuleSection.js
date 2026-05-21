// SPDX-License-Identifier: GPL-2.0-only
//
// Sekcja Rule — pola pojedynczej outbound rule (Standard scope = 1 rule).
// Pola: id, name, severity, action, message, notify_user, forward_to_siem.
// Sources/destinations/conditions to osobne sekcje (kolejne sub-fazy 2F.5-7).

import { helpTip } from '../ui/HelpTip.js';

const SEVERITY_OPTIONS = ['low', 'medium', 'high', 'critical'];
const ACTION_OPTIONS = [
  'log_only',
  'log_and_alert',
  'warn_user',
  'block',
  'quarantine',
  'lock_workstation',
  'kill_process',
];

const TIPS = {
  id: 'Stabilny identyfikator reguły (slug: a-z, 0-9, -). Używany w event '
    + 'logach jako triggered_rule_id. Nie zmieniaj po deploy — zerwiesz '
    + 'historię eventów.',
  name: 'Czytelna nazwa wyświetlana w dashboardzie i alertach. Dowolny tekst.',
  severity:
    'Waga reguły (informacyjna, nie wpływa na egzekucję):\n'
    + '• low — niska, audit only\n'
    + '• medium — uwaga\n'
    + '• high — incydent\n'
    + '• critical — bezpieczeństwo / compliance',
  action:
    'Co agent ma zrobić gdy reguła zmatchuje. UWAGA: enforcement wchodzi '
    + 'od Phase 3, dziś agent tylko zapisuje policy ale nic nie egzekwuje.\n\n'
    + '• log_only — wpis do bazy eventów, user nic nie widzi\n'
    + '• log_and_alert — log + alert w dashboardzie + opcjonalnie SIEM\n'
    + '• warn_user — desktop notification (edukacja), brak blokady\n'
    + '• block — anulowanie operacji (clipboard clear, upload kill, '
    + 'USB write cancel)\n'
    + '• quarantine — block + plik do encrypted quarantine (admin '
    + 'może odzyskać)\n'
    + '• lock_workstation — wymuszony Win+L / loginctl lock-session\n'
    + '• kill_process — SIGKILL / TerminateProcess procesu',
  message: 'Tekst notification wyświetlanego userowi gdy action=warn_user '
    + 'lub notify_user=true. Plain UTF-8, bez HTML.',
  notify_user: 'Pokaż desktop notification gdy reguła trigger\'uje (niezależnie '
    + 'od action). Wymaga `message` z treścią.',
  forward_to_siem: 'Wyślij event przez RFC 5424 syslog do skonfigurowanego '
    + 'SIEM (Wazuh / Graylog / Splunk). Default false — admin świadomie '
    + 'włącza dla high-severity.',
};

export class RuleSection {
  /**
   * @param {{ mount: HTMLElement, initialState: object, onChange: (state) => void }} opts
   */
  constructor({ mount, initialState, onChange }) {
    this.root = mount;
    this.state = { ...defaultRuleState(), ...initialState };
    this.onChange = onChange;
    this.refs = {};
  }

  render() {
    while (this.root.firstChild) this.root.removeChild(this.root.firstChild);

    const label = document.createElement('div');
    label.className = 'text-xs uppercase tracking-wide text-gray-500 font-medium mb-2';
    label.textContent = 'Rule';
    this.root.appendChild(label);

    // Row 1: id + severity + action.
    const row1 = document.createElement('div');
    row1.className = 'flex gap-2 mb-2 items-end';

    this.refs.id = this._mkInput('text', 'rule id (slug)', this.state.id);
    this.refs.id.addEventListener('input', () => this._emit({ id: this.refs.id.value }));
    row1.appendChild(this._field('id', TIPS.id, this.refs.id, 'flex-1'));

    this.refs.severity = this._mkSelect(SEVERITY_OPTIONS, this.state.severity);
    this.refs.severity.addEventListener('change', () =>
      this._emit({ severity: this.refs.severity.value }));
    row1.appendChild(this._field('severity', TIPS.severity, this.refs.severity, 'w-32'));

    this.refs.action = this._mkSelect(ACTION_OPTIONS, this.state.action);
    this.refs.action.addEventListener('change', () =>
      this._emit({ action: this.refs.action.value }));
    row1.appendChild(this._field('action', TIPS.action, this.refs.action, 'w-44'));
    this.root.appendChild(row1);

    // Row 2: human name.
    this.refs.name = this._mkInput('text', 'rule name (human readable)', this.state.name);
    this.refs.name.addEventListener('input', () => this._emit({ name: this.refs.name.value }));
    this.root.appendChild(this._field('name', TIPS.name, this.refs.name, 'w-full mb-2'));

    // Row 3: message.
    this.refs.message = this._mkInput('text',
      'tekst notification dla usera', this.state.message ?? '');
    this.refs.message.addEventListener('input', () =>
      this._emit({ message: this.refs.message.value }));
    this.root.appendChild(this._field('message', TIPS.message, this.refs.message, 'w-full mb-2'));

    // Row 4: checkboxy.
    const checks = document.createElement('div');
    checks.className = 'flex gap-4 text-xs text-gray-700';
    checks.appendChild(this._mkCheckbox('notify_user', 'notify_user', this.state.notify_user, TIPS.notify_user));
    checks.appendChild(this._mkCheckbox('forward_to_siem', 'forward_to_siem', this.state.forward_to_siem, TIPS.forward_to_siem));
    this.root.appendChild(checks);
  }

  /** Wrap input/select w `<div>` z label + tooltip nad polem. */
  _field(labelText, tipText, inputEl, wrapClass = '') {
    const wrap = document.createElement('div');
    wrap.className = 'flex flex-col ' + wrapClass;
    const lbl = document.createElement('label');
    lbl.className = 'text-[11px] text-gray-600 mb-0.5 flex items-center';
    const lblText = document.createElement('span');
    lblText.textContent = labelText;
    lbl.appendChild(lblText);
    lbl.appendChild(helpTip(tipText));
    wrap.appendChild(lbl);
    wrap.appendChild(inputEl);
    return wrap;
  }

  _mkInput(type, placeholder, value) {
    const i = document.createElement('input');
    i.type = type;
    i.placeholder = placeholder;
    i.value = value ?? '';
    i.className = 'px-3 py-1.5 border border-gray-300 rounded text-sm w-full '
      + 'focus:outline-none focus:ring-2 focus:ring-blue-500';
    return i;
  }

  _mkSelect(options, value) {
    const s = document.createElement('select');
    s.className = 'px-2 py-1.5 border border-gray-300 rounded text-sm w-full '
      + 'focus:outline-none focus:ring-2 focus:ring-blue-500';
    for (const opt of options) {
      const o = document.createElement('option');
      o.value = opt;
      o.textContent = opt;
      if (opt === value) o.selected = true;
      s.appendChild(o);
    }
    return s;
  }

  _mkCheckbox(field, label, checked, tipText) {
    const wrap = document.createElement('label');
    wrap.className = 'inline-flex items-center gap-1 cursor-pointer select-none';
    const cb = document.createElement('input');
    cb.type = 'checkbox';
    cb.checked = !!checked;
    cb.className = 'rounded';
    cb.addEventListener('change', () => this._emit({ [field]: cb.checked }));
    const txt = document.createElement('span');
    txt.textContent = label;
    wrap.appendChild(cb);
    wrap.appendChild(txt);
    if (tipText) wrap.appendChild(helpTip(tipText));
    return wrap;
  }

  _emit(patch) {
    this.state = { ...this.state, ...patch };
    if (this.onChange) this.onChange(this.state);
  }

  getState() { return this.state; }

  destroy() {
    while (this.root.firstChild) this.root.removeChild(this.root.firstChild);
    this.refs = {};
    this.root = null;
  }
}

export function defaultRuleState() {
  return {
    id: 'my-rule',
    name: '',
    severity: 'medium',
    action: 'log_only',
    message: '',
    notify_user: false,
    forward_to_siem: false,
  };
}
