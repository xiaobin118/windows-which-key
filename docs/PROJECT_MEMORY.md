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

- `vscode` / `Code.exe`
- `word` / `WINWORD.EXE`
- `excel` / `EXCEL.EXE`
- `powerpoint` / `POWERPNT.EXE`

The detailed approved design is in `docs/superpowers/specs/2026-08-21-application-plugin-system-design.md`.

## Current Scope Boundary

The design is approved, but implementation has not started. Before implementation, create a detailed plan and preserve unrelated existing working-tree changes.
