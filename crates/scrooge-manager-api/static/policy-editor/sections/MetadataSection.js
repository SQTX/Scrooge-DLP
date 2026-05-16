// SPDX-License-Identifier: GPL-2.0-only
//
// Sekcja Metadata — name, description, priority. Emit'uje change z
// pełnym slice'em metadata.

export class MetadataSection {
  /**
   * @param {{ mount: HTMLElement, initialState: {name, description, priority}, onChange: (state) => void }} opts
   */
  constructor({ mount, initialState, onChange }) {
    this.root = mount;
    this.state = { ...initialState };
    this.onChange = onChange;
    this.nameInput = null;
    this.descInput = null;
    this.prioInput = null;
  }

  render() {
    while (this.root.firstChild) this.root.removeChild(this.root.firstChild);

    const label = document.createElement('div');
    label.className = 'text-xs uppercase tracking-wide text-gray-500 font-medium mb-2';
    label.textContent = 'Metadata';
    this.root.appendChild(label);

    // Row: name + priority.
    const row1 = document.createElement('div');
    row1.className = 'flex gap-2 mb-2';

    this.nameInput = this._mkInput('text', 'name (slug: a-z0-9-)', this.state.name ?? '');
    this.nameInput.className += ' flex-1';
    this.nameInput.addEventListener('input', () => this._emit({ name: this.nameInput.value }));
    row1.appendChild(this.nameInput);

    this.prioInput = this._mkInput('number', 'priority', this.state.priority ?? 100);
    this.prioInput.className += ' w-24';
    this.prioInput.min = '0';
    this.prioInput.max = '1000';
    this.prioInput.addEventListener('input', () => {
      const n = parseInt(this.prioInput.value, 10);
      this._emit({ priority: Number.isFinite(n) ? n : 100 });
    });
    row1.appendChild(this.prioInput);

    this.root.appendChild(row1);

    this.descInput = this._mkInput('text', 'description (opcjonalne)', this.state.description ?? '');
    this.descInput.className += ' w-full';
    this.descInput.addEventListener('input', () => this._emit({ description: this.descInput.value }));
    this.root.appendChild(this.descInput);
  }

  _mkInput(type, placeholder, value) {
    const i = document.createElement('input');
    i.type = type;
    i.placeholder = placeholder;
    i.value = value;
    i.className = 'px-3 py-1.5 border border-gray-300 rounded text-sm '
      + 'focus:outline-none focus:ring-2 focus:ring-blue-500';
    return i;
  }

  _emit(patch) {
    this.state = { ...this.state, ...patch };
    if (this.onChange) this.onChange(this.state);
  }

  getState() { return this.state; }

  destroy() {
    while (this.root.firstChild) this.root.removeChild(this.root.firstChild);
    this.nameInput = null;
    this.descInput = null;
    this.prioInput = null;
    this.root = null;
  }
}
