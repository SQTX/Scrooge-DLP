// SPDX-License-Identifier: GPL-2.0-only
//
// Form mode — split view: pola po lewej, generowany YAML preview (read-only)
// po prawej. Lewa pane otrzymuje sections w kolejnych sub-fazach (2F.4+).
// Aktualnie pokazuje placeholder "Form sections w budowie".
//
// YAML preview po prawej jest aktualizowany przez parent (PolicyEditor)
// gdy state.yamlText się zmienia (po debounce z formToYaml).

import { formToYaml } from '../codec/formToYaml.js';

export class FormMode {
  /**
   * @param {{
   *   mount: HTMLElement,
   *   initialFormState: object,
   *   onChange: (formState: object) => void,
   * }} opts
   */
  constructor({ mount, initialFormState, onChange }) {
    this.root = mount;
    this.formState = initialFormState;
    this.onChange = onChange;
    this.leftPane = null;
    this.previewPane = null;
  }

  render() {
    while (this.root.firstChild) this.root.removeChild(this.root.firstChild);

    // Split view (CSS grid 2 columns).
    const grid = document.createElement('div');
    grid.className = 'grid grid-cols-2 gap-4 min-h-[24rem]';

    // ── Lewa pane: form sections ─────────────────────────────────────
    this.leftPane = document.createElement('div');
    this.leftPane.className = 'space-y-3';
    this._renderLeftPlaceholder();
    grid.appendChild(this.leftPane);

    // ── Prawa pane: YAML preview read-only ───────────────────────────
    const previewWrap = document.createElement('div');
    previewWrap.className = 'flex flex-col';

    const previewLabel = document.createElement('div');
    previewLabel.className = 'text-xs uppercase tracking-wide text-gray-500 mb-1 font-medium';
    previewLabel.textContent = 'YAML preview (read-only)';
    previewWrap.appendChild(previewLabel);

    const pre = document.createElement('pre');
    pre.className = 'flex-1 bg-gray-50 border border-gray-200 rounded p-3 '
      + 'font-mono text-xs overflow-auto whitespace-pre text-gray-800';
    pre.textContent = formToYaml(this.formState);
    previewWrap.appendChild(pre);
    this.previewPane = pre;

    grid.appendChild(previewWrap);

    this.root.appendChild(grid);
  }

  _renderLeftPlaceholder() {
    const note = document.createElement('div');
    note.className = 'p-4 bg-amber-50 border border-amber-200 rounded text-xs text-amber-900';
    // Manualny build DOM — bez innerHTML (XSS safe by construction).
    const head = document.createElement('strong');
    head.textContent = 'Form mode w budowie';
    note.appendChild(head);
    note.appendChild(document.createElement('br'));
    const body = document.createTextNode(
      'Sekcje formularza (Metadata, Rule, Sources, Destinations, Conditions) '
      + 'wjadą w kolejnych commitach Sub-fazy 2F. Na razie używaj YAML mode '
      + '(przełącznik w nagłówku) do faktycznej edycji. Prawa pane pokazuje '
      + 'YAML preview wygenerowany z domyślnego formState.',
    );
    note.appendChild(body);
    this.leftPane.appendChild(note);
  }

  /** Parent woła gdy yamlText (= formToYaml(formState)) się zmienił. */
  updatePreview(yamlText) {
    if (this.previewPane) this.previewPane.textContent = yamlText;
  }

  /** Zwraca aktualny formState (parent czyta przy switchu / save). */
  getFormState() {
    return this.formState;
  }

  destroy() {
    while (this.root.firstChild) this.root.removeChild(this.root.firstChild);
    this.leftPane = null;
    this.previewPane = null;
    this.root = null;
  }
}
