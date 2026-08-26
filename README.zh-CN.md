# Windows Which-Key v1.5.2

Windows Which-Key 是 Windows 桌面快捷键提示工具：按住修饰键即可查看当前应用可用的快捷键。它不会执行或拦截普通快捷键。

[![Release](https://img.shields.io/github/v/release/xiaobin118/windows-which-key?display_name=tag)](https://github.com/xiaobin118/windows-which-key/releases)
[![Build](https://github.com/xiaobin118/windows-which-key/actions/workflows/release.yml/badge.svg)](https://github.com/xiaobin118/windows-which-key/actions/workflows/release.yml)
[![License](https://img.shields.io/github/license/xiaobin118/windows-which-key)](https://github.com/xiaobin118/windows-which-key)

<video controls muted loop width="800">
  <source src="https://github.com/xiaobin118/windows-which-key/raw/refs/heads/master/docs/assets/demo.mp4" type="video/mp4">
  当前浏览器不支持内嵌视频，[打开演示视频](docs/assets/demo.mp4)。
</video>

## 为什么使用 Windows Which-Key？

- 不用记住每个应用的全部快捷键，按住修饰键即可查看。
- 内置支持 Codex、Claude Code、Chrome / Edge、Windows Terminal、VS Code、Word、Excel 与 PowerPoint。
- 使用纯数据 TOML 插件添加或覆盖快捷键。
- Windows 全局快捷键会与应用快捷键一起显示。

Read in English: [README.md](./README.md)

Read in Chinese: [README.zh-CN.md](./README.zh-CN.md)

## 快速开始

- Windows 10 或更高版本。
- 已安装 [Microsoft Edge WebView2 Runtime](https://developer.microsoft.com/microsoft-edge/webview2/)。
- 仅从源码运行时需要 Rust 工具链。

## 安装

从 [GitHub Releases](https://github.com/xiaobin118/windows-which-key/releases) 下载发布包：

- NSIS 安装器：运行 `which-key-windows-setup.exe`。它会安装程序、创建开始菜单和桌面快捷方式，并注册卸载项。
- 便携版 zip：解压 `which-key-windows-v1.5.2-windows-x64.zip`，然后运行 `which-key-windows.exe`。
- 源码运行：克隆仓库后执行 `cargo run --bin which-key-windows`。

在项目根目录运行：

```powershell
cargo run --bin which-key-windows
```

按住配置的修饰键会显示与前台应用匹配的快捷键，松开后隐藏。`Win+Shift+/` 可打开“显示全部”面板，以便查看 `F2`、`F12` 等不含修饰键的快捷键；按 `Esc` 关闭该面板。

## 配置

全局配置位于 `%APPDATA%\which-key-windows\which-key.toml`。

用户插件位于：

```text
%APPDATA%\which-key-windows\plugins\
```

内置插件覆盖 Codex Desktop、Codex CLI、Claude Code、Chrome / Edge、Windows Terminal、VS Code、Word、Excel 与 PowerPoint。Windows 全局快捷键会与当前应用的快捷键合并显示。

可以通过 `Win+Shift+C` 或托盘图标打开控制面板。在控制面板中可以打开配置文件、打开插件目录、导入/导出插件包、新建插件模板、编辑用户插件、调整主题和切换开机自启。

## 插件格式

每个插件是一个直接放在 `plugins` 目录中的 `*.toml` 文件。插件是纯数据：不能执行代码、命令或脚本。

```toml
schema_version = 1
id = "my-editor"
name = "My Editor"
processes = ["myeditor.exe"]

[[bindings]]
keys = ["Ctrl+P"]
description = "打开文件"
category = "常用"
priority = "essential"

[[bindings]]
keys = ["Ctrl+Shift+P", "F1"]
description = "打开命令面板"
category = "常用"
priority = "recommended"

[[bindings]]
keys = ["Ctrl+K", "Ctrl+F"]
description = "格式化选择内容"
category = "格式"
priority = "recommended"
sequence = true
```

`schema_version` 必须为 `1`。`id`、`name`、`processes` 以及每个绑定的 `keys`、`description`、`category`、`priority` 都是必填项。`priority` 只能是 `essential`、`recommended` 或 `advanced`。

## 覆盖或禁用内置插件

用户插件的 `id` 与内置插件相同时，会覆盖同名按键，并保留未提及的内置绑定。

```toml
schema_version = 1
id = "vscode"
name = "My VS Code"
processes = ["Code.exe"]

[[bindings]]
keys = ["Ctrl+P"]
description = "项目文件快速打开"
category = "常用"
priority = "essential"
```

要完全关闭一个内置插件，在用户插件目录创建同 `id` 的文件，并设置 `disabled = true`。

## Release 安装包

仓库发布页会提供 GitHub Release 资源包。下载最新的安装器或便携版 zip 都可以。

- 安装器：运行 `which-key-windows-setup.exe`，按提示完成安装。
- 便携版：解压 `which-key-windows-v1.5.2-windows-x64.zip` 后直接运行 `which-key-windows.exe`。

发布包只保留最必要的文件：

- `which-key-windows.exe`
- `README.md`
- `README.zh-CN.md`

安装器会同时创建开始菜单和桌面快捷方式。

程序首次启动时会自动创建 `%APPDATA%\which-key-windows\` 下的配置和插件目录。

## 排查问题

- 用户插件无效时，应用会忽略该文件并显示警告，其他插件仍会加载。
- 内置插件或全局配置无效会阻止启动。
- 若启动时提示 WebView2 相关错误或窗口没有显示，安装/修复 WebView2 Runtime 后重试。

## 参与贡献

欢迎提交问题报告、快捷键修正、新内置插件建议和代码贡献。提交 issue 或 pull request 前，请先阅读 [CONTRIBUTING.md](CONTRIBUTING.md)。

如果这个项目对你有帮助，欢迎在 GitHub 上点一个 star。

## 灵感
[folke/which-key.nvim](https://github.com/folke/which-key.nvim) 一个neovim插件， 帮助你使用自己的keymap
