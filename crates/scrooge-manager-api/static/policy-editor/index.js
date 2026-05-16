// SPDX-License-Identifier: GPL-2.0-only
//
// Entry point dla policy editor module'a. Wystawia `window.openPolicyEditor`
// dla istniejącego dashboardu (handlery przycisków w index.html).

import { PolicyEditor } from './PolicyEditor.js';

// Singleton — modal może być otwarty tylko raz na raz.
let instance = null;

window.openPolicyEditor = async (name) => {
  if (!instance) instance = new PolicyEditor();
  await instance.open(name);
};

window.closePolicyEditor = () => {
  if (instance) instance.close();
};
