// SPDX-License-Identifier: GPL-2.0-only
//
// SourceListEditor — dynamiczna lista source items, każdy z dropdown'em
// typu i polami zależnymi od typu. + Add, × Remove. Emit'uje całą tablicę
// na każdą zmianę.
//
// Source types (z `scrooge_policy::rules::Source`):
//   - directory       { paths: [string], recursive: bool }
//   - usb             { except_serials: [string] }
//   - network_download { domains: [string], domains_except: [string] }
//   - email_attachment (no fields)
//   - file_match      { classifiers: [string] }
//   - any             (no fields)

import { helpTip } from '../ui/HelpTip.js';

const SOURCE_TYPES = [
  { value: 'directory', label: 'directory' },
  { value: 'usb', label: 'usb' },
  { value: 'network_download', label: 'network_download' },
  { value: 'email_attachment', label: 'email_attachment' },
  { value: 'file_match', label: 'file_match' },
  { value: 'any', label: 'any' },
];

const HEADER_TIP =
  'Skąd dane pochodzą — co rule matchuje jako "source":\n\n'
  + '• directory — plik w lokalnym folderze (paths z glob, np. /srv/finance/**)\n'
  + '• usb — plik na podłączonym pendrive/dysku zewnętrznym '
  + '(except_serials = whitelist firmowych)\n'
  + '• network_download — plik pobrany z HTTP/HTTPS '
  + '(domains = lista źródeł, domains_except = whitelist)\n'
  + '• email_attachment — załącznik z klienta poczty (POP3/IMAP/Outlook)\n'
  + '• file_match — plik którego zawartość pasuje do nazwanego klasyfikatora '
  + '(credit-card-numbers, polish-pesel, iban-numbers itp.)\n'
  + '• any — wildcard (dowolne źródło)';

export class SourceListEditor {
  /**
   * @param {{ mount: HTMLElement, initialState: object[], onChange: (sources: object[]) => void }} opts
   */
  constructor({ mount, initialState, onChange }) {
    this.root = mount;
    this.items = Array.isArray(initialState) ? [...initialState] : [];
    this.onChange = onChange;
  }

  render() {
    while (this.root.firstChild) this.root.removeChild(this.root.firstChild);

    const head = document.createElement('div');
    head.className = 'flex items-center justify-between mb-2';
    const labelWrap = document.createElement('div');
    labelWrap.className = 'flex items-center';
    const label = document.createElement('div');
    label.className = 'text-xs uppercase tracking-wide text-gray-500 font-medium';
    label.textContent = 'Sources';
    labelWrap.appendChild(label);
    labelWrap.appendChild(helpTip(HEADER_TIP));
    head.appendChild(labelWrap);

    const addBtn = document.createElement('button');
    addBtn.type = 'button';
    addBtn.className = 'text-xs text-blue-600 hover:text-blue-800';
    addBtn.textContent = '+ Add source';
    addBtn.addEventListener('click', () => this._addItem());
    head.appendChild(addBtn);

    this.root.appendChild(head);

    if (this.items.length === 0) {
      const empty = document.createElement('div');
      empty.className = 'text-xs text-gray-400 italic px-3 py-2 border border-dashed border-gray-200 rounded';
      empty.textContent = 'Brak — rule bez sources matchuje nothing. Dodaj przynajmniej 1.';
      this.root.appendChild(empty);
      return;
    }

    for (let i = 0; i < this.items.length; i++) {
      this.root.appendChild(this._renderItem(i));
    }
  }

  _addItem() {
    this.items.push({ type: 'directory', paths: [], recursive: true });
    this._emit();
    this.render();
  }

  _removeItem(idx) {
    this.items.splice(idx, 1);
    this._emit();
    this.render();
  }

  _changeType(idx, newType) {
    this.items[idx] = blankForType(newType);
    this._emit();
    this.render();
  }

  _patchItem(idx, patch) {
    this.items[idx] = { ...this.items[idx], ...patch };
    this._emit();
    // Re-render nie potrzebny — pola tekstowe trzymają własną wartość.
  }

  _renderItem(idx) {
    const item = this.items[idx];
    const card = document.createElement('div');
    card.className = 'border border-gray-200 rounded p-2 mb-2 bg-white';

    // Top row: type select + × remove.
    const top = document.createElement('div');
    top.className = 'flex gap-2 items-center mb-2';

    const typeSel = document.createElement('select');
    typeSel.className = 'px-2 py-1 border border-gray-300 rounded text-xs';
    for (const t of SOURCE_TYPES) {
      const o = document.createElement('option');
      o.value = t.value;
      o.textContent = t.label;
      if (t.value === item.type) o.selected = true;
      typeSel.appendChild(o);
    }
    typeSel.addEventListener('change', () => this._changeType(idx, typeSel.value));
    top.appendChild(typeSel);

    const remBtn = document.createElement('button');
    remBtn.type = 'button';
    remBtn.className = 'ml-auto text-xs text-red-600 hover:text-red-800';
    remBtn.textContent = '× Remove';
    remBtn.addEventListener('click', () => this._removeItem(idx));
    top.appendChild(remBtn);

    card.appendChild(top);

    // Body — zależy od type.
    const body = this._renderItemBody(idx, item);
    if (body) card.appendChild(body);

    return card;
  }

  _renderItemBody(idx, item) {
    switch (item.type) {
      case 'directory': return this._bodyDirectory(idx, item);
      case 'usb': return this._bodyUsb(idx, item);
      case 'network_download': return this._bodyNetworkDownload(idx, item);
      case 'file_match': return this._bodyFileMatch(idx, item);
      case 'email_attachment':
      case 'any':
        return null; // brak pól
      default:
        return null;
    }
  }

  _bodyDirectory(idx, item) {
    const wrap = document.createElement('div');
    wrap.className = 'space-y-1';

    const pathsIn = this._csvInput('paths (oddzielone przecinkami, np. /srv/finance/**)',
      (item.paths ?? []).join(', '));
    pathsIn.addEventListener('input', () =>
      this._patchItem(idx, { paths: csvSplit(pathsIn.value) }));
    wrap.appendChild(pathsIn);

    const rec = document.createElement('label');
    rec.className = 'inline-flex items-center gap-1 text-xs text-gray-700 cursor-pointer';
    const cb = document.createElement('input');
    cb.type = 'checkbox';
    cb.checked = item.recursive !== false;
    cb.addEventListener('change', () => this._patchItem(idx, { recursive: cb.checked }));
    const txt = document.createElement('span');
    txt.textContent = 'recursive';
    rec.appendChild(cb);
    rec.appendChild(txt);
    wrap.appendChild(rec);

    return wrap;
  }

  _bodyUsb(idx, item) {
    const wrap = document.createElement('div');
    const inp = this._csvInput(
      'except_serials (whitelist firmowych pendrive\'ów, csv)',
      (item.except_serials ?? []).join(', '));
    inp.addEventListener('input', () =>
      this._patchItem(idx, { except_serials: csvSplit(inp.value) }));
    wrap.appendChild(inp);
    return wrap;
  }

  _bodyNetworkDownload(idx, item) {
    const wrap = document.createElement('div');
    wrap.className = 'space-y-1';

    const dom = this._csvInput('domains (csv, lista pasujących)',
      (item.domains ?? []).join(', '));
    dom.addEventListener('input', () =>
      this._patchItem(idx, { domains: csvSplit(dom.value) }));
    wrap.appendChild(dom);

    const exc = this._csvInput('domains_except (csv, whitelist)',
      (item.domains_except ?? []).join(', '));
    exc.addEventListener('input', () =>
      this._patchItem(idx, { domains_except: csvSplit(exc.value) }));
    wrap.appendChild(exc);

    return wrap;
  }

  _bodyFileMatch(idx, item) {
    const wrap = document.createElement('div');
    const inp = this._csvInput(
      'classifiers (csv, np. credit-card-numbers, polish-pesel)',
      (item.classifiers ?? []).join(', '));
    inp.addEventListener('input', () =>
      this._patchItem(idx, { classifiers: csvSplit(inp.value) }));
    wrap.appendChild(inp);
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

  _emit() {
    if (this.onChange) this.onChange([...this.items]);
  }

  getState() { return [...this.items]; }

  destroy() {
    while (this.root.firstChild) this.root.removeChild(this.root.firstChild);
    this.root = null;
  }
}

function blankForType(type) {
  switch (type) {
    case 'directory': return { type, paths: [], recursive: true };
    case 'usb': return { type, except_serials: [] };
    case 'network_download': return { type, domains: [], domains_except: [] };
    case 'email_attachment': return { type };
    case 'file_match': return { type, classifiers: [] };
    case 'any': return { type };
    default: return { type };
  }
}

function csvSplit(s) {
  return s
    .split(',')
    .map((x) => x.trim())
    .filter((x) => x.length > 0);
}
