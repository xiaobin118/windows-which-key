# Which-Key Control Panel Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a secure WebView2 control panel for editing the user theme and managing declarative TOML plugins, with resource-level conflict detection and transactional runtime reload.

**Architecture:** Reuse WebView2 but create a separate control-panel window and page. Route all writes through `ConfigurationService`, which validates in memory, builds a candidate immutable snapshot, checks the target resource revision again under a write lock, atomically replaces only the target resource, then swaps `RuntimeSnapshotStore`. Keep overlay and control-panel DOM/state separate while sharing bridge protocol and CSS tokens.

**Tech Stack:** Rust 2021, `windows` 0.58, WebView2 COM, `serde`/`serde_json`, `toml`, embedded HTML/CSS/JavaScript.

**Spec:** `docs/superpowers/specs/2026-08-21-control-panel-design.md`

## Global Constraints

- The existing application-plugin core from `docs/superpowers/plans/2026-08-21-application-plugin-system.md` is a prerequisite; do not duplicate plugin parsing or resolver logic in the control panel.
- Plugins are TOML data only; the control panel never executes plugin code.
- Use resource-level revisions for `theme`, `global_config`, and each plugin; `generation` is only the overall snapshot counter.
- All filesystem writes go through `ConfigurationService` and are serialized by its write lock.
- Candidate parsing, validation, and snapshot construction happen before target replacement; a failed save leaves the old file and runtime snapshot unchanged.
- User plugin writes are restricted to `%APPDATA%\\which-key-windows\\plugins\\`; builtin files are read-only.
- `resetTheme` is frontend-only; only `saveTheme` persists a theme.
- WebView2 rejects external navigation and new-window requests; Release builds disable DevTools.
- User content is transported as Web Messages and must never be interpolated into JavaScript source.
- Preserve unrelated working-tree changes and use TDD for every behavior change.

## Planned File Structure

- Create: `src/theme.rs` — `ThemeConfig`, defaults, validation, contrast warnings, TOML serialization.
- Create: `src/runtime_snapshot.rs` — `ContentRevision`, `ResourceRevisions`, `RuntimeSnapshot`, and snapshot store.
- Create: `src/configuration_service.rs` — locked transactional reads/writes, candidate snapshot construction, resource-level conflict checks.
- Create: `src/control_panel_protocol.rs` — request/response enums, JSON validation, error codes, warnings.
- Create: `src/control_panel_controller.rs` — control-panel WebView2 window lifecycle and message forwarding only.
- Modify: `src/webview_bridge.rs` — shared WebView2 message helpers, safe navigation policy, DevTools policy, overlay theme notifications.
- Modify: `src/window_manager.rs` — reusable window creation/options for overlay and control panel.
- Modify: `src/main.rs` — own `ConfigurationService`, `RuntimeSnapshotStore`, tray command, and controller lifecycle.
- Modify: `src/tray_icon.rs` — add “Open Control Panel”.
- Modify: `src/lib.rs` — export new modules.
- Create: `ui/shared/tokens.css` — theme token variables shared by both pages.
- Create: `ui/shared/components.css` — shared shortcut/card/button styles.
- Create: `ui/shared/bridge.js` — request IDs, pending request map, response dispatch.
- Create: `ui/overlay.html` — migrated overlay DOM and rendering logic.
- Create: `ui/control-panel.html` — Appearance, Plugins, About, and Global Keys pages.
- Modify: `Cargo.toml` — add only dependencies/features required by the new modules.

## Phase 0: Prerequisite Check

### Task 0: Verify application-plugin core boundary

**Files:**
- Read: `docs/superpowers/plans/2026-08-21-application-plugin-system.md`
- Read: `src/config.rs`, `src/registry.rs`, `src/state_machine.rs`, `src/webview_bridge.rs`
- Test: existing library tests

- [ ] **Step 1: Inspect current working-tree changes**

Run:

```powershell
git status --short
git diff -- src/config.rs src/registry.rs src/state_machine.rs src/webview_bridge.rs
```

Record unrelated changes and do not overwrite them.

- [ ] **Step 2: Verify prerequisite interfaces**

Confirm that the plugin implementation exposes a validated snapshot, normalized plugin IDs, plugin listing metadata, enable/disable operations, and a resolver result that can be embedded in `RuntimeSnapshot`.

- [ ] **Step 3: Run the baseline tests**

Run:

```powershell
cargo test --lib
```

Expected: the baseline result is recorded before control-panel changes. If the plugin prerequisite is incomplete, stop implementation and finish that plan first.

## Phase 1: Theme and Snapshot Core

### Task 1: Add theme model and resource revision types

**Files:**
- Create: `src/theme.rs`
- Create: `src/runtime_snapshot.rs`
- Modify: `src/lib.rs`
- Test: inline unit tests in `src/theme.rs` and `src/runtime_snapshot.rs`

**Interfaces:**
- `pub struct ThemeConfig`
- `pub fn ThemeConfig::default_theme() -> ThemeConfig`
- `pub fn ThemeConfig::validate(&self) -> Result<Vec<ThemeWarning>>`
- `pub struct ContentRevision(String)`
- `pub struct ResourceRevisions`
- `pub struct RuntimeSnapshot`
- `pub struct RuntimeSnapshotStore`

- [ ] **Step 1: Write failing theme validation tests**

Cover valid `#RRGGBB`, reject lowercase/short/alpha colors if the schema requires exact six-digit values, reject opacity outside `0.0..=1.0`, radius outside `0..=32`, blur outside `0..=64`, and row spacing outside `0..=24`. Add a low-contrast case that returns a warning without an error.

- [ ] **Step 2: Run focused tests and verify failure**

```powershell
cargo test theme::tests -- --nocapture
```

Expected: compilation fails because the theme module and types do not exist.

- [ ] **Step 3: Implement the minimal theme model**

Use serde-compatible fields matching the approved TOML schema. Keep default values in one constructor. Make validation return structured field errors and non-blocking warnings.

- [ ] **Step 4: Write and run revision tests**

Test deterministic content hashing, independent theme/global/plugin revisions, and monotonic snapshot `generation`. Run `cargo test theme::tests runtime_snapshot::tests -- --nocapture` and require all focused tests to pass.

- [ ] **Step 5: Commit**

```powershell
git add src/theme.rs src/runtime_snapshot.rs src/lib.rs
git commit -m "feat: add theme and runtime snapshot models"
```

### Task 2: Implement transactional ConfigurationService

**Files:**
- Create: `src/configuration_service.rs`
- Modify: `src/runtime_snapshot.rs`, `src/config.rs`, `src/plugin.rs`, `src/lib.rs`
- Test: inline unit tests in `src/configuration_service.rs`

**Interfaces:**
- `pub struct ConfigurationService`
- `pub fn ConfigurationService::load(...) -> Result<Self>`
- `pub fn ConfigurationService::current(&self) -> Arc<RuntimeSnapshot>`
- `pub fn ConfigurationService::save_theme(&self, expected: ContentRevision, theme: ThemeConfig) -> Result<SaveReport>`
- `pub fn ConfigurationService::create_plugin(&self, request: NewPlugin) -> Result<SaveReport>`
- `pub fn ConfigurationService::set_plugin_enabled(&self, id: &PluginId, enabled: bool, expected: ContentRevision) -> Result<SaveReport>`

- [ ] **Step 1: Write failing transaction tests**

Test that invalid theme input leaves the target file unchanged, candidate plugin parse failure leaves both file and snapshot unchanged, a resource revision mismatch returns `revision_conflict`, a second revision check catches a simulated external edit before replacement, and a successful save increments `generation` once.

- [ ] **Step 2: Run focused tests and verify failure**

```powershell
cargo test configuration_service::tests -- --nocapture
```

Expected: compilation fails because the service and transaction APIs do not exist.

- [ ] **Step 3: Implement the write-locked transaction**

Use one internal write lock for all mutations. Read the target resource revision under the lock, deserialize and validate the request, serialize to a sibling temporary file, build a candidate `RuntimeSnapshot` from memory/temporary data, flush the temporary file, re-check the target revision, atomically replace the target, increment `generation`, and swap the store. Return warnings separately from errors.

- [ ] **Step 4: Run focused and regression tests**

```powershell
cargo test configuration_service::tests -- --nocapture
cargo test --lib
```

Expected: all tests pass and failed saves preserve the previous snapshot.

- [ ] **Step 5: Commit**

```powershell
git add src/configuration_service.rs src/runtime_snapshot.rs src/config.rs src/plugin.rs src/lib.rs
git commit -m "feat: add transactional configuration service"
```

## Phase 2: Protocol and WebView2 Window

### Task 3: Add typed control-panel Web Message protocol

**Files:**
- Create: `src/control_panel_protocol.rs`
- Modify: `src/lib.rs`
- Test: inline unit tests in `src/control_panel_protocol.rs`

**Interfaces:**
- `pub enum ControlPanelRequest`
- `pub enum ControlPanelResponse`
- `pub enum ControlPanelErrorCode`
- `pub struct ControlPanelWarning`
- `pub fn parse_request(value: &str) -> Result<ControlPanelRequest, ControlPanelError>`
- `pub fn response_for(request_id: &str, result: Result<...>) -> serde_json::Value`

- [ ] **Step 1: Write failing protocol tests**

Cover valid `saveTheme` with `resourceRevision`, `createPlugin` with `resourceRevision: null`, unknown message, invalid JSON, field validation errors, `warnings` on successful responses, and `replyTo` preservation.

- [ ] **Step 2: Run focused tests and verify failure**

```powershell
cargo test control_panel_protocol::tests -- --nocapture
```

Expected: compilation fails because protocol types do not exist.

- [ ] **Step 3: Implement typed parsing and response serialization**

Reject arbitrary paths in request payloads. `openPluginFile` accepts only a plugin ID. Keep `error` and `warnings` independent: failed responses have `ok=false` and an error; successful responses may contain warnings.

- [ ] **Step 4: Run focused tests**

```powershell
cargo test control_panel_protocol::tests -- --nocapture
```

Expected: all protocol tests pass.

- [ ] **Step 5: Commit**

```powershell
git add src/control_panel_protocol.rs src/lib.rs
git commit -m "feat: add control panel message protocol"
```

### Task 4: Create the control-panel WebView2 controller

**Files:**
- Create: `src/control_panel_controller.rs`
- Modify: `src/webview_bridge.rs`, `src/window_manager.rs`, `src/main.rs`, `src/lib.rs`
- Test: pure controller message-routing tests; Windows smoke test

**Interfaces:**
- `pub struct ControlPanelController`
- `pub fn ControlPanelController::open(...) -> Result<()>`
- `pub fn ControlPanelController::close(&mut self) -> Result<()>`
- `pub fn ControlPanelController::handle_message(&self, message: &str) -> ControlPanelResponse`

- [ ] **Step 1: Write failing routing tests**

Test that a valid request reaches `ConfigurationService`, a malformed request returns `invalid_json`, and a service error is serialized without closing the window.

- [ ] **Step 2: Implement window lifecycle and forwarding**

Reuse the existing WebView2 setup, but keep this controller responsible only for window lifetime and message forwarding. The service owns all config behavior. Load `ui/control-panel.html` as embedded content.

- [ ] **Step 3: Harden WebView2 settings**

Reject external navigation, reject new-window requests, disable context menus/status bar, and disable DevTools in Release. Use Web Message APIs for inbound/outbound JSON; do not build JavaScript strings from user payloads.

- [ ] **Step 4: Add the tray command**

Add “打开控制面板” to the tray menu. Opening an existing panel focuses it; it does not create duplicate windows.

- [ ] **Step 5: Run verification**

```powershell
cargo test --lib
cargo fmt --check
```

Then manually verify the panel opens and closes on Windows.

- [ ] **Step 6: Commit**

```powershell
git add src/control_panel_controller.rs src/webview_bridge.rs src/window_manager.rs src/main.rs src/lib.rs src/tray_icon.rs
git commit -m "feat: add control panel window"
```

## Phase 3: Shared UI and Pages

### Task 5: Split UI resources and add shared bridge

**Files:**
- Create: `ui/shared/tokens.css`
- Create: `ui/shared/components.css`
- Create: `ui/shared/bridge.js`
- Create: `ui/overlay.html`
- Modify: `src/webview_bridge.rs`
- Test: static resource contract check

- [ ] **Step 1: Write the resource contract check**

Assert that both pages contain the shared token/component imports, the bridge exposes request IDs and pending responses, and neither page uses `innerHTML` for plugin-controlled text.

- [ ] **Step 2: Run the check and verify failure**

Run the repository’s frontend/static check command after adding the test harness. Expected: fail because the `ui/` resources do not exist.

- [ ] **Step 3: Extract shared styles and bridge logic**

Move the current overlay visual language into shared tokens/components. Keep overlay DOM and control-panel DOM separate. The bridge uses `window.chrome.webview.postMessage()` for requests, stores pending resolvers by `requestId`, and dispatches responses by `replyTo`.

- [ ] **Step 4: Migrate overlay behavior**

Preserve `show`, `update`, and `hide` behavior. Render all user/plugin descriptions with `textContent`. Apply theme tokens from runtime notifications without changing the existing hold-modifier interaction.

- [ ] **Step 5: Run static checks**

Verify resource imports, message names, and absence of user-controlled `innerHTML`.

- [ ] **Step 6: Commit**

```powershell
git add ui src/webview_bridge.rs
git commit -m "refactor: split shared and page-specific UI resources"
```

### Task 6: Implement Appearance page

**Files:**
- Modify: `ui/control-panel.html`
- Modify: `ui/shared/tokens.css`, `ui/shared/components.css`
- Modify: `src/control_panel_protocol.rs`, `src/control_panel_controller.rs`
- Test: frontend state tests and Rust protocol/service tests

- [ ] **Step 1: Write failing state tests**

Cover `Loading → Ready`, field edit `Ready → Dirty`, frontend-only Reset staying Dirty, Save entering Saving, successful save entering Saved with a new resource revision, validation failure entering Error, and revision conflict entering Conflict.

- [ ] **Step 2: Run focused tests and verify failure**

Run the frontend state test command and `cargo test control_panel_protocol::tests configuration_service::tests`. Expected: the new state cases fail before implementation.

- [ ] **Step 3: Build the Appearance page**

Add the approved fields, field-level errors, contrast warnings, Reset, Save, and an in-page shortcut overlay preview. Dirty preview changes must remain local until save succeeds.

- [ ] **Step 4: Connect saveTheme**

Send only structured JSON with `resourceRevision`; render separate warnings and errors; on conflict show Reload/Overwrite/Cancel. Keep form values after errors.

- [ ] **Step 5: Run focused tests and manual checks**

Verify all state transitions and that the real overlay changes only after a successful save.

- [ ] **Step 6: Commit**

```powershell
git add ui/control-panel.html ui/shared src/control_panel_protocol.rs src/control_panel_controller.rs
git commit -m "feat: add control panel appearance editor"
```

### Task 7: Implement Plugins and About pages

**Files:**
- Modify: `ui/control-panel.html`
- Modify: `src/control_panel_protocol.rs`, `src/configuration_service.rs`, `src/control_panel_controller.rs`
- Test: Rust path-security/plugin-service tests and frontend interaction tests

- [ ] **Step 1: Write failing plugin-management tests**

Cover listing builtin/user source, enabling/disabling a user plugin, creating a plugin from structured input, rejecting invalid IDs and reserved device names, rejecting paths outside the user plugin directory, opening files by discovered ID only, and isolating invalid user plugins.

- [ ] **Step 2: Run focused tests and verify failure**

```powershell
cargo test configuration_service::tests plugin::tests -- --nocapture
```

Expected: new control-panel operations fail before implementation.

- [ ] **Step 3: Implement Rust plugin operations**

Resolve file paths from the discovered plugin index. Generate filenames from validated IDs. Never accept a path string from the frontend. Create a minimal plugin payload, serialize it to a temporary file, validate it through `PluginLoader`, then commit it through `ConfigurationService`.

- [ ] **Step 4: Build the Plugins page**

Show source, process names, enabled state, entry count, and errors. Add details, enable/disable, create plugin, open file, and open directory actions. Keep builtin entries read-only.

- [ ] **Step 5: Build the About and Global Keys pages**

Show version, paths, WebView2 status, and buttons to open directories or the raw global config. Do not add a visual global-key editor in this phase.

- [ ] **Step 6: Run focused tests and manual checks**

Verify plugin create/disable/reload, path rejection, invalid-plugin isolation, Dirty close confirmation, and About actions.

- [ ] **Step 7: Commit**

```powershell
git add ui/control-panel.html src/control_panel_protocol.rs src/configuration_service.rs src/control_panel_controller.rs
git commit -m "feat: add plugin management control panel"
```

## Final Integration and Verification

### Task 8: Wire startup, reload, and overlay notifications

**Files:**
- Modify: `src/main.rs`, `src/overlay_controller.rs`, `src/webview_bridge.rs`, `src/tray_icon.rs`
- Test: full library suite and Windows smoke matrix

- [ ] **Step 1: Write failing integration tests**

Cover startup loading the same snapshot service as the control panel, tray reload using the same transaction path, overlay theme notification after save, and control-panel focus instead of duplicate window creation.

- [ ] **Step 2: Implement shared service ownership**

Construct one `ConfigurationService`/`RuntimeSnapshotStore` in startup and pass handles to state machine, overlay, tray, and control panel. A successful save or reload replaces the immutable snapshot once and notifies consumers.

- [ ] **Step 3: Run automated verification**

```powershell
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
```

Expected: all commands exit with status 0.

- [ ] **Step 4: Run the Windows smoke matrix**

Verify:

1. Control panel opens from tray and does not duplicate.
2. Appearance edits preview locally, then update the overlay only after Save.
3. Invalid theme values return field errors.
4. External theme edit produces Conflict and does not overwrite the file.
5. User plugin creation and disable/reload affect the active keymap.
6. Invalid plugin input is isolated and reported as an error/warning.
7. Illegal paths and builtin modifications are rejected.
8. Dirty close confirmation works for Appearance and Plugins.
9. External navigation/new windows are rejected.

- [ ] **Step 5: Run final review and inspect working tree**

Run `git status --short`, confirm only intended files changed, compare behavior against every acceptance criterion in `docs/superpowers/specs/2026-08-21-control-panel-design.md`, then use `superpowers:requesting-code-review` and `superpowers:verification-before-completion` before claiming completion.
