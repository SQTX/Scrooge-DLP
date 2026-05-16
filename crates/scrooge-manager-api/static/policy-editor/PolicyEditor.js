// SPDX-License-Identifier: GPL-2.0-only
//
// Root controller policy editora (modal). Orkiestruje:
// - Mode switching (Form ↔ YAML) z ModeSwitch w nagłówku.
// - Lifecycle FormMode (split view) i YamlMode (full-width textarea).
// - Validate / Save (woła backend API, aktualizuje ValidationPanel).
// - Modal width toggle (4xl YAML / 6xl Form — split view potrzebuje szerszego).

import { FormMode } from './modes/FormMode.js';
import { YamlMode, DEFAULT_TEMPLATE } from './modes/YamlMode.js';
import { ValidationPanel } from './ui/ValidationPanel.js';
import { ModeSwitch } from './ui/ModeSwitch.js';
import { formToYaml, defaultFormState } from './codec/formToYaml.js';
import * as api from './api/policies.js';

const PREVIEW_DEBOUNCE_MS = 150;

export class PolicyEditor {
  constructor() {
    this.modal = document.getElementById('policy-modal');
    this.modalCard = this.modal?.querySelector(':scope > div'); // wrapper z max-w-*
    this.modeLabel = document.getElementById('policy-modal-mode');
    this.headerEl = this.modeLabel?.closest('.flex.items-center.justify-between');
    this.bodyMount = document.getElementById('policy-validation')?.parentElement;
    this.validationMount = document.getElementById('policy-validation');
    this.validateBtn = document.getElementById('policy-validate-btn');
    this.saveBtn = document.getElementById('policy-save-btn');

    // State.
    this.editingName = null;
    this.mode = 'form';        // 'form' | 'yaml' — startowy default dla new
    this.formState = null;
    this.yamlText = '';        // single source of truth dla Validate/Save
    this.activeMode = null;    // referencja do FormMode lub YamlMode
    this.modeSwitch = null;
    this.modeSwitchMount = null;
    this.activeBodyMount = null;
    this.validation = null;
    this._previewTimer = null;

    this._onValidateClick = () => this._validate();
    this._onSaveClick = () => this._save();
    this._onBackdropClick = (e) => { if (e.target === this.modal) this.close(); };
  }

  /**
   * Otwórz modal. `name=null` → nowa polityka, `name="..."` → edycja.
   * @param {string|null} name
   */
  async open(name) {
    this.editingName = name;
    this.modeLabel.textContent = name ? 'Edit' : 'New';

    // Initial state — dla edit pobierz YAML, dla new użyj template'u + defaultFormState.
    if (name) {
      try {
        const policy = await api.getPolicy(name);
        this.yamlText = policy.content_yaml;
        this.formState = defaultFormState(); // 2F.10 doda yamlToForm parsing
        // 2F.10: zdecyduje smart default mode na podstawie supportability.
        this.mode = 'yaml';
      } catch (err) {
        this._toast('Error loading policy: ' + err.message);
        return;
      }
    } else {
      this.formState = defaultFormState();
      this.yamlText = formToYaml(this.formState);
      this.mode = 'form';
    }

    // ── Sprzątanie poprzedniego open'a ─────────────────────────────────
    this._unmountBody();

    // ── ModeSwitch w nagłówku (mount przed × button) ───────────────────
    if (!this.modeSwitchMount) {
      this.modeSwitchMount = document.createElement('div');
      this.modeSwitchMount.id = 'policy-mode-switch';
      this.modeSwitchMount.className = 'ml-4';
      // headerEl: <div flex items-center justify-between>
      //   <h3>...</h3>     ← mode label
      //   <button>×</button>
      // Wstawiamy mode switch przed × button żeby się trzymał w header'ze.
      const closeBtn = this.headerEl.querySelector('button');
      this.headerEl.insertBefore(this.modeSwitchMount, closeBtn);
    }
    this.modeSwitch = new ModeSwitch({
      mount: this.modeSwitchMount,
      initialMode: this.mode,
      onSwitch: (m) => this._handleSwitch(m),
    });
    this.modeSwitch.render();

    // ── Validation panel ──────────────────────────────────────────────
    this.validation = new ValidationPanel({ mount: this.validationMount });
    this.validation.clear();

    // ── Mount aktualny mode ───────────────────────────────────────────
    this._mountMode();

    // ── Buttony ──────────────────────────────────────────────────────
    this.validateBtn.addEventListener('click', this._onValidateClick);
    this.saveBtn.addEventListener('click', this._onSaveClick);
    this.modal.addEventListener('click', this._onBackdropClick);

    this.modal.classList.remove('hidden');
  }

  close() {
    this.modal.classList.add('hidden');

    this.validateBtn.removeEventListener('click', this._onValidateClick);
    this.saveBtn.removeEventListener('click', this._onSaveClick);
    this.modal.removeEventListener('click', this._onBackdropClick);

    this._unmountBody();

    if (this.modeSwitch) {
      this.modeSwitch.destroy();
      this.modeSwitch = null;
    }
    if (this.modeSwitchMount && this.modeSwitchMount.parentNode) {
      this.modeSwitchMount.parentNode.removeChild(this.modeSwitchMount);
      this.modeSwitchMount = null;
    }
    if (this.validation) {
      this.validation.destroy();
      this.validation = null;
    }

    this.editingName = null;
  }

  // ── private ────────────────────────────────────────────────────────────

  _mountMode() {
    // Stwórz mount point dla aktywnego mode'u (wstawiamy przed validation div).
    this.activeBodyMount = document.createElement('div');
    this.bodyMount.insertBefore(this.activeBodyMount, this.validationMount);

    // Modal width: Form mode potrzebuje szerszego (split view), YAML wąski.
    this._setModalWidth(this.mode === 'form' ? '6xl' : '4xl');

    if (this.mode === 'form') {
      this.activeMode = new FormMode({
        mount: this.activeBodyMount,
        initialFormState: this.formState,
        onChange: (fs) => this._handleFormChange(fs),
      });
    } else {
      this.activeMode = new YamlMode({
        mount: this.activeBodyMount,
        initialYaml: this.yamlText,
        onChange: (txt) => { this.yamlText = txt; },
      });
    }
    this.activeMode.render();
  }

  _unmountBody() {
    if (this._previewTimer) {
      clearTimeout(this._previewTimer);
      this._previewTimer = null;
    }
    if (this.activeMode) {
      this.activeMode.destroy();
      this.activeMode = null;
    }
    if (this.activeBodyMount && this.activeBodyMount.parentNode) {
      this.activeBodyMount.parentNode.removeChild(this.activeBodyMount);
      this.activeBodyMount = null;
    }
  }

  _handleFormChange(newFormState) {
    this.formState = newFormState;
    // Debounce regeneracji YAML preview — przy szybkim typing'u keystroke'i
    // grupują się i regeneracja idzie raz na ~150ms.
    if (this._previewTimer) clearTimeout(this._previewTimer);
    this._previewTimer = setTimeout(() => {
      this.yamlText = formToYaml(this.formState);
      if (this.activeMode && typeof this.activeMode.updatePreview === 'function') {
        this.activeMode.updatePreview(this.yamlText);
      }
      this._previewTimer = null;
    }, PREVIEW_DEBOUNCE_MS);
  }

  _handleSwitch(target) {
    if (target === this.mode) return;
    // 2F.10: tu wejdzie yamlToForm + ConfirmDialog przy YAML→Form z unsupported.
    // Na razie prosty switch — Form→YAML zawsze działa (yamlText z formToYaml),
    // YAML→Form pomija parsing (resetuje formState do default + ghost).
    if (target === 'yaml') {
      // Zapisz wygenerowany YAML jako tekst do edycji.
      this.yamlText = formToYaml(this.formState);
    } else {
      // Form ← YAML: na razie reset do default formState. Pełen parsing 2F.10.
      this.formState = defaultFormState();
    }
    this.mode = target;
    this._unmountBody();
    this._mountMode();
    this.modeSwitch.setMode(target);
  }

  _setModalWidth(size) {
    // Tailwind 4xl / 6xl — toggle na wrapper'ze max-w-*.
    if (!this.modalCard) return;
    this.modalCard.classList.remove('max-w-4xl', 'max-w-6xl');
    this.modalCard.classList.add(size === '6xl' ? 'max-w-6xl' : 'max-w-4xl');
  }

  async _validate() {
    // yamlText jest single source of truth dla Validate — w form mode
    // został wygenerowany przez debounce (lub przy switchu), w yaml mode
    // jest aktualizowany na każde keystroke przez onChange.
    if (this.mode === 'form') {
      // Wymuś świeży regenerate (debounce mogło nie odpalić jeszcze).
      this.yamlText = formToYaml(this.formState);
    }
    this.validation.loading();
    try {
      const resp = await api.validate(this.yamlText);
      if (resp.valid) {
        this.validation.ok(resp);
        return true;
      }
      this.validation.errors(resp.errors);
      return false;
    } catch (err) {
      this.validation.fail('Validate error: ' + err.message);
      return false;
    }
  }

  async _save() {
    const ok = await this._validate();
    if (!ok) return;
    try {
      if (this.editingName) {
        const body = await api.update(this.editingName, this.yamlText);
        this._toast('Policy updated (v' + body.version + ')');
      } else {
        await api.create(this.yamlText);
        this._toast('Policy created');
      }
      this.close();
      if (typeof window.loadPolicies === 'function') window.loadPolicies();
    } catch (err) {
      this._toast('Save failed: ' + err.message);
    }
  }

  _toast(msg) {
    if (typeof window.toast === 'function') window.toast(msg);
    else console.log('[toast]', msg);
  }
}

// Eksport stałych przydatnych dla testów / debug.
export { DEFAULT_TEMPLATE };
