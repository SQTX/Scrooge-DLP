// SPDX-License-Identifier: GPL-2.0-only
//
// Sekcja Rule — pola pojedynczej outbound rule (Standard scope = 1 rule).
// Pola: id, name, severity, action, message, notify_user, forward_to_siem.
// Sources/destinations/conditions to osobne sekcje (kolejne sub-fazy 2F.5-7).

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
    row1.className = 'flex gap-2 mb-2';
    this.refs.id = this._mkInput('text', 'rule id (slug)', this.state.id);
    this.refs.id.className += ' flex-1';
    this.refs.id.addEventListener('input', () => this._emit({ id: this.refs.id.value }));
    row1.appendChild(this.refs.id);

    this.refs.severity = this._mkSelect(SEVERITY_OPTIONS, this.state.severity);
    this.refs.severity.className += ' w-32';
    this.refs.severity.addEventListener('change', () =>
      this._emit({ severity: this.refs.severity.value }));
    row1.appendChild(this.refs.severity);

    this.refs.action = this._mkSelect(ACTION_OPTIONS, this.state.action);
    this.refs.action.className += ' w-40';
    this.refs.action.addEventListener('change', () =>
      this._emit({ action: this.refs.action.value }));
    row1.appendChild(this.refs.action);
    this.root.appendChild(row1);

    // Row 2: human name.
    this.refs.name = this._mkInput('text', 'rule name (human readable)', this.state.name);
    this.refs.name.className += ' w-full mb-2';
    this.refs.name.addEventListener('input', () => this._emit({ name: this.refs.name.value }));
    this.root.appendChild(this.refs.name);

    // Row 3: message.
    this.refs.message = this._mkInput('text',
      'message (opcjonalny tekst notification)', this.state.message ?? '');
    this.refs.message.className += ' w-full mb-2';
    this.refs.message.addEventListener('input', () =>
      this._emit({ message: this.refs.message.value }));
    this.root.appendChild(this.refs.message);

    // Row 4: checkboxy.
    const checks = document.createElement('div');
    checks.className = 'flex gap-4 text-xs text-gray-700';
    checks.appendChild(this._mkCheckbox('notify_user', 'notify_user', this.state.notify_user));
    checks.appendChild(this._mkCheckbox('forward_to_siem', 'forward_to_siem', this.state.forward_to_siem));
    this.root.appendChild(checks);
  }

  _mkInput(type, placeholder, value) {
    const i = document.createElement('input');
    i.type = type;
    i.placeholder = placeholder;
    i.value = value ?? '';
    i.className = 'px-3 py-1.5 border border-gray-300 rounded text-sm '
      + 'focus:outline-none focus:ring-2 focus:ring-blue-500';
    return i;
  }

  _mkSelect(options, value) {
    const s = document.createElement('select');
    s.className = 'px-2 py-1.5 border border-gray-300 rounded text-sm '
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

  _mkCheckbox(field, label, checked) {
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
