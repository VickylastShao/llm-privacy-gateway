# Single-Page Internationalized Desktop UX Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the placeholder-oriented desktop navigation with a bilingual single-page connection workflow, expose the existing rotate/delete operations safely, improve controls and menus, and suppress the Windows release console window.

**Architecture:** Keep the Rust gateway and Tauri command contracts unchanged except for the Windows entry-point attribute. Split the React control plane into typed desktop-service, localization, model/helper, application, and focused presentation components so behavior can be tested with an injected fake desktop API. Keep all locale data and sensitive-data processing local.

**Tech Stack:** React 19, TypeScript, Vite, Vitest, Testing Library, jsdom, Tauri 2, Rust 1.97, CSS.

---

## File map

```text
apps/desktop/src/main.tsx                         React mount only
apps/desktop/src/App.tsx                          Single-page orchestration and state
apps/desktop/src/desktop-api.ts                   Typed Tauri invoke boundary
apps/desktop/src/model.ts                         Shared types and local URL helper
apps/desktop/src/i18n.ts                          Typed zh-CN/en catalog and persistence
apps/desktop/src/components/ConnectionForm.tsx   Connection creation and advanced policy
apps/desktop/src/components/ConnectionCard.tsx   Profile summary and actions
apps/desktop/src/components/OverflowMenu.tsx      Accessible low-frequency action popover
apps/desktop/src/components/OneTimeSecret.tsx     One-time local configuration result
apps/desktop/src/components/ConfirmDialog.tsx     Rotate/delete confirmation
apps/desktop/src/styles.css                       Single-page visual system and responsive rules
apps/desktop/src/test/setup.ts                    DOM test cleanup and browser stubs
apps/desktop/src/*.test.ts(x)                     Locale and application behavior tests
apps/desktop/src-tauri/src/main.rs                 Release-only Windows GUI subsystem
apps/desktop/src/windows-entry.test.ts             Cross-platform console regression source check
package.json / package-lock.json                  DOM testing dependencies and scripts
vite.config.ts                                    jsdom test environment and setup
```

### Task 1: Add a real frontend behavior-test harness

**Files:**
- Modify: `package.json`
- Modify: `package-lock.json`
- Modify: `vite.config.ts`
- Create: `apps/desktop/src/test/setup.ts`
- Replace: `apps/desktop/src/main.test.tsx`

- [ ] **Step 1: Install the test-only DOM dependencies**

Run:

```bash
PATH=/home/shao/.nvm/versions/node/v24.13.1/bin:$PATH npm install --save-dev @testing-library/dom @testing-library/react @testing-library/user-event jsdom
```

Expected: package files include the four dev dependencies; production dependencies do not change.

- [ ] **Step 2: Configure Vitest for jsdom and deterministic cleanup**

Set `vite.config.ts` test configuration to:

```ts
test: {
  environment: "jsdom",
  include: ["apps/desktop/src/**/*.test.ts", "apps/desktop/src/**/*.test.tsx"],
  setupFiles: ["apps/desktop/src/test/setup.ts"],
}
```

The setup file must call Testing Library `cleanup()` after every test, clear `localStorage`, restore Vitest mocks, and define a writable `navigator.clipboard.writeText` mock.

- [ ] **Step 3: Replace string-only tests with a failing shell test**

Write a test that renders the exported `App` with an injected fake `DesktopApi` returning no profiles and asserts that the three first-run steps and one connection form are visible, while `Dashboard`, `Policies`, `Test Console`, and `Activity` are absent.

The service boundary used by the test and production app is:

```ts
export interface DesktopApi {
  gatewayStatus(): Promise<GatewayStatus>;
  startGateway(): Promise<GatewayStatus>;
  stopGateway(): Promise<GatewayStatus>;
  listProfiles(): Promise<ProfileSummary[]>;
  createProfile(draft: ProfileDraft): Promise<CreatedProfile>;
  rotateLocalKey(id: string): Promise<RotatedLocalKey>;
  deleteProfile(id: string): Promise<void>;
}
```

- [ ] **Step 4: Verify RED**

Run:

```bash
PATH=/home/shao/.nvm/versions/node/v24.13.1/bin:$PATH npx vitest run apps/desktop/src/main.test.tsx
```

Expected: FAIL because `App`, `DesktopApi`, and the new single-page labels do not exist.

- [ ] **Step 5: Add only the minimal exported shell needed by the test**

Create typed `DesktopApi`, shared profile types, and an `App` that loads status/profiles and renders one page with the three first-run labels and the initial connection form landmark. Move `createRoot` into a mount-only `main.tsx`.

- [ ] **Step 6: Verify GREEN and commit**

Run the focused test, then:

```bash
git add package.json package-lock.json vite.config.ts apps/desktop/src
git commit -m "test: add desktop interaction harness"
```

Expected: focused test passes with no warnings.

### Task 2: Implement typed bilingual localization

**Files:**
- Create: `apps/desktop/src/i18n.ts`
- Create: `apps/desktop/src/i18n.test.ts`
- Modify: `apps/desktop/src/App.tsx`

- [ ] **Step 1: Write failing locale tests**

Test these exact behaviors:

```ts
expect(detectLocale("zh-CN")).toBe("zh-CN");
expect(detectLocale("zh-Hans")).toBe("zh-CN");
expect(detectLocale("en-US")).toBe("en");
expect(detectLocale("fr-FR")).toBe("en");
```

Also test that a persisted `lpg.locale=en` overrides a Chinese browser locale, that `setLocalePreference("zh-CN")` persists the setting, and that known backend errors receive localized copy while unknown errors remain escaped plain text. In the rendered application, switch from Chinese to English and assert the header, first-run steps, form labels, primary action, and help copy all update without reloading.

- [ ] **Step 2: Verify RED**

Run:

```bash
PATH=/home/shao/.nvm/versions/node/v24.13.1/bin:$PATH npx vitest run apps/desktop/src/i18n.test.ts
```

Expected: FAIL because the localization API does not exist.

- [ ] **Step 3: Implement the typed catalog**

Export:

```ts
export type Locale = "zh-CN" | "en";
export type TranslationKey = keyof typeof catalogs.en;
export function detectLocale(language: string): Locale;
export function initialLocale(language?: string): Locale;
export function setLocalePreference(locale: Locale): void;
export function translate(locale: Locale, key: TranslationKey, values?: Record<string, string>): string;
export function localizeError(locale: Locale, error: unknown): { summary: string; detail?: string };
```

The English and Chinese catalogs must contain every visible label, hint, status, confirmation, policy description, copy result, and empty-state string. Known Rust error messages map to actionable localized summaries. Unknown error text is returned only as a string and never inserted as HTML.

- [ ] **Step 4: Add the language segmented control**

Initialize from persisted preference or `navigator.language`, persist manual changes, and pass `translate` through explicit typed props. The control must have `aria-label`, `aria-pressed`, and `lang` on the application root.

- [ ] **Step 5: Verify GREEN and commit**

Run locale and shell tests, then:

```bash
git add apps/desktop/src/i18n.ts apps/desktop/src/i18n.test.ts apps/desktop/src/App.tsx
git commit -m "feat: localize desktop control plane"
```

### Task 3: Build the guided connection workflow

**Files:**
- Create: `apps/desktop/src/components/ConnectionForm.tsx`
- Create: `apps/desktop/src/components/OneTimeSecret.tsx`
- Create: `apps/desktop/src/model.test.ts`
- Modify: `apps/desktop/src/model.ts`
- Modify: `apps/desktop/src/App.tsx`
- Modify: `apps/desktop/src/main.test.tsx`

- [ ] **Step 1: Write failing model and interaction tests**

Test `localBaseUrl("openai") === "http://127.0.0.1:17891/v1"` and `localBaseUrl("anthropic") === "http://127.0.0.1:17891"`. Test that protocol switching changes an untouched default upstream URL, does not overwrite a manually edited URL, and a reset action restores the protocol default. Test that advanced policy controls are initially hidden and Strict is submitted by default.

Add a creation test whose fake API records this exact camel-case draft shape:

```ts
{
  name: "DeepSeek",
  protocol: "openai",
  upstreamUrl: "https://api.deepseek.com",
  upstreamKey: "test-upstream-key",
  policy: "strict",
}
```

After success, assert that the full local base URL/key result is visible once and focus moves to it.

- [ ] **Step 2: Verify RED**

Run focused model and application tests. Expected: FAIL because form behavior and helpers are absent.

- [ ] **Step 3: Implement the form and one-time result**

Use a two-option semantic radio segmented control for protocols. Track `urlIsDirty` separately from `upstreamUrl`; reset it only from the explicit reset button. Put policy radio cards inside `<details>`. Keep the upstream key in an uncontrolled password input, clear it after success or failure, disable submit while pending, and never copy it into React state.

`OneTimeSecret` receives `{ profile, local_base_url, local_key }`, provides separate copy actions for URL, key, and full environment configuration, and removes the secret from rendered state when closed. OpenAI full configuration uses `OPENAI_BASE_URL`/`OPENAI_API_KEY`; Anthropic uses `ANTHROPIC_BASE_URL`/`ANTHROPIC_API_KEY`.

- [ ] **Step 4: Implement app orchestration**

On creation, append the returned profile, show the one-time result, and keep the page in the same workflow. Disable Start protection with zero profiles. Add pending state to start/stop and render localized inline errors from `localizeError`.

- [ ] **Step 5: Verify GREEN and commit**

Run frontend tests and TypeScript checking, then commit:

```bash
git add apps/desktop/src
git commit -m "feat: add guided connection workflow"
```

### Task 4: Expose profile management without adding navigation

**Files:**
- Create: `apps/desktop/src/components/ConnectionCard.tsx`
- Create: `apps/desktop/src/components/OverflowMenu.tsx`
- Create: `apps/desktop/src/components/ConfirmDialog.tsx`
- Modify: `apps/desktop/src/App.tsx`
- Modify: `apps/desktop/src/main.test.tsx`

- [ ] **Step 1: Write failing card/menu tests**

With one loaded profile, assert that the card shows name, protocol, upstream URL, policy, and derived local base URL but no upstream/local key. Verify its copy action writes only the local base URL.

Test that `···` opens a menu containing only Rotate local key and Delete connection, Escape closes it, outside click closes it, and keyboard focus returns to the trigger. Assert that the rendered page contains no native `<select>` and no legacy navigation landmark.

Test confirmations using a fake API: rotation calls `rotateLocalKey(id)` and displays the returned key once; deletion calls `deleteProfile(id)` and removes the card only after success. When the gateway is running, the confirmation explains that protection must be stopped and does not submit the destructive action.

- [ ] **Step 2: Verify RED**

Run the application test. Expected: FAIL because persistent cards and actions are absent.

- [ ] **Step 3: Implement cards, popover, and dialogs**

`ConnectionCard` receives only a `ProfileSummary`, gateway-running flag, translated labels, and rotate/delete callbacks. `OverflowMenu` uses a trigger button, `role="menu"`, `role="menuitem"`, document pointer listener, Escape listener, and explicit focus restoration. `ConfirmDialog` uses native `<dialog>` semantics when available in the WebView and a test-compatible controlled open state.

The app converts the rotation response into a `CreatedProfile`-shaped one-time result by combining the existing summary, `localBaseUrl(profile.protocol)`, and returned `local_key`. Failed actions preserve the current card list.

- [ ] **Step 4: Verify GREEN and commit**

Run frontend tests and TypeScript checking, then commit:

```bash
git add apps/desktop/src
git commit -m "feat: add low-friction profile actions"
```

### Task 5: Apply the visual system and responsive behavior

**Files:**
- Replace: `apps/desktop/src/styles.css`
- Modify: `apps/desktop/src/main.test.tsx`

- [ ] **Step 1: Add failing structural accessibility assertions**

Assert that the page has one main heading, the language/protocol controls expose pressed or checked state, status contains text in addition to color, the advanced disclosure is named, and every icon-only button has an accessible name.

- [ ] **Step 2: Verify RED**

Run the application test. Expected: FAIL for missing semantics or accessible names.

- [ ] **Step 3: Implement the dark single-page design**

Define CSS variables for surface, border, foreground, muted, primary, success, warning, and danger colors. Style a compact sticky header, centered `960px` workflow, step indicator, connection grid, radio-card policy choices, segmented controls, one-time result, popover, dialog, notices, disabled/pending states, visible `:focus-visible`, and copy feedback. Use no native select elements.

At `max-width: 760px`, stack header actions, form rows, and cards into one column. At `prefers-reduced-motion: reduce`, remove transitions and animation. Keep the layout usable at 1080×720 without horizontal scrolling.

- [ ] **Step 4: Verify GREEN and commit**

Run frontend tests, TypeScript checking, and production build, then commit:

```bash
git add apps/desktop/src/styles.css apps/desktop/src/main.test.tsx
git commit -m "style: simplify desktop task flow"
```

### Task 6: Suppress the Windows release console and complete verification

**Files:**
- Create: `apps/desktop/src/windows-entry.test.ts`
- Modify: `apps/desktop/src-tauri/src/main.rs`
- Modify: `docs/release-checklist.md`

- [ ] **Step 1: Write the failing regression test**

The Vitest regression test reads `apps/desktop/src-tauri/src/main.rs` and asserts it contains exactly:

```rust
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
```

Run:

```bash
PATH=/home/shao/.nvm/versions/node/v24.13.1/bin:$PATH npx vitest run apps/desktop/src/windows-entry.test.ts
```

Expected: FAIL because the attribute is missing.

- [ ] **Step 2: Add the release-only subsystem attribute**

Place the attribute above `#![forbid(unsafe_code)]` in `main.rs`. Debug builds retain console diagnostics; release Windows builds use the GUI subsystem.

- [ ] **Step 3: Verify GREEN and document manual validation**

Re-run the focused test. Add release checklist steps to build/install the NSIS package, launch the Start Menu shortcut, and confirm no `conhost.exe`/console window appears while the main app remains functional.

- [ ] **Step 4: Run complete verification**

Run:

```bash
PATH=/home/shao/.nvm/versions/node/v24.13.1/bin:$PATH npm test
PATH=/home/shao/.nvm/versions/node/v24.13.1/bin:$PATH npm run lint
PATH=/home/shao/.nvm/versions/node/v24.13.1/bin:$PATH npm run build
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo deny check
```

Expected: all commands exit zero with no test failures or compiler errors.

- [ ] **Step 5: Build and inspect the Windows release**

Copy the committed checkout to a native Windows path, install frontend dependencies, build `dist`, configure the installed MSVC/Windows SDK environment, and run:

```powershell
npm run desktop:build -- --bundles nsis
```

Expected: the release executable and NSIS installer are produced. Launch the packaged executable and verify the application window appears without a console window.

- [ ] **Step 6: Commit**

```bash
git add apps/desktop/src-tauri/src/main.rs apps/desktop/src/windows-entry.test.ts docs/release-checklist.md
git commit -m "fix: hide Windows release console"
```

## Completion checklist

- [ ] Compare every visible UI element against the approved design specification.
- [ ] Confirm Chinese and English cover all visible frontend text.
- [ ] Confirm no Setup/Dashboard/Connections/Policies/Test Console/Activity navigation remains.
- [ ] Confirm no upstream key or persisted local key is rendered.
- [ ] Confirm all automated verification commands pass from fresh output.
- [ ] Confirm the NSIS-packaged release launches without a console window.
