// SPDX-License-Identifier: GPL-2.0-only
//
// Most cienki bridge do `authHeaders()` z parent dashboardu (index.html).
// Parent eksponuje `window.authHeaders` po inicjalizacji — moduł czeka aż
// będzie dostępne (najwyżej do `load` event'u).

/**
 * Zwraca nagłówki HTTP do uwierzytelnionych requestów REST.
 * Throw'uje gdy user nie zalogowany (parent handle'uje przekierowanie do login).
 * @returns {Promise<Record<string, string>>}
 */
export async function authHeaders() {
  if (typeof window.authHeaders !== 'function') {
    throw new Error('parent dashboard nie wystawił window.authHeaders');
  }
  return window.authHeaders();
}
