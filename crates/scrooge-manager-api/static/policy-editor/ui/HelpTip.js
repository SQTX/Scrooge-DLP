// SPDX-License-Identifier: GPL-2.0-only
//
// `?` bubble z custom popoverem na klik. Natywne `title` ma 1-2s delay +
// brak styling — gorszy UX. Popover otwiera się na klik, zamyka na drugi
// klik / klik poza / Esc.

let activePopover = null;
let activeAnchor = null;
let activeOnDocClick = null;
let activeOnKey = null;

/**
 * @param {string} text — treść tooltipa (plain text, \n robi line breaks).
 * @returns {HTMLSpanElement}
 */
export function helpTip(text) {
  const tip = document.createElement('button');
  tip.type = 'button';
  tip.className = 'inline-flex items-center justify-center ml-1 w-4 h-4 rounded-full '
    + 'bg-gray-200 text-gray-600 text-[10px] font-bold cursor-help select-none '
    + 'hover:bg-gray-300 focus:outline-none focus:ring-2 focus:ring-blue-400';
  tip.textContent = '?';
  tip.setAttribute('aria-label', 'help');
  tip.addEventListener('click', (e) => {
    e.preventDefault();
    e.stopPropagation();
    if (activeAnchor === tip) {
      closePopover();
    } else {
      openPopover(tip, text);
    }
  });
  return tip;
}

function openPopover(anchor, text) {
  closePopover();

  const pop = document.createElement('div');
  pop.className = 'fixed z-[70] max-w-sm bg-gray-900 text-gray-100 text-xs '
    + 'rounded shadow-lg p-3 whitespace-pre-line leading-relaxed';
  pop.textContent = text;
  document.body.appendChild(pop);

  // Pozycja: poniżej kotwicy, wyrównane do lewej. Klamruj do viewport'u.
  const r = anchor.getBoundingClientRect();
  // Tymczasowo: lewa krawędź pod kotwicą.
  pop.style.top = (r.bottom + 6) + 'px';
  pop.style.left = r.left + 'px';
  // Po render — korekta jeśli wychodzi za prawą krawędź.
  const popRect = pop.getBoundingClientRect();
  const vw = window.innerWidth;
  if (popRect.right > vw - 8) {
    pop.style.left = Math.max(8, vw - popRect.width - 8) + 'px';
  }

  activePopover = pop;
  activeAnchor = anchor;

  // Klik poza popoverem / kotwicą → zamknij.
  activeOnDocClick = (e) => {
    if (pop.contains(e.target) || anchor.contains(e.target)) return;
    closePopover();
  };
  // Esc → zamknij.
  activeOnKey = (e) => { if (e.key === 'Escape') closePopover(); };
  // setTimeout: nie złap obecnego click event'u (bubble z helpTip click).
  setTimeout(() => {
    document.addEventListener('click', activeOnDocClick);
    document.addEventListener('keydown', activeOnKey);
  }, 0);
}

function closePopover() {
  if (activePopover && activePopover.parentNode) {
    activePopover.parentNode.removeChild(activePopover);
  }
  if (activeOnDocClick) document.removeEventListener('click', activeOnDocClick);
  if (activeOnKey) document.removeEventListener('keydown', activeOnKey);
  activePopover = null;
  activeAnchor = null;
  activeOnDocClick = null;
  activeOnKey = null;
}

/**
 * Owija label string + helpTip w jeden inline element.
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
