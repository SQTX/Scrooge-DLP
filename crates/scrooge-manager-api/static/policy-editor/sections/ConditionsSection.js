// SPDX-License-Identifier: GPL-2.0-only
//
// ConditionsSection — filtry post-match (file size, extensions, regex).
// Domyślnie zwinięty (collapsible) — większość polityk go nie używa.
//
// Pola (z `scrooge_policy::rules::Conditions`):
//   - file_size_min   string "1KB" lub liczba bajtów (parser akceptuje oba)
//   - file_size_max   j.w.
//   - file_extensions list (csv: "xlsx,pdf,docx")
//   - filename_regex  string (regex na filename bez ścieżki)
//   - path_regex      string (regex na pełnej ścieżce)

import { helpTip } from '../ui/HelpTip.js';

const HEADER_TIP =
  'Dodatkowe filtry stosowane PO matchu source+destination. Jeśli condition '
  + 'nie pasuje — rule nie wpada w action.\n\n'
  + '• file_size_min/max — rozmiar pliku (np. "1KB", "2MB", "1024")\n'
  + '• file_extensions — lista rozszerzeń bez kropki (np. xlsx, pdf, docx)\n'
  + '• filename_regex — regex matching nazwy pliku (bez ścieżki)\n'
  + '• path_regex — regex matching pełnej ścieżki\n\n'
  + 'Wszystkie opcjonalne — pusta sekcja = brak dodatkowych filtrów.';

export class ConditionsSection {
  /**
   * @param {{ mount: HTMLElement, initialState: object|null, onChange: (state) => void }} opts
   */
  constructor({ mount, initialState, onChange }) {
    this.root = mount;
    this.state = { ...(initialState ?? {}) };
    this.onChange = onChange;
    // Auto-rozwiń jeśli jakieś pole już ustawione (np. po YAML→Form parsing).
    this.expanded = hasAnyField(this.state);
    this.refs = {};
  }

  render() {
    while (this.root.firstChild) this.root.removeChild(this.root.firstChild);

    // Header (klikalny toggle).
    const header = document.createElement('button');
    header.type = 'button';
    header.className = 'flex items-center gap-1 text-xs uppercase tracking-wide '
      + 'text-gray-500 font-medium hover:text-gray-700';
    const caret = document.createElement('span');
    caret.textContent = this.expanded ? '▾' : '▸';
    header.appendChild(caret);
    const label = document.createElement('span');
    label.textContent = 'Conditions (opcjonalne)';
    header.appendChild(label);
    header.addEventListener('click', () => {
      this.expanded = !this.expanded;
      this.render();
    });
    this.root.appendChild(header);
    // Tooltip obok header'a (poza klikalnym buttonem żeby `?` nie toggle'ował).
    const tipWrap = document.createElement('span');
    tipWrap.className = 'inline-flex ml-1';
    tipWrap.appendChild(helpTip(HEADER_TIP));
    this.root.firstChild.parentNode.insertBefore(tipWrap, this.root.firstChild.nextSibling);

    if (!this.expanded) return;

    const body = document.createElement('div');
    body.className = 'mt-2 space-y-1';

    this.refs.sizeMin = this._mkInput('text',
      'file_size_min (np. 1KB, 2MB, 1024) — opcjonalne',
      this.state.file_size_min ?? '');
    this.refs.sizeMin.addEventListener('input', () =>
      this._emit({ file_size_min: this.refs.sizeMin.value }));
    body.appendChild(this.refs.sizeMin);

    this.refs.sizeMax = this._mkInput('text',
      'file_size_max (np. 100MB) — opcjonalne',
      this.state.file_size_max ?? '');
    this.refs.sizeMax.addEventListener('input', () =>
      this._emit({ file_size_max: this.refs.sizeMax.value }));
    body.appendChild(this.refs.sizeMax);

    this.refs.exts = this._mkInput('text',
      'file_extensions (csv bez kropki, np. xlsx,pdf,docx)',
      (this.state.file_extensions ?? []).join(', '));
    this.refs.exts.addEventListener('input', () =>
      this._emit({ file_extensions: csvSplit(this.refs.exts.value) }));
    body.appendChild(this.refs.exts);

    this.refs.fnRe = this._mkInput('text',
      'filename_regex (regex na nazwie pliku, np. ^report-.*\\.xlsx$)',
      this.state.filename_regex ?? '');
    this.refs.fnRe.addEventListener('input', () =>
      this._emit({ filename_regex: this.refs.fnRe.value }));
    body.appendChild(this.refs.fnRe);

    this.refs.pathRe = this._mkInput('text',
      'path_regex (regex na pełnej ścieżce)',
      this.state.path_regex ?? '');
    this.refs.pathRe.addEventListener('input', () =>
      this._emit({ path_regex: this.refs.pathRe.value }));
    body.appendChild(this.refs.pathRe);

    this.root.appendChild(body);
  }

  _mkInput(type, placeholder, value) {
    const i = document.createElement('input');
    i.type = type;
    i.placeholder = placeholder;
    i.value = value;
    i.className = 'w-full px-2 py-1 border border-gray-300 rounded text-xs '
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
    this.refs = {};
    this.root = null;
  }
}

function hasAnyField(s) {
  if (!s) return false;
  if (s.file_size_min || s.file_size_max) return true;
  if (Array.isArray(s.file_extensions) && s.file_extensions.length) return true;
  if (s.filename_regex || s.path_regex) return true;
  return false;
}

function csvSplit(s) {
  return s
    .split(',')
    .map((x) => x.trim())
    .filter((x) => x.length > 0);
}
