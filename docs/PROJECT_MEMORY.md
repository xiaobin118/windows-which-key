# Project Memory

## Product Direction

Windows Which-Key is a Windows desktop shortcut reminder. Its primary interaction is holding a modifier key to see relevant shortcuts. It is not a shortcut launcher.

The first product milestone focuses on Windows system shortcuts. Application-aware suggestions are added through data-only plugins, beginning with VS Code, Word, Excel, and PowerPoint.

## Confirmed Decisions

- Detect the active application by case-insensitive executable filename only.
- Merge Windows global shortcuts with the active application's shortcuts.
- Precedence: user plugin > built-in plugin > Windows global keymap.
- Ship common plugins with the application and load third-party plugins from `%APPDATA%\which-key-windows\plugins\`.
- Use one TOML file per plugin.
- Plugins are data-only and cannot execute code.
- A user plugin may reuse a built-in plugin `id` to extend or override it.
- `disabled = true` in a user plugin disables the matching built-in plugin.
- Plugin bindings include `category` and `priority` (`essential`, `recommended`, `advanced`).
- Support alternative shortcuts and multi-key sequences.
- Keep the existing hold-modifier interaction.
- Add a configurable “show all shortcuts” hotkey, initially `Win+Shift+/`, for unmodified keys such as `F2` and `F12`.
- Add a global “open control panel” hotkey, initially `Win+Shift+C`; it is intercepted and swallowed like the show-all hotkey. Single left-click on the tray icon also opens the panel.
- The `[theme]` section of the global config is parsed into the configuration snapshot and applied to the overlay as CSS variables; the control panel edits it via `toml_edit` so other sections and comments are preserved.
- Only the application's own show-all hotkey and `Esc` while that panel is open are intercepted. Normal shortcuts pass through to the active application.
- First version does not detect application-internal modes. Context-specific shortcuts use categories such as “幻灯片放映”.
- First version excludes mouse shortcuts, online marketplaces, executable plugins, Web Office, and automatic shortcut execution.

## Architecture Boundaries

- `ForegroundAppDetector`: active window to normalized executable name.
- `PluginLoader`: load, validate, merge, disable, and index plugins.
- `KeymapResolver`: produce the effective keymap for the active process.
- `StateMachine`: handle timing, sequence navigation, show-all mode, and UI commands; no file I/O or direct foreground-process lookup.

Configuration reloads must build a validated immutable snapshot before replacement. Invalid user plugins are isolated; invalid global configuration or built-in plugins are fatal. A failed reload retains the previous snapshot.

## Initial Built-in Plugins

- `codex` / `codex.exe`
- `claude` / `claude.exe`
- `vscode` / `Code.exe`
- `word` / `WINWORD.EXE`
- `excel` / `EXCEL.EXE`
- `powerpoint` / `POWERPNT.EXE`

The detailed approved design is in `docs/superpowers/specs/2026-08-21-application-plugin-system-design.md`.

The approved implementation plan is in `docs/superpowers/plans/2026-08-21-application-plugin-system.md`. The preserved source list for the first built-in plugins is in `docs/references/initial-built-in-shortcuts.md`.

## Implementation Status

- Plugin schema/loading: complete and covered by automated tests.
- Foreground application resolution: complete for executable-name matching.
- Show-all and modifier interactions: complete.
- Built-in plugins: VS Code, Word, Excel, and PowerPoint.
- Control panel (`src/control_panel.{rs,html}`): separate activatable window opened from the tray menu or `Win+Shift+C`; shows hotkeys, plugin inventory, paths, and an editable theme (saved to `[theme]` with comments preserved, applied live to the overlay).
- Automated verification: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test --all-targets` passed offline with the E:\\rust toolchain homes (65 library tests and 4 binary tests).
- Final review fixes include real modifier-aware sequence resolution, exact root-entry filtering, optional plugin descriptions, and a keyboard-hook install/uninstall handshake that reports failures and joins its worker thread.
- Manual Windows application smoke: pending for VS Code, Office, and multi-monitor cases.

- Overlay layout: neovim which-key style — anchored to the bottom edge of the screen, full width, entries flow in auto-filled columns, and the window height is estimated per render so all entries are visible without scrolling.
- Control panel window is centered on the primary screen; theme edits preview live on the overlay and are persisted only by the explicit save button.

## Known Verification Gap

The Windows smoke matrix requires installed target applications and an interactive Windows desktop. Do not record it as passed until those manual cases have been run.

## Open TODO

- Add a Windows installer package for GitHub releases, starting with NSIS as the primary path and MSI only if enterprise distribution later requires it.
- Add a first-run or installer-time shortcut for creating the `%APPDATA%\which-key-windows\` config and plugin folders if needed by the installer flow.
- Validate the release artifact on a clean Windows machine before calling the release process complete.

## Bug Log

- 2026-08-22 — Overlay footer not pinned to the bottom when the entry list is shorter than the window (for example after pressing Ctrl then Shift, which filters down to the `C-S-` list). The footer sat right after the last entry instead of at the container bottom. `position: sticky` did not help because a short list never scrolls. Fixed by making `.container.visible` a vertical flex column and giving `.footer` `margin-top: auto`.
