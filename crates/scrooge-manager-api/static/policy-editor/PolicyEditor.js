// SPDX-License-Identifier: GPL-2.0-only
//
// Root controller policy editora (modal). W obecnej iteracji (Sub-faza 2F.2)
// obsługuje tylko YamlMode — funkcjonalnie no-op względem starego inline
// kodu z index.html (parity check). FormMode + ModeSwitch + split view
// wjeżdżają w kolejnych sub-fazach.

import { YamlMode } from './modes/YamlMode.js';
import { ValidationPanel } from './ui/ValidationPanel.js';
import * as api from './api/policies.js';

export class PolicyEditor {
  constructor() {
    // DOM elements (modal istniejący w index.html, używamy istniejących id).
    this.modal = document.getElementById('policy-modal');
    this.modeLabel = document.getElementById('policy-modal-mode');
    this.bodyMount = document.getElementById('policy-yaml-input')?.parentElement; // body container
    this.validationMount = document.getElementById('policy-validation');
    this.validateBtn = document.getElementById('policy-validate-btn');
    this.saveBtn = document.getElementById('policy-save-btn');

    this.editingName = null;     // null = new, string = edit
    this.yamlMode = null;
    this.validation = null;

    // Bind once — będziemy add/remove listeners przy open/close.
    this._onValidateClick = () => this._validate();
    this._onSaveClick = () => this._save();
    this._onModalBackdropClick = (e) => {
      if (e.target === this.modal) this.close();
    };
  }

  /**
   * Otwórz modal. `name=null` → nowa polityka, `name="..."` → edycja.
   * @param {string|null} name
   */
  async open(name) {
    this.editingName = name;
    this.modeLabel.textContent = name ? 'Edit' : 'New';

    // Validation panel (re-mount na czysto).
    this.validation = new ValidationPanel({ mount: this.validationMount });
    this.validation.clear();

    // Initial YAML — dla edit pobierz z managera, dla new użyj template'u.
    let initialYaml;
    if (name) {
      try {
        const policy = await api.getPolicy(name);
        initialYaml = policy.content_yaml;
      } catch (err) {
        this._toast('Error loading policy: ' + err.message);
        return;
      }
    }

    // Stwórz mount point dla mode'u (zastępujemy stary textarea).
    // body container ma w sobie: <p>schema info</p>, <textarea>, <div#validation>.
    // Czyścimy textarea (jeśli zostało coś z poprzedniego open'a) i mountujemy YamlMode.
    const oldTextarea = document.getElementById('policy-yaml-input');
    if (oldTextarea) oldTextarea.remove();

    // Wstaw nowy mount dla mode'u przed validation div.
    const modeMount = document.createElement('div');
    this.bodyMount.insertBefore(modeMount, this.validationMount);

    this.yamlMode = new YamlMode({ mount: modeMount, initialYaml });
    this.yamlMode.render();

    // Podpinamy buttony.
    this.validateBtn.addEventListener('click', this._onValidateClick);
    this.saveBtn.addEventListener('click', this._onSaveClick);
    this.modal.addEventListener('click', this._onModalBackdropClick);

    this.modal.classList.remove('hidden');
  }

  close() {
    this.modal.classList.add('hidden');

    this.validateBtn.removeEventListener('click', this._onValidateClick);
    this.saveBtn.removeEventListener('click', this._onSaveClick);
    this.modal.removeEventListener('click', this._onModalBackdropClick);

    if (this.yamlMode) {
      this.yamlMode.destroy();
      this.yamlMode = null;
    }
    if (this.validation) {
      this.validation.destroy();
      this.validation = null;
    }
    this.editingName = null;
  }

  // ── private ────────────────────────────────────────────────────────────

  /** @returns {Promise<boolean>} czy YAML jest valid (do Save logic) */
  async _validate() {
    const yaml = this.yamlMode.getYaml();
    this.validation.loading();
    try {
      const resp = await api.validate(yaml);
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

    const yaml = this.yamlMode.getYaml();
    try {
      if (this.editingName) {
        const body = await api.update(this.editingName, yaml);
        this._toast('Policy updated (v' + body.version + ')');
      } else {
        await api.create(yaml);
        this._toast('Policy created');
      }
      this.close();
      // Refresh tabeli polityk — parent ma loadPolicies() jako global.
      if (typeof window.loadPolicies === 'function') window.loadPolicies();
    } catch (err) {
      this._toast('Save failed: ' + err.message);
    }
  }

  _toast(msg) {
    // Parent dashboardu eksponuje toast() — używamy gdy dostępne, fallback alert.
    if (typeof window.toast === 'function') window.toast(msg);
    else console.log('[toast]', msg);
  }
}
