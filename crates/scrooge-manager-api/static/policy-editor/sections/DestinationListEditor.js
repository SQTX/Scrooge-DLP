// SPDX-License-Identifier: GPL-2.0-only
//
// DestinationListEditor — analogiczny do SourceListEditor, ale dla destinations.
// + Add / × Remove, dropdown typu + pola zależne od typu.
//
// Destination types (z `scrooge_policy::rules::Destination`):
//   - directory       { paths: [string], recursive: bool }
//   - usb             { except_serials: [string] }
//   - network_upload  { domains: [string] }
//   - clipboard       (no fields)
//   - print           (no fields)
//   - any_external    (no fields)
//   - any             (no fields)

const DEST_TYPES = [
  { value: 'directory', label: 'directory' },
  { value: 'usb', label: 'usb' },
  { value: 'network_upload', label: 'network_upload' },
  { value: 'clipboard', label: 'clipboard' },
  { value: 'print', label: 'print' },
  { value: 'any_external', label: 'any_external' },
  { value: 'any', label: 'any' },
];

export class DestinationListEditor {
  /**
   * @param {{ mount: HTMLElement, initialState: object[], onChange: (dests: object[]) => void }} opts
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
    const label = document.createElement('div');
    label.className = 'text-xs uppercase tracking-wide text-gray-500 font-medium';
    label.textContent = 'Destinations';
    head.appendChild(label);

    const addBtn = document.createElement('button');
    addBtn.type = 'button';
    addBtn.className = 'text-xs text-blue-600 hover:text-blue-800';
    addBtn.textContent = '+ Add destination';
    addBtn.addEventListener('click', () => this._addItem());
    head.appendChild(addBtn);

    this.root.appendChild(head);

    if (this.items.length === 0) {
      const empty = document.createElement('div');
      empty.className = 'text-xs text-gray-400 italic px-3 py-2 border border-dashed border-gray-200 rounded';
      empty.textContent = 'Brak — rule bez destinations matchuje nothing. Dodaj przynajmniej 1.';
      this.root.appendChild(empty);
      return;
    }

    for (let i = 0; i < this.items.length; i++) {
      this.root.appendChild(this._renderItem(i));
    }
  }

  _addItem() {
    this.items.push({ type: 'usb', except_serials: [] });
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
  }

  _renderItem(idx) {
    const item = this.items[idx];
    const card = document.createElement('div');
    card.className = 'border border-gray-200 rounded p-2 mb-2 bg-white';

    const top = document.createElement('div');
    top.className = 'flex gap-2 items-center mb-2';

    const typeSel = document.createElement('select');
    typeSel.className = 'px-2 py-1 border border-gray-300 rounded text-xs';
    for (const t of DEST_TYPES) {
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

    const body = this._renderItemBody(idx, item);
    if (body) card.appendChild(body);

    return card;
  }

  _renderItemBody(idx, item) {
    switch (item.type) {
      case 'directory': return this._bodyDirectory(idx, item);
      case 'usb': return this._bodyUsb(idx, item);
      case 'network_upload': return this._bodyNetworkUpload(idx, item);
      case 'clipboard':
      case 'print':
      case 'any_external':
      case 'any':
        return null;
      default:
        return null;
    }
  }

  _bodyDirectory(idx, item) {
    const wrap = document.createElement('div');
    wrap.className = 'space-y-1';

    const pathsIn = this._csvInput('paths (csv, np. /tmp/exfil)',
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

  _bodyNetworkUpload(idx, item) {
    const wrap = document.createElement('div');
    const inp = this._csvInput(
      'domains (csv blokowane, np. *.dropbox.com, drive.google.com)',
      (item.domains ?? []).join(', '));
    inp.addEventListener('input', () =>
      this._patchItem(idx, { domains: csvSplit(inp.value) }));
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
    case 'network_upload': return { type, domains: [] };
    case 'clipboard':
    case 'print':
    case 'any_external':
    case 'any':
      return { type };
    default: return { type };
  }
}

function csvSplit(s) {
  return s
    .split(',')
    .map((x) => x.trim())
    .filter((x) => x.length > 0);
}
