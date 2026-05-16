# Policy editor — dual-mode (Form + YAML, split-view)

**Status:** design (brainstormed 2026-05-16) — gotowy do implementation plan
**Affects:** głównie `crates/scrooge-manager-api/static/` (frontend). Plus drobne rozszerzenie `ValidateResponse` w `handlers/policies.rs` o pole `parsed: Option<Policy>` (potrzebne dla YAML→Form parsingu).
**Phase:** post-2E poprawka, przed PR `dev → main` + tag `v0.1.1`

---

## 1. Cel

Obecny policy editor (Sub-faza 2D) ma tylko jeden tryb: surowy YAML w `<textarea>`. Wymaga znajomości schemy `scroogedlp.io/v1`. Cel: dodać **drugi tryb (Form)** który prowadzi usera przez tworzenie polityki za pomocą dropdownów i pól tekstowych — jak edytor reguł firewalla. Form generuje YAML automatycznie, więc obie ścieżki kończą się tym samym artefaktem wysyłanym do `POST /api/v1/policies`.

**Co osiągamy:**
- Nowy user może utworzyć działającą politykę bez czytania docsa schemy.
- Power user nadal ma pełną kontrolę przez YAML mode.
- Form generuje YAML *na żywo* — user widzi obok co tworzy, naturalnie uczy się składni.

**Czego NIE robimy w tym specu:** clipboard/USB/fs enforce na agencie (Phase 3+), zmiany schemy YAMLa, zmiany w gRPC stream / agent-side. Backend ruszamy minimalnie — tylko rozszerzenie `ValidateResponse` o `parsed: Option<Policy>`.

---

## 2. Decyzje brainstormingu

| Pytanie | Wybór |
|---------|-------|
| Format reguł | YAML (potwierdzone — wcześniejsze "yara" było typo) |
| Scope form mode | **Standard** — metadata + 1 outbound rule (severity, sources, destinations, conditions, action, notify_user/message, forward_to_siem). BEZ targets, monitored_paths, inbound, classifiers. |
| Source of truth | **YAML**. Form generuje YAML. YAML→Form parsuje + ostrzega o unsupported. |
| Default tryb | **Smart**: nowa policy → Form. Edit istniejącej → Form jeśli form-friendly, inaczej YAML. |
| Layout form mode | **Split view** — pola po lewej, generowany YAML preview (read-only) po prawej, aktualizowany live (debounce ~150ms). |
| Layout YAML mode | Full-width edytowalna textarea (jak dziś). |
| Architektura JS | **Wydzielony moduł** `static/policy-editor/` z osobnymi klasami per komponent. ES Modules, klasy ES6, zero build step. |
| Testy JS | Brak. Ręczny E2E + backend Validate jako gate. |

---

## 3. Architektura kodu

### 3.1 Lokalizacja

```
crates/scrooge-manager-api/static/
├── index.html                    # diff: usuwamy stary modal body + JS;
│                                 # dodajemy <script type="module" src="/static/policy-editor/index.js">
└── policy-editor/                # nowy folder
    ├── index.js                  # entry point, eksportuje window.openPolicyEditor(name)
    ├── PolicyEditor.js           # root controller
    ├── modes/
    │   ├── FormMode.js           # form mode (split view)
    │   └── YamlMode.js           # yaml mode (full-width textarea)
    ├── sections/
    │   ├── MetadataSection.js
    │   ├── RuleSection.js
    │   ├── SourceListEditor.js
    │   ├── DestinationListEditor.js
    │   └── ConditionsSection.js
    ├── codec/
    │   ├── formToYaml.js         # FormState → string YAML (deterministic)
    │   ├── yamlToForm.js         # parse YAML → { formState, unsupported, ghost }
    │   └── supportability.js     # reguły co Standard form pokrywa
    ├── api/
    │   ├── policies.js           # cienki wrapper REST API
    │   └── auth.js               # re-export authHeaders() z parent dashboardu
    └── ui/
        ├── ModeSwitch.js
        ├── ValidationPanel.js
        └── ConfirmDialog.js
```

Axum `tower_http::ServeDir` już serwuje cały `static/` rekursywnie — żadne zmiany w Rust routerze.

### 3.2 Stack

- **Vanilla ES2022**, `<script type="module">` — browser-native, zero transpilation.
- **Tailwind CDN** zostaje, jak w obecnym dashboardzie.
- **Zero zewnętrznych libów** (żadnego YAML libka po stronie frontendu — formToYaml generuje stringi ręcznie, yamlToForm używa `serde_yaml` po stronie backendu przez nowy lub istniejący endpoint).
- **Brak build stepu, brak Node toolchaina** — spójne z CLAUDE.md ("vanilla HTML/JS + Tailwind CDN").

### 3.3 Konwencja klas

Każda klasa-komponent:

```js
class FooComponent {
  constructor({ mount, initialState, onEmit }) {
    this.root = mount;       // DOM element gdzie komponent rysuje
    this.state = initialState;
    this.onEmit = onEmit;    // callback do parenta: (event, payload) => void
  }
  render() { /* idempotentne — pisze HTML do this.root */ }
  getState() { return this.state; }
  setState(s) { this.state = s; this.render(); }
  destroy() { /* odłącz event listenery, this.root = null */ }
}
```

- **Brak globalnego state managera.** Root (`PolicyEditor`) trzyma form state, slice'y idą w dół przez `setState`.
- **Brak two-way binding.** Wykonawca emit'uje `onEmit('change', payload)`, parent decyduje co z tym zrobić.
- **Każdy komponent zna tylko swój `this.root`.** Żadnego `document.querySelector` poza własnym sub-treem.

---

## 4. Komponenty

### 4.1 `PolicyEditor` (root)

**Stan:**
```js
{
  mode: 'form' | 'yaml',
  formState: {           // pola form mode'a
    metadata: { name, description, priority },
    rule: { id, name, severity, action, message, notify_user, forward_to_siem },
    sources: [Source],
    destinations: [Destination],
    conditions: { file_size_min, file_size_max, file_extensions, filename_regex, path_regex }
  },
  yamlText: string,      // current YAML text (in yaml mode = edited by user;
                         //                    in form mode = generated, read-only)
  ghost: { unsupportedYaml: string|null },  // preserved bits gdy switched YAML→Form
  dirty: boolean,
  lastValidation: { valid, errors, sha256, msgpack_size } | null,
}
```

**Odpowiedzialności:**
- Lifecycle modala (open/close, dirty check przy close).
- Komponowanie podkomponentów per `mode`.
- Orkiestracja Validate / Save (woła API, aktualizuje `ValidationPanel`).
- Obsługa Mode switcha (z `yamlToForm` supportability checkiem dla kierunku YAML→Form).
- Debounce regeneracji YAML preview gdy mode='form' (~150ms po ostatnim onEmit z sekcji).

### 4.2 `FormMode`

- Mount: split view (CSS grid 2 kolumny, lewa = pola, prawa = YAML `<pre>` read-only).
- Komponuje wszystkie `*Section` komponenty.
- Otrzymuje `formState` z parenta, przekazuje slice'y per sekcji.
- Każda sekcja emit'uje `change` → FormMode agreguje → emit do PolicyEditor.

### 4.3 `YamlMode`

- Mount: full-width `<textarea>` (kopia obecnego `#policy-yaml-input` ale w module).
- Emit'uje `change` na każde keystroke (bez debounce — to leci do state, nie do regeneracji).
- Backspace, tab handling (tab = 2 spaces, jak w istniejącym).

### 4.4 Sekcje form (każda osobna klasa)

| Sekcja | Pola | Generowany YAML fragment |
|--------|------|--------------------------|
| `MetadataSection` | name (text), description (text), priority (number) | `metadata: { name, description }, priority: N` |
| `RuleSection` | rule id (text), name (text), severity (select: low/medium/high/critical), action (select: log_only/log_and_alert/warn_user/block/quarantine/lock_workstation/kill_process), message (text), notify_user (checkbox), forward_to_siem (checkbox) | `rules.outbound[0].{id,name,severity,action,message,notify_user,forward_to_siem}` |
| `SourceListEditor` | lista source items, każdy z `+ Add`/`× Remove`. Per item: dropdown type (directory/usb/network_download/email_attachment/file_match/any) + pola zależne od type (np. directory → paths textarea, usb → except_serials csv input) | `rules.outbound[0].sources: [...]` |
| `DestinationListEditor` | analogicznie, types: directory/usb/network_upload/clipboard/print/any_external/any | `rules.outbound[0].destinations: [...]` |
| `ConditionsSection` | collapsible (domyślnie zwinięte). Pola: file_size_min (text z hintem "1KB / 2MB"), file_size_max, file_extensions (csv: "xlsx,pdf"), filename_regex (text), path_regex (text) | `rules.outbound[0].conditions: {...}` |

### 4.5 UI helpers

- `ModeSwitch` — toggle Form ◉ ━ ○ YAML w nagłówku modala, emit `switch` z nowym mode.
- `ValidationPanel` — kawałek pod body, `setState({ valid, errors, sha256, msgpack_size })` rysuje zielony OK z sha256/rozmiarem albo czerwoną listę błędów.
- `ConfirmDialog` — generic modal-na-modalu do confirm dialogów (close-dirty, YAML→Form-unsupported).

### 4.6 Codec

- `formToYaml(formState, ghostYaml)` → `string`.
  - Generuje deterministyczny YAML w stałym porządku pól.
  - Jeśli `ghostYaml` istnieje (po wcześniejszym YAML→Form z unsupported), merguje na poziomie roota (zachowuje top-level klucze poza tymi które form ma).
  - Manualne stringi (template literals + indent helper), bez external libka.
- `yamlToForm(yamlText)` → `{ ok: true, formState, unsupported: string[], ghostYaml: string|null }` albo `{ ok: false, error: string }`.
  - **NIE parsuje YAMLa po stronie frontendu** — `serde_yaml` po stronie Rust jest jednym source of truth dla reguł parsowania i walidacji, żadnego dryfu.
  - **Wymaga rozszerzenia backendu**: obecny `ValidateResponse` zwraca `{valid, errors, hash, name, version}` — brakuje parsed structure. Dodajemy pole `parsed: Option<Policy>` (Some gdy `valid=true`, None gdy invalid). Mała zmiana w `crates/scrooge-manager-api/src/handlers/policies.rs::validate_policy()` — dosłownie 1 dodatkowe pole + propagacja `parsed` ze ścieżki `Ok` w match. Plus `#[derive(Serialize, ToSchema)]` na `Policy` (lub osobny `PolicyDTO` jeśli wewnętrzny `Policy` ma trickowe pola jak `FileSize`).
  - `yamlToForm` wywołuje `POST /api/v1/policies/validate { content_yaml: yamlText }` → bierze `response.parsed` → konwertuje do `formState`.
- `supportability.js` — `isFormFriendly(parsedPolicy) → bool` + `listUnsupported(parsed) → ['inbound rules', 'targets.os', 'monitored_paths', ...]`.

### 4.7 API wrapper

- `api/policies.js`: `getPolicy(name)`, `validate(yaml)`, `create(yaml)`, `update(name, yaml)`.
- `api/auth.js`: re-export `authHeaders()` z parent `index.html` (na razie udostępnione przez `window.authHeaders`). Decyzja w plan-stage czy refactor do common'a.

---

## 5. Data flow

### 5.1 User pisze w form mode

```
user keystroke w MetadataSection.name input
  → MetadataSection.onEmit('change', { name: 'foo' })
  → FormMode merguje → FormMode.onEmit('change', formState)
  → PolicyEditor: state.formState = newFormState, state.dirty = true
  → debounce 150ms → state.yamlText = formToYaml(formState, ghost)
  → FormMode prawy panel (YAML preview) setState({ text: state.yamlText })
```

YAML preview render to po prostu `<pre class="text-xs font-mono">${escapeHtml(yaml)}</pre>` — bez syntax highlighting (YAGNI; jeśli chcemy potem, można dorzucić malutkie regex-based coloring 2 linijki kodu).

### 5.2 Switch Form → YAML

```
ModeSwitch.onEmit('switch', 'yaml')
  → PolicyEditor: state.yamlText już aktualny (debounce zdążył)
  → destroy FormMode (cleanup event listeners)
  → mount YamlMode z initial = state.yamlText
  → ModeSwitch.setState({ active: 'yaml' })
```

### 5.3 Switch YAML → Form

```
ModeSwitch.onEmit('switch', 'form')
  → const result = await yamlToForm(state.yamlText)
  → if (!result.ok): toast(result.error); zostań w YAML; abort
  → if (result.unsupported.length > 0):
       ConfirmDialog: "YAML zawiera: [inbound rules, monitored_paths].
                      Po przełączeniu form ich nie pokaże, ale pozostają
                      w YAML i Save je zachowa. Kontynuować?"
       ├─ Cancel → abort
       └─ Continue ↓
  → state.formState = result.formState
  → state.ghost.unsupportedYaml = result.ghostYaml
  → destroy YamlMode, mount FormMode
  → ModeSwitch.setState({ active: 'form' })
```

### 5.4 Validate

```
user klika "Validate (dry-run)"
  → const yaml = state.yamlText (zawsze aktualny w obu trybach)
  → ValidationPanel.setState({ status: 'loading' })
  → POST /api/v1/policies/validate { content_yaml: yaml }
  → response → state.lastValidation = response
  → ValidationPanel.setState(response)
```

### 5.5 Save

```
user klika "Save"
  → const yaml = state.yamlText
  → if mode='form' z ghost: yaml = mergeWithGhost(formToYaml(formState), ghost)
  → POST /api/v1/policies (create) lub PUT /api/v1/policies/{name} (edit)
  → response 2xx → closePolicyModal(); refresh table; toast('Saved')
  → response 4xx/5xx → ValidationPanel.setState({ errors: [serverError] })
```

---

## 6. Smart default + supportability

### 6.1 Otwieranie modala

- **+ New policy** → `mode='form'`, `formState = defaultFormState()`.
- **Edit existing policy**:
  1. `getPolicy(name)` → pobierz `content_yaml`.
  2. `yamlToForm(content_yaml)` → result.
  3. Jeśli `result.ok && result.unsupported.length === 0` → `mode='form'`.
  4. Inaczej → `mode='yaml'`, `state.yamlText = content_yaml`, subtle hint pod ModeSwitch: "Ta polityka używa funkcji których form nie pokrywa".

### 6.2 Co Standard form pokrywa (form-friendly definition)

Polityka jest "form-friendly" wtw gdy:
- ✅ `rules.outbound.length === 1`
- ✅ `rules.inbound` jest puste lub `undefined`
- ✅ `targets` jest puste lub `undefined`
- ✅ `monitored_paths` jest puste lub `undefined`
- ✅ `classifiers` jest puste lub `undefined`

Wszystko inne (np. 0 outbound, 2+ outbound, jakikolwiek inbound, targets, monitored_paths, classifiers) → not form-friendly.

### 6.3 Lista unsupported features (dla ConfirmDialog)

Każdy z poniższych dodaje wpis do `unsupported[]`:
- `rules.outbound.length > 1` → "wiele reguł outbound"
- `rules.inbound.length > 0` → "reguły inbound"
- `targets.match` lub `targets.exclude` non-empty → "targets (filtering agentów)"
- `monitored_paths.length > 0` → "monitored paths (filesystem watcher)"
- `classifiers.length > 0` → "classifiers"

---

## 7. Edge cases + error handling

| Scenariusz | Zachowanie |
|------------|------------|
| Form mode, user wpisał `[unclosed` w regex | Form nic nie sygnalizuje. Backend Validate zwraca błąd, ValidationPanel pokazuje. |
| YAML mode, syntactically broken YAML | Form mode niedostępny (lewy panel ukryty). Validate w textarea pokazuje błąd. Save woła Validate first — jeśli invalid, Save odrzucone. |
| Switch YAML→Form ale YAML invalid | `yamlToForm` zwraca `{ok: false}` → toast "Najpierw napraw YAML"; switch zablokowany. |
| Form-side incompletna polityka (sources=[], destinations=[]) | Form generuje YAML jaki ma. Backend Validate przyjmuje (valid syntactically). Save działa. (Future: warning, ale YAGNI.) |
| Save fail 409 (nazwa zajęta) | ValidationPanel: "Policy already exists. Try Edit." Modal nie zamyka, user może poprawić. |
| Save fail 412 (concurrent edit) | ValidationPanel: "Polityka zmieniona przez kogoś innego. Reload?" z przyciskiem Reload (re-fetch policy, override state). |
| Network error | ValidationPanel: "Nie udało się połączyć z managerem." Bez retry button, user klika Validate sam. |
| Close modal z `dirty=true` | ConfirmDialog: "Niezapisane zmiany zostaną stracone. Zamknąć?" Cancel/Confirm. |

---

## 8. Out of scope (YAGNI)

Świadomie nie robimy w tej iteracji:
- ❌ Multi-rule editor w form mode (tylko 1 outbound, więcej → YAML mode)
- ❌ Form mode dla `targets` / `monitored_paths` / `classifiers` / `inbound`
- ❌ YAML syntax highlighting w preview pane (zwykły `<pre>` wystarczy)
- ❌ Client-side YAML parser (zostawiamy backend, jeden source of truth dla parsing rules)
- ❌ Unit testy JS (Rust tests + manual E2E)
- ❌ Auto-save / draft persistence (closing modal = lost changes z confirmem)
- ❌ Field-level live validation (regex check w przeglądarce) — backend zna te reguły, nie duplikujemy
- ❌ Schema-driven form generation (backend `GET /api/v1/policies/schema` endpoint) — hardkodujemy enum'y w JS dla Standard scope

Jeśli któraś z tych rzeczy okaże się konieczna po użyciu w prod — refactor w osobnej iteracji.

---

## 9. Test plan

**Automatyczne:** brak nowych testów (vanilla JS, brak Node toolchaina w projekcie).

**Manualne E2E (na żywej VM, jak w Phase 2):**
1. New policy → Form mode, wypełnij metadata + jedną outbound rule (sources=directory, destinations=usb, action=block) → YAML preview po prawej pokazuje wygenerowany YAML → Validate → OK → Save → polityka w tabeli.
2. New policy → Form mode → przełącz na YAML mode → edytuj ręcznie, dodaj `targets.match.os: ["linux"]` → Validate OK → przełącz z powrotem na Form mode → ConfirmDialog "targets niekomptaybilne". Cancel → zostań w YAML. Save → polityka w tabeli z targets.
3. Edit istniejącej polityki (z punktu 2, ma targets) → modal otwiera się w **YAML mode** (smart default) z hintem.
4. Edit istniejącej polityki bez targets (z punktu 1) → modal otwiera się w **Form mode**, pola wypełnione.
5. New policy → Form mode → wpisz regex `[unclosed` w conditions.filename_regex → Validate → ValidationPanel pokazuje błąd regex compile.
6. Otwórz form mode, wpisz coś, zamknij modal "×" → ConfirmDialog "niezapisane zmiany".

**Backend regression:** żaden plik Rust nie tknięty, ale po implementacji puścić `cargo test --workspace` żeby się upewnić.

---

## 10. Plan implementacji (outline — szczegóły w writing-plans)

Sub-zadania w kolejności (każde własny commit, Conventional Commits format):

1. **Backend: ValidateResponse + `parsed: Option<Policy>`** — rozszerzenie `handlers/policies.rs::validate_policy()` + `#[derive(Serialize, ToSchema)]` na `Policy` (lub `PolicyDTO`). Test: `cargo test --workspace`. Swagger UI musi pokazywać nowe pole.
2. **Setup struktury** — `static/policy-editor/` skeleton z 4 plikami (index.js, PolicyEditor.js, ModeSwitch.js, ValidationPanel.js) + diff w `index.html` (script module import, usunięcie starego inline JS dla policy modala, `window.authHeaders = authHeaders;` jako bridge).
3. **YamlMode** — wyizolowanie istniejącego textarea do osobnej klasy. Funkcjonalnie no-op (cały dashboard musi nadal działać identycznie po tym commicie).
4. **API wrapper + auth bridge** — `api/policies.js`, `api/auth.js`.
5. **FormMode skeleton** — split view layout (CSS grid), pusty lewy panel + YAML preview po prawej, debounce 150ms.
6. **MetadataSection + RuleSection** — pierwsze 2 sekcje. End-to-end formToYaml dla podstawowych pól.
7. **SourceListEditor** — najbardziej skomplikowane (dynamiczna lista + per-item subform). 6 source types.
8. **DestinationListEditor** — analogicznie, 7 destination types.
9. **ConditionsSection** — collapsible, 5 pól.
10. **yamlToForm + supportability** — używa rozszerzonego endpointa Validate (zadanie 1) + form-friendly check + ghost preservation.
11. **Smart default w openPolicyEditor()** — wybór mode'u przy otwieraniu, hint dla unsupported.
12. **ConfirmDialog** + integracja w switch YAML→Form i close-dirty.
13. **Manualny E2E na VM** — wszystkie 6 scenariuszy z sekcji 9, dokumentacja w MILESTONE/CLAUDE.md update.

Approx 13 commitów. Po wszystkim → PR `dev → main` + tag `v0.1.1`.

---

## Notatki autorskie

- Spec jest świadomie zachowawczy w scope (Standard form, brak schema-driven). Po pierwszym użyciu w prod (lub feedbacku usera) — wracamy.
- Backend ruszamy minimalnie (1 plik, 1 nowe pole w ValidateResponse). Reszta pracy = frontend.
