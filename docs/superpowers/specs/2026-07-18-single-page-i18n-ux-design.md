# Single-Page Internationalized Desktop UX Design

**Date:** 2026-07-18
**Status:** Approved design
**Scope:** Windows desktop control plane only

## 1. Objective

Reduce first-run learning cost by replacing the six-page placeholder-oriented interface with one guided task flow. The new control plane must support complete Simplified Chinese and English switching, improve menus and form controls, remove duplicate or misleading functions, and prevent the Windows release build from opening a console window.

The default path must let a new user complete three tasks without navigating between pages:

1. Add an upstream model connection.
2. Copy the generated local client configuration.
3. Start the local privacy gateway.

## 2. Current-state findings

- `Setup` and `Connections` render the same connection form.
- `Dashboard`, `Policies`, `Test Console`, and `Activity` currently expose descriptive placeholder text rather than independent working features.
- The backend transformation preview returns the original text, activity query returns an empty list, and connection test validates only the URL format. These must not be presented as completed product capabilities.
- Profile deletion and local-key rotation are implemented in the backend but are not exposed in the UI.
- The Windows entry point lacks the release-only Windows GUI subsystem attribute, so launching the packaged executable can open an additional console window.
- Native `select` controls and the current navigation have minimal styling and weak visual hierarchy.

## 3. Product principles

- **One primary workflow:** connection setup, local configuration, and gateway control live on one page.
- **Progressive disclosure:** advanced privacy policy and destructive operations remain hidden until requested.
- **No placeholder navigation:** a feature without working behavior is not shown as a page or action.
- **Security transparency:** upstream keys, mapping keys, prompts, and responses are never rendered after submission.
- **One-time secrets:** newly created or rotated local keys are shown only in the immediate success state.
- **Plain language:** labels describe user outcomes instead of internal architecture where possible.

## 4. Information architecture

### 4.1 Header

The fixed page header contains:

- product name and a one-sentence purpose;
- gateway status pill;
- Simplified Chinese / English segmented language control;
- primary `Start protection` / `Stop protection` action.

The primary action is disabled when no connection exists. The adjacent explanation tells the user to add a connection first.

### 4.2 First-run state

When no profile exists, the main content shows a three-step path:

1. Add model connection.
2. Copy local client settings.
3. Start protection.

Only the first step is initially active. The connection form is visible below the step summary. Successful creation moves focus to the one-time local configuration result.

### 4.3 Configured state

When profiles exist, the page shows:

- a compact gateway summary;
- connection cards;
- an `Add connection` action;
- the one-time configuration result only when a profile was just created or its key was rotated.

The setup explainer is shortened after the first profile exists.

### 4.4 Connection form

Required fields:

- connection name;
- protocol;
- upstream base URL;
- upstream API key.

OpenAI-compatible and Anthropic Messages are represented by a two-option segmented control rather than a native dropdown. Changing protocol updates the default upstream URL but never overwrites a URL after the user has manually edited it unless they explicitly choose to reset it.

Privacy policy is placed under an `Advanced settings` disclosure and defaults to Strict. Strict, Balanced, and Monitor are represented by radio cards with one-sentence explanations. No standalone policy page is retained.

### 4.5 Connection card

Each card shows only:

- display name;
- protocol badge;
- upstream host or base URL;
- policy badge;
- local base URL;
- a copy-local-base-URL action.

The local API key is not retrievable after its one-time display. Full client settings, including the local key, can be copied only from the immediate one-time result. The persistent card can copy the local base URL but does not imply that the existing key can be revealed.

### 4.6 Low-frequency action menu

The `···` menu contains:

- rotate local key;
- delete connection.

The menu is a custom accessible popover with clear hover and focus states, rounded corners, restrained shadow, keyboard focus management, Escape-to-close, outside-click dismissal, and a destructive visual treatment for deletion.

Key rotation requires confirmation and displays the replacement key once. Deletion requires confirmation and explains that the gateway must be stopped first when applicable.

## 5. Internationalization

### 5.1 Supported locales

- `zh-CN` — Simplified Chinese;
- `en` — English.

All user-visible frontend text must come from a typed translation catalog. Protocol names, API product names, URLs, and code samples remain untranslated where translation would reduce technical accuracy.

### 5.2 Locale selection

On first run:

- use Chinese when the system or WebView locale begins with `zh`;
- otherwise use English.

After a manual change, persist the locale locally and prefer it on subsequent launches. Locale preference contains no sensitive data and may use local WebView storage.

### 5.3 Backend errors

Known backend error strings map to localized, actionable messages. Unknown errors remain available as secondary technical detail so diagnostics are not lost. Raw errors must not be treated as trusted HTML.

The system tray remains English in this iteration because Tauri menu labels are created before the React locale state exists. Tray localization is a separate future enhancement and is not required for completion of this scope.

## 6. Functional consolidation

Remove from the visible interface:

- duplicate Setup and Connections navigation;
- standalone Dashboard placeholder;
- standalone Policies placeholder;
- Test Console placeholder;
- Activity placeholder;
- connection-test action that only validates URL syntax;
- permanent generic security notices repeated on every view.

Retain and expose:

- create connection;
- list connections;
- start and stop gateway;
- copy the local base URL at any time and copy the full generated client configuration only while its key is in the one-time result;
- rotate local key;
- delete connection;
- select privacy policy during connection creation.

Backend commands that are not exposed may remain temporarily for compatibility, but the UI must not advertise them as functioning product features. Removing those commands is outside this iteration unless repository analysis proves they are unreferenced and safe to delete without widening the task.

## 7. Component boundaries

- `i18n.ts`: typed catalogs, locale detection, persistence, and translation lookup.
- `App`: status loading, profile loading, global action orchestration, and one-page layout.
- `ConnectionForm`: draft state, protocol choice, URL defaults, API key submission, and advanced policy disclosure.
- `ConnectionCard`: profile summary and copyable local base URL. The base URL is derived from the profile protocol and the configured local gateway address by a pure helper; it never requires access to the stored local key.
- `OverflowMenu`: low-frequency profile actions and keyboard behavior.
- `OneTimeSecret`: newly created or rotated key display and copy action.
- shared presentation helpers: status pill, segmented control, inline notice, and confirmation dialog.

Components communicate through explicit typed props. Tauri invocations remain in an application service boundary or narrow action handlers so presentation components can be tested without a running desktop runtime.

## 8. Windows console behavior

The binary entry point uses:

```rust
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
```

Release builds therefore launch as a Windows GUI application without an additional console window. Debug builds retain console output for development diagnostics.

## 9. Error and state behavior

- Initial runtime failures appear as a concise inline error with technical detail available on demand.
- Form submission disables only the relevant action and prevents duplicate profile creation requests.
- Empty required fields use native validation plus localized labels.
- Gateway start/stop displays an in-progress state and prevents repeated clicks.
- A failed destructive operation leaves the profile card intact.
- Closing a one-time secret removes it from rendered state; it is not recoverable through the UI.
- Confirmation dialogs identify the affected profile by display name.

## 10. Accessibility and visual rules

- All interactive controls have visible keyboard focus.
- Segmented controls use semantic radio behavior.
- Popovers use appropriate button/menu semantics and predictable keyboard dismissal.
- Status is conveyed through text as well as color.
- Body text, muted text, borders, and action colors meet practical contrast requirements for the dark theme.
- The page remains usable at 1080×720 and collapses to a single-column layout at narrow widths.
- Motion is restrained and disabled when `prefers-reduced-motion` is active.

## 11. Verification

### Frontend behavior tests

- first-run locale follows a Chinese or non-Chinese environment;
- manual locale switching updates all visible text and persists the choice;
- no legacy six-page navigation or duplicate setup entry remains;
- protocol segmented control updates the untouched default URL;
- advanced policy controls are hidden by default;
- creating a profile renders its local key in the one-time result;
- the full generated client configuration is copyable only from the one-time result, while an existing card exposes only its local base URL;
- gateway start and stop call the correct commands and update status;
- overflow menu opens, closes on Escape and outside click, and exposes only rotate/delete;
- rotation displays a new one-time local key;
- deletion confirmation calls the correct command and removes the card only after success;
- unknown runtime errors are rendered as text rather than HTML.

### Rust and packaging checks

- release entry point has the Windows GUI subsystem attribute;
- Rust formatting, Clippy, and workspace tests pass;
- TypeScript checking, frontend tests, and production build pass;
- Windows `cargo check --locked` passes;
- NSIS release package builds successfully;
- launching the packaged release executable does not create a console window.

## 12. Out of scope

- additional languages;
- localized tray menu;
- a real activity dashboard;
- a real transformation test console;
- per-category policy-rule editing;
- a real upstream connection health check;
- autostart configuration;
- cloud synchronization or telemetry;
- redesign of the gateway protocol or privacy engine.

These features require independently testable backend behavior and must not be introduced as visual placeholders.
