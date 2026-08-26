# Contributing

Thanks for helping improve Windows Which-Key.

## What to Contribute

- Bug reports with clear reproduction steps.
- Corrections to built-in shortcut definitions.
- New built-in plugin proposals with a reliable source for the shortcuts.
- Rust, UI, documentation, and test improvements.

## Development

Requirements:

- Windows 10 or later.
- Rust stable toolchain.
- Microsoft Edge WebView2 Runtime for running the desktop app.

Run the test suite before opening a pull request:

```powershell
cargo test --lib
```

Run the application from the repository root with:

```powershell
cargo run --bin which-key-windows
```

## Plugin Contributions

Built-in plugins are data-only TOML files under `plugins/builtin/`. Keep definitions focused on common shortcuts, use the existing schema, and add or update tests when behavior changes.

## Pull Requests

- Keep changes focused and explain the user-visible behavior.
- Preserve existing formatting and avoid unrelated refactors.
- Include tests or a clear reason why tests are not applicable.
- Do not commit generated release packages or local screenshots.

## Issues

Use the issue templates for bug reports and feature requests. Please include your Windows version, app version, reproduction steps, and relevant logs where applicable.
