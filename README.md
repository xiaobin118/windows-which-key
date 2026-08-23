# Windows Which-Key v1.5.1

Windows Which-Key is a Windows desktop shortcut hint tool: hold down the modifier key to view the available shortcuts for the current application. It does not execute or block regular shortcuts.

Read in English: [README.md](./README.md)

Read in Chinese: [README.zh-CN.md](./README.zh-CN.md)

## Quick Start

- Windows 10 or later.
- [Microsoft Edge WebView2 Runtime](https://developer.microsoft.com/microsoft-edge/webview2/) installed.
- Rust toolchain only if you run from source.

## Install

Use one of the release packages from [GitHub Releases](https://github.com/xiaobin118/windows-which-key/releases):

- NSIS installer: run `which-key-windows-setup.exe`. It installs the app, creates Start Menu and desktop shortcuts, and registers an uninstaller.
- Portable zip: unzip `which-key-windows-v1.5.1-windows-x64.zip` and run `which-key-windows.exe`.
- Source build: clone the repository and run `cargo run --bin which-key-windows`.

## Usage Options

Pick the path that fits how you want to use the app:

- Installer: best for regular use. It installs to `Program Files`, creates Start Menu and desktop shortcuts, and gives you a normal uninstall entry.
- Portable zip: best for trying the app or keeping it on a USB drive. Unzip and run the executable directly.
- Run from source: best for development or local debugging. Use `cargo run --bin which-key-windows`.

For daily use, the app is designed around three main actions:

- Hold a modifier key to show shortcuts for the active app.
- Press `Win+Shift+/` to open the full shortcut panel.
- Use the tray icon to reload config, open the control panel, or quit.

Run from the project root:

```powershell
cargo run --bin which-key-windows
```

Hold the configured modifier key to show shortcuts for the foreground app, then release to hide them. `Win+Shift+/` opens the “show all” panel so you can inspect shortcuts such as `F2` and `F12`. Press `Esc` to close that panel.

## Configuration

Global configuration lives at `%APPDATA%\which-key-windows\which-key.toml`.

User plugins live in:

```text
%APPDATA%\which-key-windows\plugins\
```

Built-in plugins cover Codex Desktop, Codex CLI, Claude Code, VS Code, Word, Excel, and PowerPoint. Windows global shortcuts are merged with the active app's shortcuts.

Open the control panel with `Win+Shift+C` or from the tray icon. From there you can open the config file, open the plugin directory, import/export plugin bundles, create a plugin template, edit user plugins, adjust the theme, and toggle autostart.

## Plugin Format

Each plugin is a `*.toml` file placed directly in the `plugins` directory. Plugins are data-only: they cannot execute code, commands, or scripts.

```toml
schema_version = 1
id = "my-editor"
name = "My Editor"
processes = ["myeditor.exe"]

[[bindings]]
keys = ["Ctrl+P"]
description = "Open file"
category = "Common"
priority = "essential"

[[bindings]]
keys = ["Ctrl+Shift+P", "F1"]
description = "Open command palette"
category = "Common"
priority = "recommended"

[[bindings]]
keys = ["Ctrl+K", "Ctrl+F"]
description = "Format selection"
category = "Formatting"
priority = "recommended"
sequence = true
```

`schema_version` must be `1`. `id`, `name`, `processes`, and each binding's `keys`, `description`, `category`, and `priority` are required. Valid priorities are `essential`, `recommended`, and `advanced`.

## Override or Disable Built-ins

If a user plugin has the same `id` as a built-in plugin, it overrides the matching bindings and keeps the rest.

```toml
schema_version = 1
id = "vscode"
name = "My VS Code"
processes = ["Code.exe"]

[[bindings]]
keys = ["Ctrl+P"]
description = "Quick open project files"
category = "Common"
priority = "essential"
```

To disable a built-in plugin completely, create a user plugin file with the same `id` and set `disabled = true`.

## Release Downloads

The repository ships Windows release packages as GitHub Release assets. Download the latest installer or the portable zip.

- Installer: run `which-key-windows-setup.exe` and follow the prompts.
- Portable: unzip `which-key-windows-v1.5.1-windows-x64.zip` and run `which-key-windows.exe`.

Package contents are intentionally small:

- `which-key-windows.exe`
- `README.md`
- `README.zh-CN.md`

The installer also creates Start Menu and desktop shortcuts.

The app creates its config and plugin folders under `%APPDATA%\which-key-windows\` on first launch.

## Troubleshooting

- Invalid user plugins are skipped with a warning; other plugins still load.
- Invalid built-in plugins or global config block startup.
- If WebView2 errors appear or the window does not show, repair WebView2 Runtime and try again.
