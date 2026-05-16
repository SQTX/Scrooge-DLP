// SPDX-License-Identifier: GPL-2.0-only
//
// Mały komponent renderujący wynik walidacji pod body modala.
// Konsumuje response z `POST /api/v1/policies/validate` albo własny stan
// loading/error.

export class ValidationPanel {
  /**
   * @param {{ mount: HTMLElement }} opts
   */
  constructor({ mount }) {
    this.root = mount;
    this.root.className = 'text-xs';
  }

  /** Reset do pustego stanu. */
  clear() {
    while (this.root.firstChild) this.root.removeChild(this.root.firstChild);
    this.root.className = 'text-xs';
  }

  /** Stan "ładowanie" podczas request'u. */
  loading(label = 'Walidacja…') {
    this.clear();
    this.root.textContent = label;
    this.root.className = 'text-xs text-gray-500';
  }

  /**
   * Sukces walidacji.
   * @param {{ name?: string, version?: number, hash?: string }} resp
   */
  ok(resp) {
    this.clear();
    const label = '✓ Valid — ' + (resp.name ?? '?') + ' v' + (resp.version ?? '?')
      + ' (hash: ' + (resp.hash ?? '').slice(0, 16) + '…)';
    this.root.textContent = label;
    this.root.className = 'text-xs text-green-700';
  }

  /**
   * Błędy walidacji (lista stringów).
   * Bezpieczne renderowanie — error stringi mogą zawierać user-controlled content.
   * @param {string[]} errors
   */
  errors(errors) {
    this.clear();
    const head = document.createElement('div');
    head.className = 'font-semibold mb-1';
    head.textContent = '✗ Invalid:';
    this.root.appendChild(head);
    for (const err of errors) {
      const li = document.createElement('div');
      li.className = 'pl-3';
      li.textContent = '• ' + err;
      this.root.appendChild(li);
    }
    this.root.className = 'text-xs text-red-700';
  }

  /** Komunikat o błędzie sieci/serwera (nie validation error). */
  fail(message) {
    this.clear();
    this.root.textContent = message;
    this.root.className = 'text-xs text-red-700';
  }

  destroy() {
    this.clear();
    this.root = null;
  }
}
