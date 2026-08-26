# Contributing

Thanks for helping improve Windows Which-Key.

感谢你帮助改进 Windows Which-Key。

## What to Contribute

## 可以贡献什么

- Bug reports with clear reproduction steps.
- Corrections to built-in shortcut definitions.
- New built-in plugin proposals with a reliable source for the shortcuts.
- Rust, UI, documentation, and test improvements.

- 提供包含清晰复现步骤的问题报告。
- 修正内置快捷键定义。
- 提议新增内置插件，并提供可靠的快捷键来源。
- 改进 Rust 代码、UI、文档和测试。

## Development

## 开发环境

Requirements:

环境要求：

- Windows 10 or later.
- Rust stable toolchain.
- Microsoft Edge WebView2 Runtime for running the desktop app.

- Windows 10 或更高版本。
- Rust stable toolchain。
- 运行桌面应用需要 Microsoft Edge WebView2 Runtime。

Run the test suite before opening a pull request:

创建 pull request 前，请先运行测试套件：

```powershell
cargo test --lib
```

Run the application from the repository root with:

在仓库根目录运行应用：

```powershell
cargo run --bin which-key-windows
```

## Plugin Contributions

## 插件贡献

Built-in plugins are data-only TOML files under `plugins/builtin/`. Keep definitions focused on common shortcuts, use the existing schema, and add or update tests when behavior changes.

内置插件是位于 `plugins/builtin/` 下的纯数据 TOML 文件。请聚焦于常用快捷键，使用现有 schema，并在行为发生变化时新增或更新测试。

## Pull Requests

## Pull Request

- Keep changes focused and explain the user-visible behavior.
- Preserve existing formatting and avoid unrelated refactors.
- Include tests or a clear reason why tests are not applicable.
- Do not commit generated release packages or local screenshots.

- 保持修改范围明确，并说明用户可见的行为变化。
- 保持现有格式，避免无关重构。
- 添加测试，或清楚说明为什么不适合添加测试。
- 不要提交生成的发布包或本地截图。

## Issues

## Issue

Use the issue templates for bug reports and feature requests. Please include your Windows version, app version, reproduction steps, and relevant logs where applicable.

提交 bug report 或 feature request 时，请使用对应的 issue template。在适用的情况下，请提供 Windows 版本、应用版本、复现步骤和相关日志。
