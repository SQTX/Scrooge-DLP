// SPDX-License-Identifier: GPL-2.0-only
//
// YAML mode — wrapper nad textarea do swobodnej edycji YAMLa. Funkcjonalnie
// kopia obecnego policy editora (Sub-faza 2D), wyizolowana do klasy żeby
// PolicyEditor mógł ją wymieniać z FormMode w jednym modalu.

const DEFAULT_TEMPLATE =
  'apiVersion: scroogedlp.io/v1\n'
  + 'kind: Policy\n'
  + 'metadata:\n'
  + '  name: my-policy\n'
  + '  description: "Opis polityki"\n'
  + 'priority: 100\n'
  + 'rules:\n'
  + '  outbound:\n'
  + '    - id: block-finance-to-usb\n'
  + '      name: "Block finance files to USB"\n'
  + '      severity: high\n'
  + '      action: block\n'
  + '      sources:\n'
  + '        - type: directory\n'
  + '          paths: ["/srv/finance/**"]\n'
  + '      destinations:\n'
  + '        - type: usb\n';

export class YamlMode {
  /**
   * @param {{ mount: HTMLElement, initialYaml?: string, onChange?: (yaml: string) => void }} opts
   */
  constructor({ mount, initialYaml, onChange }) {
    this.root = mount;
    this.onChange = onChange;
    this.textarea = null;
    this.initialYaml = initialYaml ?? DEFAULT_TEMPLATE;
  }

  render() {
    while (this.root.firstChild) this.root.removeChild(this.root.firstChild);

    const ta = document.createElement('textarea');
    ta.id = 'policy-yaml-input';
    ta.className = 'w-full h-96 px-3 py-2 border border-gray-300 rounded font-mono text-xs '
      + 'focus:outline-none focus:ring-2 focus:ring-blue-500';
    ta.spellcheck = false;
    ta.value = this.initialYaml;

    ta.addEventListener('input', () => {
      if (this.onChange) this.onChange(ta.value);
    });

    this.root.appendChild(ta);
    this.textarea = ta;
  }

  getYaml() {
    return this.textarea ? this.textarea.value : this.initialYaml;
  }

  setYaml(yaml) {
    this.initialYaml = yaml;
    if (this.textarea) this.textarea.value = yaml;
  }

  destroy() {
    if (this.textarea) {
      this.textarea.remove();
      this.textarea = null;
    }
    this.root = null;
  }
}

export { DEFAULT_TEMPLATE };
