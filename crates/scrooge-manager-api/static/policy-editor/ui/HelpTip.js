// SPDX-License-Identifier: GPL-2.0-only
//
// Mały helper — generuje `?` bubble z natywnym `title` tooltipem na hover.
// Zero JS overhead, browser obsługuje tooltip sam.

/**
 * @param {string} text — treść tooltipa (plain text, browser sam łamie linie).
 * @returns {HTMLSpanElement}
 */
export function helpTip(text) {
  const tip = document.createElement('span');
  tip.className = 'inline-flex items-center justify-center ml-1 w-4 h-4 rounded-full '
    + 'bg-gray-200 text-gray-600 text-[10px] font-bold cursor-help select-none '
    + 'hover:bg-gray-300';
  tip.textContent = '?';
  tip.title = text;
  return tip;
}

/**
 * Owija label string + helpTip w jeden inline element.
 * @param {string} labelText
 * @param {string} tipText
 */
export function labelWithTip(labelText, tipText) {
  const wrap = document.createElement('span');
  wrap.className = 'inline-flex items-center';
  const lbl = document.createElement('span');
  lbl.textContent = labelText;
  wrap.appendChild(lbl);
  wrap.appendChild(helpTip(tipText));
  return wrap;
}
