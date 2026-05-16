// SPDX-License-Identifier: GPL-2.0-only
//
// Generic confirm dialog — modal nad modalem. Promise-based: zwraca true
// (confirm) lub false (cancel/close).

/**
 * @param {{ title: string, message: string, items?: string[], confirmLabel?: string, cancelLabel?: string }} opts
 * @returns {Promise<boolean>}
 */
export function confirmDialog(opts) {
  return new Promise((resolve) => {
    // Backdrop.
    const backdrop = document.createElement('div');
    backdrop.className = 'fixed inset-0 bg-black bg-opacity-50 flex items-center '
      + 'justify-center z-[60] p-4';
    backdrop.tabIndex = -1;

    // Card.
    const card = document.createElement('div');
    card.className = 'bg-white rounded-lg max-w-md w-full shadow-2xl';

    // Header.
    const header = document.createElement('div');
    header.className = 'px-5 py-3 border-b border-gray-200';
    const title = document.createElement('h3');
    title.className = 'text-base font-semibold text-gray-900';
    title.textContent = opts.title ?? 'Potwierdź';
    header.appendChild(title);
    card.appendChild(header);

    // Body.
    const body = document.createElement('div');
    body.className = 'px-5 py-4 text-sm text-gray-700 space-y-2';
    const msg = document.createElement('p');
    msg.textContent = opts.message ?? '';
    body.appendChild(msg);
    if (Array.isArray(opts.items) && opts.items.length > 0) {
      const ul = document.createElement('ul');
      ul.className = 'list-disc pl-5 text-xs text-gray-600 space-y-0.5';
      for (const it of opts.items) {
        const li = document.createElement('li');
        li.textContent = it;
        ul.appendChild(li);
      }
      body.appendChild(ul);
    }
    card.appendChild(body);

    // Footer.
    const footer = document.createElement('div');
    footer.className = 'px-5 py-3 bg-gray-50 border-t border-gray-200 flex justify-end gap-2 rounded-b-lg';
    const cancelBtn = document.createElement('button');
    cancelBtn.type = 'button';
    cancelBtn.className = 'px-4 py-1.5 bg-gray-200 hover:bg-gray-300 text-gray-700 text-sm font-medium rounded';
    cancelBtn.textContent = opts.cancelLabel ?? 'Cancel';
    const confirmBtn = document.createElement('button');
    confirmBtn.type = 'button';
    confirmBtn.className = 'px-4 py-1.5 bg-blue-600 hover:bg-blue-700 text-white text-sm font-medium rounded';
    confirmBtn.textContent = opts.confirmLabel ?? 'Continue';
    footer.appendChild(cancelBtn);
    footer.appendChild(confirmBtn);
    card.appendChild(footer);

    backdrop.appendChild(card);
    document.body.appendChild(backdrop);

    const cleanup = (result) => {
      document.body.removeChild(backdrop);
      resolve(result);
    };
    cancelBtn.addEventListener('click', () => cleanup(false));
    confirmBtn.addEventListener('click', () => cleanup(true));
    backdrop.addEventListener('click', (e) => {
      if (e.target === backdrop) cleanup(false);
    });
    // Esc → cancel.
    const onKey = (e) => {
      if (e.key === 'Escape') {
        document.removeEventListener('keydown', onKey);
        cleanup(false);
      }
    };
    document.addEventListener('keydown', onKey);

    confirmBtn.focus();
  });
}
