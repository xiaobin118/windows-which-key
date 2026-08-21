# Windows Which-Key

Windows Which-Key 是 Windows 桌面快捷键提示工具：按住修饰键即可查看当前应用可用的快捷键。它不会执行或拦截普通快捷键。

## 运行条件与启动

- Windows 10 或更高版本。
- 已安装 [Microsoft Edge WebView2 Runtime](https://developer.microsoft.com/microsoft-edge/webview2/)；程序界面依赖它启动。
- Rust 工具链（仅从源码运行时需要）。

在项目根目录运行：

```powershell
cargo run --bin which-key-windows
```

按住配置的修饰键会显示与前台应用匹配的快捷键；松开后隐藏。`Win+Shift+/` 可打开“显示全部”面板，以便查看 `F2`、`F12` 等不含修饰键的快捷键；该面板打开时按 `Esc` 关闭。

## 配置与插件位置

全局配置位于 `%APPDATA%\which-key-windows\which-key.toml`，用户插件直接放在：

```text
%APPDATA%\which-key-windows\plugins\
```

内置插件覆盖 VS Code、Word、Excel 与 PowerPoint。Windows 全局快捷键会与当前应用的插件快捷键合并显示。

## 插件格式（schema version 1）

每个插件是一个直接放在 `plugins` 目录中的 `*.toml` 文件，不扫描子目录。插件是纯数据：不能执行代码、命令或脚本。

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

# 多个等价快捷键：按其中任一个都适用。
[[bindings]]
keys = ["Ctrl+Shift+P", "F1"]
description = "打开命令面板"
category = "常用"
priority = "recommended"

# 多键序列：按顺序输入 Ctrl+K，再输入 Ctrl+F。
[[bindings]]
keys = ["Ctrl+K", "Ctrl+F"]
description = "格式化选择内容"
category = "格式"
priority = "recommended"
sequence = true
```

`schema_version` 必须为 `1`。`id`、`name`、`processes` 及每个绑定的 `keys`、`description`、`category`、`priority` 都是必填项。`priority` 只能是 `essential`、`recommended` 或 `advanced`。可用按键写法包括 `Ctrl+P`、`Alt+Up`、`Shift+F5`、`PageDown`、`Ctrl+Shift++`。

## 覆盖或禁用内置插件

用户插件的 `id` 与内置插件相同时，会覆盖同名按键，并保留未提及的内置绑定。例如自定义 VS Code 的快速打开提示：

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

要完全关闭一个内置插件，在用户插件目录创建同 `id` 的文件：

```toml
schema_version = 1
id = "vscode"
name = "Visual Studio Code"
processes = ["Code.exe"]
disabled = true
```

## 排查问题

- 用户插件无效时，应用会忽略该文件并显示警告，其他插件仍会加载。检查 schema 版本、必填字段、按键写法和重复绑定。
- 内置插件或全局配置无效会阻止配置加载；恢复随程序发布的内置文件，或修正 `%APPDATA%\which-key-windows\which-key.toml`。
- 若启动时提示 WebView2 相关错误或窗口没有显示，安装/修复 WebView2 Runtime 后重试；确认 Windows 版本受支持。
