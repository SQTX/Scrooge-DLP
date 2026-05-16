// SPDX-License-Identifier: GPL-2.0-only
//
// Toggle Form ↔ YAML w nagłówku policy modala. Vanilla button group z
// aktywnym/nieaktywnym stanem zarządzanym przez klasy Tailwind.

export class ModeSwitch {
  /**
   * @param {{ mount: HTMLElement, initialMode: 'form'|'yaml', onSwitch: (mode: 'form'|'yaml') => void }} opts
   */
  constructor({ mount, initialMode, onSwitch }) {
    this.root = mount;
    this.mode = initialMode;
    this.onSwitch = onSwitch;
    this.formBtn = null;
    this.yamlBtn = null;
  }

  render() {
    while (this.root.firstChild) this.root.removeChild(this.root.firstChild);

    const wrap = document.createElement('div');
    wrap.className = 'inline-flex rounded-md border border-gray-300 overflow-hidden text-xs';

    this.formBtn = this._mkButton('Form', 'form');
    this.yamlBtn = this._mkButton('YAML', 'yaml');
    wrap.appendChild(this.formBtn);
    wrap.appendChild(this.yamlBtn);
    this.root.appendChild(wrap);

    this._applyActive();
  }

  _mkButton(label, mode) {
    const b = document.createElement('button');
    b.type = 'button';
    b.dataset.mode = mode;
    b.textContent = label;
    b.className = 'px-3 py-1 font-medium transition';
    b.addEventListener('click', () => {
      if (this.mode === mode) return;
      // Najpierw zapytaj parent — może zablokować (np. YAML invalid przy
      // próbie YAML→Form). Parent woła setMode() po sukcesie.
      this.onSwitch(mode);
    });
    return b;
  }

  /** Wymuszone ustawienie aktywnego mode'u (parent wywołuje po decyzji). */
  setMode(mode) {
    this.mode = mode;
    this._applyActive();
  }

  _applyActive() {
    const active = 'bg-blue-600 text-white';
    const inactive = 'bg-white text-gray-700 hover:bg-gray-50';
    const fmActive = this.mode === 'form';
    this.formBtn.className = 'px-3 py-1 font-medium transition ' + (fmActive ? active : inactive);
    this.yamlBtn.className = 'px-3 py-1 font-medium transition ' + (fmActive ? inactive : active);
  }

  destroy() {
    while (this.root.firstChild) this.root.removeChild(this.root.firstChild);
    this.formBtn = null;
    this.yamlBtn = null;
    this.root = null;
  }
}
