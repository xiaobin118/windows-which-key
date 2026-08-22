# Application Plugin System Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add safe TOML application plugins, foreground executable detection, merged Windows/application suggestions, multi-key sequences, and a show-all shortcut browser.

**Architecture:** Implement the feature in three stages: a pure plugin/keymap core, a Windows foreground-application adapter plus resolver, and interaction/UI integration. Keep Win32 access outside parsing and merge logic; expose immutable `Arc<ShortcutRegistry>` snapshots to the existing state machine.

**Tech Stack:** Rust 2021, `windows` 0.58, `serde`, `toml`, `anyhow`, Win32 `WH_KEYBOARD_LL`, WebView2, embedded HTML/CSS/JavaScript.

**Spec:** `docs/superpowers/specs/2026-08-21-application-plugin-system-design.md`

## Global Constraints

- Preserve all unrelated working-tree changes; inspect `git diff` before editing each existing file.
- Plugins are TOML data only and must never execute code.
- Application matching uses case-insensitive executable filenames only.
- Merge precedence is user plugin > built-in plugin > Windows global keymap.
- User plugins live in `%APPDATA%\which-key-windows\plugins\`.
- Built-in plugin schema version is exactly `1`.
- Priorities are exactly `essential`, `recommended`, and `advanced`.
- Normal application shortcuts pass through; only `Win+Shift+/` and `Esc` while show-all is open are intercepted.
- First version excludes mouse shortcuts, executable plugins, online distribution, application-internal mode detection, Web Office, and automatic shortcut execution.
- Use TDD for every behavior change and commit only the files listed by the current task.

## Planned File Structure

- Create `src/shortcut.rs`: canonical key parsing, formatting, alternatives, and sequence compilation helpers.
- Modify `src/types.rs`: binding metadata, priorities, display categories, and show-all events/commands.
- Modify `src/config.rs`: delegate key parsing to `shortcut`; retain Windows global configuration loading.
- Modify `src/registry.rs`: metadata-aware entries, deterministic ordering, deep merge, and sequence tree insertion.
- Create `src/plugin.rs`: plugin schema, validation, built-in/user loading, override, disable, and process index.
- Create `src/foreground_app.rs`: Win32 foreground executable detection only.
- Create `src/keymap_resolver.rs`: choose and merge the effective registry for a normalized process.
- Modify `src/hook.rs`: pure interception decisions plus `Win+Shift+/` and conditional `Esc` interception.
- Modify `src/state_machine.rs`: registry snapshot replacement, sequence completion, and show-all state.
- Modify `src/main.rs`: own the active snapshot, foreground detection, reload, and hook interception mode.
- Modify `src/webview_bridge.rs`: serialize categories, priorities, application name, and show-all commands.
- Modify `src/frontend.html`: grouped show-all view and priority ordering.
- Create `plugins/builtin/{vscode,word,excel,powerpoint}.toml`: approved initial application data.
- Create `docs/references/initial-built-in-shortcuts.md`: preserved source corpus for the first four built-in plugins.
- Modify `Cargo.toml`: add only Win32 features required by foreground process lookup.
- Modify `src/lib.rs`: export new modules.
- Modify `docs/PROJECT_MEMORY.md`: record completed implementation status only after final verification.

---

## Phase 1: Plugin and Keymap Core

### Task 1: Canonical shortcut syntax and metadata

**Files:**
- Create: `src/shortcut.rs`
- Modify: `src/types.rs`
- Modify: `src/config.rs`
- Modify: `src/registry.rs`
- Modify: `src/lib.rs`
- Test: inline unit tests in `src/shortcut.rs`, `src/config.rs`, and `src/registry.rs`

**Interfaces:**
- Produces: `pub fn parse_shortcut(input: &str) -> anyhow::Result<ShortcutKey>`
- Produces: `pub fn format_shortcut(key: &ShortcutKey) -> String`
- Produces: `pub enum BindingPriority { Essential, Recommended, Advanced }`
- Produces: `pub struct BindingMetadata { pub category: String, pub priority: BindingPriority }`
- Produces: `Node::new_binding(desc: String, metadata: BindingMetadata) -> Node`
- Consumes: existing `Key`, `ModifierSet`, `ShortcutKey`, and `Node`

- [ ] **Step 1: Write failing parser tests**

Add these cases to `src/shortcut.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_canonicalizes_named_and_symbol_keys() {
        for (input, expected) in [
            ("Ctrl-Shift-P", "C-S-p"),
            ("F12", "F12"),
            ("PageDown", "PageDown"),
            ("/", "/"),
            ("`", "`"),
            ("Ctrl-+", "C-+"),
        ] {
            let key = parse_shortcut(input).unwrap();
            assert_eq!(format_shortcut(&key), expected);
        }
    }

    #[test]
    fn rejects_modifier_without_a_key() {
        assert!(parse_shortcut("Ctrl-").is_err());
    }
}
```

- [ ] **Step 2: Run the focused test and verify failure**

Run:

```powershell
cargo test shortcut::tests -- --nocapture
```

Expected: compilation fails because `src/shortcut.rs`, `parse_shortcut`, and `format_shortcut` do not exist.

- [ ] **Step 3: Implement canonical parsing and formatting**

Move the parsing responsibility out of `config.rs` and the formatting responsibility out of `registry.rs`. Support letters, digits, `/`, `` ` ``, `+`, `-`, `;`, arrows, `Backspace`, `Delete`, `Enter`, `Esc`, `Space`, `Tab`, `Home`, `End`, `PageUp`, `PageDown`, and `F1` through `F24`. Use virtual-key codes and one canonical short modifier order: `C-A-S-M`.

The public entry points must be:

```rust
pub fn parse_shortcut(input: &str) -> Result<ShortcutKey>;
pub fn format_shortcut(key: &ShortcutKey) -> String;
```

For OEM punctuation, define explicit VK constants in `shortcut.rs` rather than deriving ASCII codes, because Win32 keyboard events report virtual keys such as `VK_OEM_2` for `/`.

- [ ] **Step 4: Add metadata without breaking global config**

In `src/types.rs`, add:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BindingPriority {
    Essential,
    Recommended,
    Advanced,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BindingMetadata {
    pub category: String,
    pub priority: BindingPriority,
}
```

Extend `Node` with `metadata: Option<BindingMetadata>`. Existing Windows bindings created from `which-key.toml` receive category `"Windows"` and priority `Recommended` so the old file remains compatible.

- [ ] **Step 5: Delegate existing call sites and run tests**

Replace private parsing/formatting functions in `config.rs` and `registry.rs` with the new module. Run:

```powershell
cargo test shortcut::tests config::tests registry::tests types::tests -- --nocapture
```

Expected: all focused tests pass, including existing tests.

- [ ] **Step 6: Commit Task 1**

```powershell
git add src/shortcut.rs src/types.rs src/config.rs src/registry.rs src/lib.rs
git commit -m "refactor: centralize shortcut syntax"
```

### Task 2: Plugin schema validation

**Files:**
- Create: `src/plugin.rs`
- Modify: `src/lib.rs`
- Test: inline unit tests in `src/plugin.rs`

**Interfaces:**
- Consumes: `shortcut::parse_shortcut`, `BindingMetadata`, and `BindingPriority`
- Produces: `pub struct PluginDefinition`
- Produces: `pub struct PluginBinding`
- Produces: `pub fn parse_plugin_toml(source: &str, origin: PluginOrigin) -> anyhow::Result<PluginDefinition>`
- Produces: `pub enum PluginOrigin { BuiltIn, User(PathBuf) }`

- [ ] **Step 1: Write failing schema tests**

```rust
#[test]
fn parses_alternatives_and_sequences() {
    let source = r#"
schema_version = 1
id = "vscode"
name = "Visual Studio Code"
processes = ["Code.exe"]

[[bindings]]
keys = ["C-S-p", "F1"]
description = "Command Palette"
category = "Common"
priority = "essential"

[[bindings]]
keys = ["C-k", "C-f"]
description = "Format Selection"
category = "Editing"
priority = "recommended"
sequence = true
"#;

    let plugin = parse_plugin_toml(source, PluginOrigin::BuiltIn).unwrap();
    assert_eq!(plugin.id, "vscode");
    assert_eq!(plugin.processes, vec!["code.exe"]);
    assert_eq!(plugin.bindings.len(), 2);
    assert!(!plugin.bindings[0].sequence);
    assert!(plugin.bindings[1].sequence);
}

#[test]
fn rejects_unsupported_schema_and_empty_fields() {
    let bad_version = "schema_version=2\nid='x'\nname='X'\nprocesses=['x.exe']";
    assert!(parse_plugin_toml(bad_version, PluginOrigin::BuiltIn).is_err());

    let empty_keys = r#"
schema_version=1
id="x"
name="X"
processes=["x.exe"]
[[bindings]]
keys=[]
description="Missing"
category="Common"
priority="essential"
"#;
    assert!(parse_plugin_toml(empty_keys, PluginOrigin::BuiltIn).is_err());
}
```

- [ ] **Step 2: Run the focused test and verify failure**

```powershell
cargo test plugin::tests -- --nocapture
```

Expected: compilation fails because the plugin types and parser do not exist.

- [ ] **Step 3: Implement schema types and validation**

Deserialize private raw structs, then validate into public types. The final binding representation must distinguish alternatives from sequences:

```rust
#[derive(Debug, Clone)]
pub enum BindingKeys {
    Alternatives(Vec<ShortcutKey>),
    Sequence(Vec<ShortcutKey>),
}

#[derive(Debug, Clone)]
pub struct PluginBinding {
    pub keys: BindingKeys,
    pub description: String,
    pub metadata: BindingMetadata,
}
```

Reject schema versions other than `1`, blank `id`/`name`/`description`/`category`, empty `processes`/`keys`, invalid priorities, malformed shortcuts, and duplicate normalized bindings within one plugin. Normalize `id` to lowercase ASCII and process names to lowercase Unicode.

- [ ] **Step 4: Run plugin and shortcut tests**

```powershell
cargo test plugin::tests shortcut::tests -- --nocapture
```

Expected: all focused tests pass.

- [ ] **Step 5: Commit Task 2**

```powershell
git add src/plugin.rs src/lib.rs
git commit -m "feat: define application plugin schema"
```

### Task 3: Plugin loading, override, disable, and immutable snapshots

**Files:**
- Modify: `src/plugin.rs`
- Test: inline unit tests in `src/plugin.rs`

**Interfaces:**
- Consumes: `parse_plugin_toml`
- Produces: `pub struct PluginSnapshot`
- Produces: `PluginSnapshot::load(built_ins: &[(&str, &str)], user_dir: &Path) -> anyhow::Result<PluginLoadReport>`
- Produces: `pub struct PluginLoadReport { pub snapshot: Arc<PluginSnapshot>, pub warnings: Vec<PluginWarning> }`
- Produces: `PluginSnapshot::for_process(&self, normalized_exe: &str) -> Option<&PluginDefinition>`

- [ ] **Step 1: Write failing loader tests with temporary directories**

Add tests proving all four rules:

```rust
#[test]
fn user_plugin_overrides_builtin_by_id_and_binding() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("vscode.toml"), USER_VSCODE).unwrap();
    let report = PluginSnapshot::load(&[("vscode.toml", BUILTIN_VSCODE)], dir.path()).unwrap();
    let plugin = report.snapshot.for_process("CODE.EXE").unwrap();
    assert_eq!(plugin.binding_description("C-p"), Some("My Quick Open"));
    assert_eq!(plugin.binding_description("C-S-p"), Some("Command Palette"));
}

#[test]
fn disabled_user_plugin_removes_builtin() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("vscode.toml"), DISABLED_VSCODE).unwrap();
    let report = PluginSnapshot::load(&[("vscode.toml", BUILTIN_VSCODE)], dir.path()).unwrap();
    assert!(report.snapshot.for_process("code.exe").is_none());
}

#[test]
fn invalid_user_plugin_is_warning_not_fatal() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("broken.toml"), "not toml").unwrap();
    let report = PluginSnapshot::load(&[("vscode.toml", BUILTIN_VSCODE)], dir.path()).unwrap();
    assert_eq!(report.warnings.len(), 1);
    assert!(report.snapshot.for_process("code.exe").is_some());
}

#[test]
fn conflicting_ids_for_same_process_are_fatal_within_one_origin() {
    let builtins = [("one.toml", PLUGIN_ONE), ("two.toml", PLUGIN_TWO_SAME_PROCESS)];
    assert!(PluginSnapshot::load(&builtins, Path::new("missing-user-dir")).is_err());
}
```

- [ ] **Step 2: Run focused tests and verify failure**

```powershell
cargo test plugin::tests -- --nocapture
```

Expected: the four new tests fail because snapshot loading and merge behavior do not exist.

- [ ] **Step 3: Implement deterministic loading and merge**

Load built-ins first and treat any invalid built-in as fatal. Read only direct `*.toml` children of the user directory, sort paths case-insensitively before parsing, and turn invalid user files into `PluginWarning`. Merge same-`id` user definitions by normalized binding identity while retaining unmentioned built-in bindings. A disabled user definition removes the built-in from the process index.

Do not silently choose between different plugin IDs that claim the same normalized process at the same origin; return an error naming both IDs and the process.

- [ ] **Step 4: Run focused and full library tests**

```powershell
cargo test plugin::tests -- --nocapture
cargo test --lib
```

Expected: all tests pass.

- [ ] **Step 5: Commit Task 3**

```powershell
git add src/plugin.rs
git commit -m "feat: load and merge application plugins"
```

### Task 4: Compile plugin bindings into mergeable shortcut registries

**Files:**
- Modify: `src/registry.rs`
- Modify: `src/plugin.rs`
- Modify: `src/types.rs`
- Test: inline tests in `src/registry.rs`

**Interfaces:**
- Consumes: `PluginDefinition`, `PluginBinding`, and `BindingKeys`
- Produces: `ShortcutRegistry::from_plugin(plugin: &PluginDefinition) -> anyhow::Result<Self>`
- Produces: `ShortcutRegistry::merge_from(&mut self, higher_priority: &ShortcutRegistry)`
- Produces: `ShortcutRegistry::all_entries(&self) -> Vec<DisplayEntry>` with full canonical sequences such as `C-k, C-f`
- Produces: deterministic `DisplayEntry` values containing category and priority

- [ ] **Step 1: Write failing registry compilation tests**

```rust
#[test]
fn compiles_alternative_and_sequence_bindings() {
    let plugin = parse_plugin_toml(TEST_PLUGIN, PluginOrigin::BuiltIn).unwrap();
    let registry = ShortcutRegistry::from_plugin(&plugin).unwrap();

    assert!(matches!(registry.resolve(&[], key("C-S-p")), ResolveResult::Leaf(_)));
    assert!(matches!(registry.resolve(&[], key("F1")), ResolveResult::Leaf(_)));
    assert!(matches!(registry.resolve(&[], key("C-k")), ResolveResult::Group(_)));
    assert!(matches!(
        registry.resolve(&[key("C-k")], key("C-f")),
        ResolveResult::Leaf(_)
    ));
}

#[test]
fn higher_priority_registry_replaces_same_path_only() {
    let mut global = registry_with("C-p", "Global", "Windows", BindingPriority::Recommended);
    let app = registry_with("C-p", "Quick Open", "Navigation", BindingPriority::Essential);
    global.merge_from(&app);
    assert_eq!(leaf_desc(&global, "C-p"), "Quick Open");
    assert_eq!(leaf_desc(&global, "C-c"), "Copy");
}

#[test]
fn all_entries_flattens_sequences_with_full_key_labels() {
    let plugin = parse_plugin_toml(TEST_PLUGIN, PluginOrigin::BuiltIn).unwrap();
    let registry = ShortcutRegistry::from_plugin(&plugin).unwrap();
    let entries = registry.all_entries();
    assert!(entries.iter().any(|entry| entry.key == "C-k, C-f"));
}
```

- [ ] **Step 2: Run registry tests and verify failure**

```powershell
cargo test registry::tests -- --nocapture
```

Expected: compilation fails because `from_plugin`, `merge_from`, and metadata fields do not exist.

- [ ] **Step 3: Implement tree insertion and recursive merge**

Insert every alternative as an independent leaf. Insert sequences as a path. Reject a plugin where one path must be both a leaf and a prefix, because that ambiguity cannot be displayed or merged deterministically in the current `Node` model.

Implement deep merge so a higher-priority leaf replaces the same lower-priority leaf, while a higher-priority group recursively merges children. Preserve unrelated global children.

Implement `all_entries()` as a depth-first traversal that emits leaves only, joins every path segment with `", "`, clones leaf metadata, and applies the same deterministic priority/category/key sorting used by the normal view.

- [ ] **Step 4: Add deterministic display sorting and run tests**

Extend `DisplayEntry` with `category` and `priority`. Sort by priority, category, groups-first within a category, then canonical key. Run:

```powershell
cargo test registry::tests plugin::tests -- --nocapture
```

Expected: all tests pass.

- [ ] **Step 5: Commit Task 4**

```powershell
git add src/registry.rs src/plugin.rs src/types.rs
git commit -m "feat: compile plugin keymaps"
```

## Phase 2: Foreground Application and Effective Keymaps

### Task 5: Foreground executable detector

**Files:**
- Create: `src/foreground_app.rs`
- Modify: `src/lib.rs`
- Modify: `Cargo.toml`
- Test: inline pure tests in `src/foreground_app.rs`

**Interfaces:**
- Produces: `pub trait ForegroundAppProvider { fn foreground_executable(&self) -> anyhow::Result<Option<String>>; }`
- Produces: `pub struct Win32ForegroundAppProvider`
- Produces: `pub fn normalize_executable_path(path: &str) -> Option<String>`

- [ ] **Step 1: Write failing path normalization tests**

```rust
#[test]
fn extracts_and_normalizes_windows_executable_name() {
    assert_eq!(
        normalize_executable_path(r"C:\Program Files\Microsoft VS Code\Code.exe"),
        Some("code.exe".to_string())
    );
    assert_eq!(normalize_executable_path(""), None);
}
```

- [ ] **Step 2: Run focused test and verify failure**

```powershell
cargo test foreground_app::tests -- --nocapture
```

Expected: compilation fails because the module and function do not exist.

- [ ] **Step 3: Implement Win32 lookup with owned handles**

Use `GetForegroundWindow`, `GetWindowThreadProcessId`, `OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid)`, and `QueryFullProcessImageNameW`. Wrap the process `HANDLE` in a local RAII type whose `Drop` calls `CloseHandle`. Return `Ok(None)` for a null foreground window and return contextual errors for all other failures.

Add only the exact `windows` feature flags needed by those APIs.

- [ ] **Step 4: Run focused tests and a compile check**

```powershell
cargo test foreground_app::tests -- --nocapture
cargo check --all-targets
```

Expected: tests pass and all targets compile.

- [ ] **Step 5: Commit Task 5**

```powershell
git add src/foreground_app.rs src/lib.rs Cargo.toml Cargo.lock
git commit -m "feat: detect foreground Windows application"
```

### Task 6: Effective keymap resolver and atomic reload service

**Files:**
- Create: `src/keymap_resolver.rs`
- Modify: `src/lib.rs`
- Modify: `src/config.rs`
- Test: inline tests in `src/keymap_resolver.rs`

**Interfaces:**
- Consumes: Windows `ShortcutRegistry`, `PluginSnapshot`, and normalized executable name
- Produces: `pub struct KeymapResolver`
- Produces: `pub struct ResolvedKeymap { pub app_name: String, pub registry: Arc<ShortcutRegistry> }`
- Produces: `KeymapResolver::resolve(&self, process: Option<&str>) -> ResolvedKeymap`
- Produces: `pub struct ConfigurationSnapshot { pub global: ShortcutRegistry, pub plugins: Arc<PluginSnapshot> }`
- Produces: `pub struct ConfigurationService` with `current()` and transactional `reload()`

- [ ] **Step 1: Write failing resolver tests**

```rust
#[test]
fn merges_app_over_global_and_falls_back_for_unknown_process() {
    let resolver = fixture_resolver();
    let vscode = resolver.resolve(Some("CODE.EXE"));
    assert_eq!(vscode.app_name, "Visual Studio Code");
    assert_eq!(leaf_desc(&vscode.registry, "C-p"), "Quick Open");
    assert_eq!(leaf_desc(&vscode.registry, "M-d"), "Show desktop");

    let unknown = resolver.resolve(Some("unknown.exe"));
    assert_eq!(unknown.app_name, "Windows");
    assert_eq!(leaf_desc(&unknown.registry, "C-p"), "Global command");
}

#[test]
fn failed_reload_keeps_previous_snapshot() {
    let service = fixture_configuration_service();
    let before = service.current();
    assert!(service.reload_from_sources(INVALID_GLOBAL, &[], Path::new("missing")).is_err());
    assert!(Arc::ptr_eq(&before, &service.current()));
}
```

- [ ] **Step 2: Run focused tests and verify failure**

```powershell
cargo test keymap_resolver::tests -- --nocapture
```

Expected: compilation fails because resolver and configuration snapshot types do not exist.

- [ ] **Step 3: Implement resolver caching and transactional reload**

Build effective registries lazily per normalized process and cache them inside the immutable resolver snapshot. `reload()` must parse all fatal sources into a new `Arc<ConfigurationSnapshot>` first, then replace the current `Arc` under a short-lived `RwLock`; never mutate a live registry.

Invalid user plugins remain warnings in `PluginLoadReport`. Return warnings to the caller so `main.rs` can log each one.

- [ ] **Step 4: Run focused and full library tests**

```powershell
cargo test keymap_resolver::tests -- --nocapture
cargo test --lib
```

Expected: all tests pass.

- [ ] **Step 5: Commit Task 6**

```powershell
git add src/keymap_resolver.rs src/config.rs src/lib.rs
git commit -m "feat: resolve effective application keymaps"
```

## Phase 3: Interaction, UI, and Built-in Plugins

### Task 7: Hook interception and show-all state

**Files:**
- Modify: `src/types.rs`
- Modify: `src/hook.rs`
- Modify: `src/state_machine.rs`
- Test: inline tests in `src/hook.rs` and `src/state_machine.rs`

**Interfaces:**
- Produces: `KeyEvent::ToggleShowAll`
- Produces: `State::BrowsingAll`
- Produces: `UiCommand::ShowAll { app_name: String, entries: Vec<DisplayEntry> }`
- Produces: `KeyboardHook::set_show_all_open(open: bool)`
- Consumes: replaceable `Arc<ShortcutRegistry>` supplied before a trigger is handled

- [ ] **Step 1: Write failing pure hook-decision tests**

Extract a pure decision function and test it without installing a hook:

```rust
#[test]
fn show_all_hotkey_is_swallowed_but_normal_shortcuts_pass() {
    let mut decision = HookDecisionState::default();
    decision.modifiers = ModifierSet::META | ModifierSet::SHIFT;
    assert_eq!(decision.on_key_down(key("/"), false), HookAction::SendAndSwallow(KeyEvent::ToggleShowAll));
    assert_eq!(decision.on_key_down(Key::P, false), HookAction::SendAndPass(KeyEvent::KeyDown(Key::P)));
}

#[test]
fn escape_is_swallowed_only_while_show_all_is_open() {
    let mut decision = HookDecisionState::default();
    assert_eq!(decision.on_key_down(key("Esc"), false), HookAction::SendAndPass(KeyEvent::KeyDown(key("Esc"))));
    assert_eq!(decision.on_key_down(key("Esc"), true), HookAction::SendAndSwallow(KeyEvent::KeyDown(key("Esc"))));
}
```

- [ ] **Step 2: Write failing state-machine tests**

```rust
#[test]
fn toggle_show_all_opens_and_closes_browser() {
    let mut sm = fixture_state_machine();
    let show = sm.handle_event(KeyEvent::ToggleShowAll);
    assert!(matches!(show, Some(UiCommand::ShowAll { .. })));
    assert_eq!(sm.state, State::BrowsingAll);
    let hide = sm.handle_event(KeyEvent::ToggleShowAll);
    assert!(matches!(hide, Some(UiCommand::Hide)));
    assert_eq!(sm.state, State::Idle);
}

#[test]
fn sequence_leaf_hides_after_completion() {
    let mut sm = sequence_state_machine();
    sm.show_immediately_for_test(ModifierSet::CTRL);
    assert!(matches!(sm.handle_event(KeyEvent::KeyDown(Key::K)), Some(UiCommand::UpdateEntries { .. })));
    assert!(matches!(sm.handle_event(KeyEvent::KeyDown(Key::F)), Some(UiCommand::Hide)));
}
```

- [ ] **Step 3: Run focused tests and verify failure**

```powershell
cargo test hook::tests state_machine::tests -- --nocapture
```

Expected: compilation fails because hook decisions, events, state, and command variants do not exist.

- [ ] **Step 4: Implement minimal interception and state transitions**

Keep interception policy in `HookDecisionState`; make the Win32 callback only translate raw events, consult an atomic show-all-open flag, send the event, and return either `CallNextHookEx` or `LRESULT(1)`.

Add `StateMachine::replace_registry(registry: Arc<ShortcutRegistry>, app_name: String)`. In `BrowsingAll`, `Esc` and `ToggleShowAll` produce `Hide`. A completed leaf in modifier mode also produces `Hide`; an intermediate sequence produces `UpdateEntries`.

Construct `UiCommand::ShowAll` from `registry.all_entries()` so unmodified leaves and complete multi-key sequences are visible without navigating groups.

- [ ] **Step 5: Run focused and library tests**

```powershell
cargo test hook::tests state_machine::tests -- --nocapture
cargo test --lib
```

Expected: all tests pass.

- [ ] **Step 6: Commit Task 7**

```powershell
git add src/types.rs src/hook.rs src/state_machine.rs
git commit -m "feat: add show-all shortcut interaction"
```

### Task 8: Integrate foreground resolution, reload, and UI rendering

**Files:**
- Modify: `src/main.rs`
- Modify: `src/tray_icon.rs`
- Modify: `src/webview_bridge.rs`
- Modify: `src/overlay_controller.rs`
- Modify: `src/frontend.html`
- Test: inline serialization tests in `src/webview_bridge.rs`; manual Windows smoke test

**Interfaces:**
- Consumes: `ConfigurationService`, `ForegroundAppProvider`, `KeymapResolver`, `UiCommand::ShowAll`
- Produces: complete runtime flow from trigger to application-aware overlay

- [ ] **Step 1: Write failing WebView command-shape test**

```rust
#[test]
fn show_all_json_contains_app_category_and_priority() {
    let cmd = UiCommand::ShowAll {
        app_name: "Visual Studio Code".to_string(),
        entries: vec![DisplayEntry {
            key: "F2".to_string(),
            desc: "Rename Symbol".to_string(),
            is_group: false,
            category: "Editing".to_string(),
            priority: BindingPriority::Essential,
        }],
    };
    let value = command_json(&cmd);
    assert_eq!(value["type"], "showAll");
    assert_eq!(value["appName"], "Visual Studio Code");
    assert_eq!(value["entries"][0]["category"], "Editing");
    assert_eq!(value["entries"][0]["priority"], "essential");
}
```

- [ ] **Step 2: Run focused test and verify failure**

```powershell
cargo test webview_bridge::tests -- --nocapture
```

Expected: compilation fails because `command_json` and show-all serialization do not exist.

- [ ] **Step 3: Make WebView serialization testable and explicit**

Extract `fn command_json(cmd: &UiCommand) -> serde_json::Value`. Stop swallowing WebView2 creation and HTML navigation errors with `.ok()`: return contextual errors so startup cannot claim a usable overlay after WebView initialization failed.

- [ ] **Step 4: Implement grouped show-all frontend**

In `frontend.html`, render `appName` as a header and group entries by `category`. Add CSS classes for `essential`, `recommended`, and `advanced`; use emphasis, not decorative icons, to distinguish priority. Continue creating shortcut text with `textContent`, never `innerHTML`, so third-party plugin descriptions cannot inject HTML.

- [ ] **Step 5: Wire runtime resolution and transactional reload**

In `main.rs`, before handling the first modifier trigger or `ToggleShowAll`, call `ForegroundAppProvider`, resolve the effective registry, and call `StateMachine::replace_registry`. After every command, synchronize `KeyboardHook::set_show_all_open(matches!(state_machine.state, State::BrowsingAll))`.

While `State::BrowsingAll` is active, compare the normalized foreground executable with the process captured when the panel opened on each existing main-loop tick. If it changes or can no longer be read, dispatch `UiCommand::Hide`, return the state machine to `Idle`, and clear hook interception mode. Do not poll the foreground process while the panel is closed.

Implement tray reload by building a new configuration snapshot and swapping it only on success. Log each skipped user plugin warning. Keep “open config” scoped to the actual `%APPDATA%` global configuration path; do not execute a shell command assembled from user text.

- [ ] **Step 6: Run automated verification**

```powershell
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
```

Expected: all commands exit with status 0.

- [ ] **Step 7: Run the Windows smoke matrix**

Run the app with `RUST_LOG=debug` and verify:

1. Unknown app + hold Ctrl: Windows Ctrl shortcuts appear.
2. VS Code + hold Ctrl: VS Code and Windows entries merge; `C-p` uses VS Code description.
3. VS Code + `Win+Shift+/`: all entries appear, including `F1`, `F2`, and `F12`; the trigger does not reach VS Code.
4. Show-all + Esc: panel closes and Esc does not reach VS Code.
5. `Ctrl+K, Ctrl+F`: UI advances after the prefix, VS Code receives both chords, then UI hides.
6. Switch foreground applications while show-all is open: panel closes.
7. Break one user plugin and reload: old/other plugins continue working and a warning is logged.
8. Break global config and reload: previous snapshot remains active.

- [ ] **Step 8: Commit Task 8**

```powershell
git add src/main.rs src/tray_icon.rs src/webview_bridge.rs src/overlay_controller.rs src/frontend.html
git commit -m "feat: integrate application-aware shortcut UI"
```

### Task 9: Add initial built-in application plugins and user documentation

**Files:**
- Create: `plugins/builtin/vscode.toml`
- Create: `plugins/builtin/word.toml`
- Create: `plugins/builtin/excel.toml`
- Create: `plugins/builtin/powerpoint.toml`
- Create: `README.md`
- Modify: `src/plugin.rs`
- Modify: `docs/PROJECT_MEMORY.md`
- Read: `docs/references/initial-built-in-shortcuts.md`
- Test: plugin snapshot test in `src/plugin.rs`

**Interfaces:**
- Consumes: schema version `1` and `PluginSnapshot::load`
- Produces: embedded built-in source table `pub const BUILTIN_PLUGINS: &[(&str, &str)]`
- Produces: documented third-party plugin workflow

- [ ] **Step 1: Write failing built-in corpus test**

```rust
#[test]
fn every_builtin_plugin_loads_and_expected_processes_exist() {
    let report = PluginSnapshot::load(BUILTIN_PLUGINS, Path::new("missing-user-dir")).unwrap();
    assert!(report.warnings.is_empty());
    for process in ["code.exe", "winword.exe", "excel.exe", "powerpnt.exe"] {
        assert!(report.snapshot.for_process(process).is_some(), "missing {process}");
    }
}
```

- [ ] **Step 2: Run focused test and verify failure**

```powershell
cargo test plugin::tests::every_builtin_plugin_loads_and_expected_processes_exist -- --nocapture
```

Expected: compilation fails because `BUILTIN_PLUGINS` and plugin files do not exist.

- [ ] **Step 3: Create the four approved built-in TOML files**

Transcribe every applicable entry from `docs/references/initial-built-in-shortcuts.md`. Apply these exact normalization rules:

- Mark each “最值得先背” item `essential`.
- Mark ordinary frequent items `recommended`.
- Mark context-specific PowerPoint slideshow items and low-frequency items `advanced`.
- Expand slash groups such as `Ctrl+B / I / U` into separate `[[bindings]]` records.
- Encode `Ctrl+K, Ctrl+F` and `Ctrl+K, Ctrl+S` with `sequence = true`.
- Encode alternative shortcuts such as `Ctrl+Shift+P` and `F1` in one non-sequence `keys` list.
- Exclude `Alt+Click`.
- Use categories `常用`, `编辑`, `导航`, `格式`, `工作表`, and `幻灯片放映` as applicable.

Embed the four sources with `include_str!("../plugins/builtin/<name>.toml")` so packaged applications do not depend on the process working directory.

- [ ] **Step 4: Document installation and authoring**

Create `README.md` with:

- Product purpose and Windows/WebView2 prerequisites.
- Explicit run command: `cargo run --bin which-key-windows`.
- Hold-modifier and `Win+Shift+/` interactions.
- Global config and `%APPDATA%\which-key-windows\plugins\` locations.
- Complete schema-version-1 plugin example.
- Override and `disabled = true` examples.
- Statement that plugins are data-only and direct `*.toml` files only.
- Troubleshooting for invalid user plugins and WebView2 startup failure.

- [ ] **Step 5: Run corpus and full verification**

```powershell
cargo test plugin::tests::every_builtin_plugin_loads_and_expected_processes_exist -- --nocapture
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
```

Expected: all commands exit with status 0.

- [ ] **Step 6: Update project memory with evidence**

Only after Step 5 passes and the Task 8 smoke matrix is complete, change `docs/PROJECT_MEMORY.md` from “implementation has not started” to a concise status containing:

```markdown
## Implementation Status

- Plugin schema/loading: complete and covered by automated tests.
- Foreground application resolution: complete for executable-name matching.
- Show-all and modifier interactions: complete.
- Built-in plugins: VS Code, Word, Excel, and PowerPoint.
- Verification: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --all-targets`, and the Windows smoke matrix passed.
```

Do not record success if any listed verification is pending or failed.

- [ ] **Step 7: Commit Task 9**

```powershell
git add plugins/builtin README.md src/plugin.rs docs/PROJECT_MEMORY.md
git commit -m "feat: ship initial application shortcut plugins"
```

## Final Verification and Handoff

- [ ] Confirm `git status --short` contains no changes accidentally created by this plan's execution; preserve pre-existing unrelated changes.
- [ ] Run `cargo fmt --check`.
- [ ] Run `cargo clippy --all-targets -- -D warnings`.
- [ ] Run `cargo test --all-targets`.
- [ ] Repeat the eight-case Windows smoke matrix from Task 8.
- [ ] Compare delivered behavior against every acceptance criterion in the design spec.
- [ ] Use `superpowers:requesting-code-review` for a final review before integration.
- [ ] Use `superpowers:verification-before-completion` before claiming completion.
- [ ] Use `superpowers:finishing-a-development-branch` to choose merge, PR, or local integration.
