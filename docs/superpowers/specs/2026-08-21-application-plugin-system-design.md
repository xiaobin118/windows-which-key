# Windows Which-Key 应用插件系统设计

## 1. 背景与目标

Windows Which-Key 的核心定位是 Windows 桌面快捷键提示器。第一阶段聚焦 Windows 系统级快捷键；后续通过应用插件识别当前前台程序，并展示适合该程序的快捷键建议。

首批内置应用插件包括：

- Visual Studio Code
- Microsoft Word
- Microsoft Excel
- Microsoft PowerPoint

系统同时允许用户从本地目录添加第三方插件，或覆盖、扩展、禁用内置插件。

## 2. 产品边界

### 2.1 第一版包含

- 根据前台窗口的可执行文件名识别应用。
- Windows 全局快捷键与当前应用快捷键合并展示。
- 内置插件和本地用户插件。
- 单文件 TOML 插件格式。
- 长按修饰键显示对应快捷键。
- 独立热键显示当前应用的全部推荐快捷键。
- 多键序列提示，例如 `Ctrl+K, Ctrl+F`。
- 按分类和推荐优先级组织快捷键。
- 配置和插件重新加载时保留最后一个有效快照。

### 2.2 第一版不包含

- 在线插件市场和自动下载。
- 可执行 Rust、JavaScript 或其他代码的插件。
- 应用内部状态识别，例如 PowerPoint 是否正在放映。
- VS Code editor context 或扩展状态识别。
- Web 版 Office。
- 鼠标组合，例如 `Alt+Click`。
- 自动执行插件声明的快捷键。

## 3. 核心设计决策

### 3.1 应用识别

第一版只使用当前前台窗口所属进程的可执行文件名，进行不区分大小写的匹配：

- VS Code：`Code.exe`
- Word：`WINWORD.EXE`
- Excel：`EXCEL.EXE`
- PowerPoint：`POWERPNT.EXE`

窗口标题、文件类型和应用内部模式不参与第一版匹配。无法识别前台应用时，回退到 Windows 全局快捷键。

### 3.2 插件安全模型

插件是纯数据 TOML 文件，不执行代码。这样可以降低第三方插件的权限风险，并使插件易于审查、分享和版本控制。

### 3.3 插件来源和优先级

插件有两个来源：

- 内置插件：项目中的 `plugins/builtin/*.toml`，随应用发布。
- 用户插件：`%APPDATA%\which-key-windows\plugins\*.toml`。

最终快捷键映射的优先级从低到高为：

1. Windows 全局快捷键
2. 内置应用插件
3. 用户应用插件

同一规范化按键或按键序列发生冲突时，高优先级定义替换低优先级定义；没有冲突的条目共同保留。

用户插件可以复用内置插件的 `id`，逐项覆盖或扩展它。用户插件设置 `disabled = true` 时，关闭对应内置插件。

## 4. 组件边界

### 4.1 `ForegroundAppDetector`

职责：

- 获取当前前台窗口。
- 查询窗口所属进程。
- 返回规范化的可执行文件名。

它不了解插件、配置合并和快捷键。

### 4.2 `PluginLoader`

职责：

- 加载和校验内置插件。
- 扫描并加载用户插件。
- 合并同 `id` 插件。
- 处理 `disabled`。
- 建立 `process_name -> plugin` 索引。

它不访问当前前台窗口。

### 4.3 `KeymapResolver`

职责：

- 接收 Windows 全局映射、当前进程和插件快照。
- 按确定的优先级合并映射。
- 输出本次交互使用的 `EffectiveKeymap`。

### 4.4 `StateMachine`

职责：

- 管理快捷键提示的时序和导航状态。
- 处理长按修饰键、多键序列和“显示全部”模式。
- 生成 `UiCommand`。

它不读取文件，也不直接调用前台窗口 API。

## 5. 数据流

1. 用户长按修饰键，或按下“显示全部”热键。
2. `ForegroundAppDetector` 获取当前进程名。
3. `KeymapResolver` 从不可变插件快照中查找匹配插件。
4. Windows 全局、内置应用和用户应用映射按优先级合并。
5. `StateMachine` 根据当前模式过滤或导航快捷键树。
6. `OverlayController` 显示最终条目。

切换前台应用时，已打开的“显示全部”面板自动关闭，避免显示过期上下文。

## 6. 插件 TOML Schema

每个插件使用一个 TOML 文件：

```toml
schema_version = 1
id = "vscode"
name = "Visual Studio Code"
description = "VS Code for Windows desktop"
processes = ["Code.exe"]
disabled = false

[[bindings]]
keys = ["C-p"]
description = "按文件名快速打开文件"
category = "导航"
priority = "essential"

[[bindings]]
keys = ["C-S-p", "F1"]
description = "打开命令面板"
category = "常用"
priority = "essential"

[[bindings]]
keys = ["C-k", "C-f"]
description = "格式化选中代码"
category = "编辑"
priority = "recommended"
sequence = true
```

### 6.1 顶层字段

- `schema_version`：必填，第一版只接受 `1`。
- `id`：必填，稳定且唯一的插件标识。
- `name`：必填，UI 显示名称。
- `description`：可选，插件说明。
- `processes`：必填，匹配的可执行文件名列表。
- `disabled`：可选，默认 `false`。

### 6.2 Binding 字段

- `keys`：必填，按键字符串列表。
- `description`：必填，功能描述。
- `category`：必填，例如“常用”“编辑”“导航”“幻灯片放映”。
- `priority`：必填，取值为 `essential`、`recommended`、`advanced`。
- `sequence`：可选，默认 `false`。

当 `sequence = false` 时，多个 `keys` 表示同一功能的替代快捷键；当 `sequence = true` 时，多个 `keys` 表示必须依次输入的多键序列。

### 6.3 按键规范

修饰键采用短格式：

- `C`：Ctrl
- `A`：Alt
- `S`：Shift
- `M`：Win/Meta

示例：`C-p`、`C-S-p`、`S-A-f`、`C-PageDown`、`F12`、`Up`。

解析器可以接受 `Ctrl-Shift-P` 等长格式，但内置插件统一使用短格式。合并前必须把按键和进程名规范化。

## 7. 交互设计

### 7.1 修饰键提示模式

用户按住 `Ctrl`、`Alt`、`Shift` 或 `Win` 约 300 ms 后：

- 检测当前前台应用。
- 合并 Windows 与应用映射。
- 仅显示与当前修饰键集合完全匹配的条目。
- 快捷键事件继续传给当前应用，工具只观察和提示。

条目首先按 `essential`、`recommended`、`advanced` 排序，然后按分类和快捷键排序。

### 7.2 多键序列

对于 `Ctrl+K, Ctrl+F`：

1. 第一段 `Ctrl+K` 继续传给前台应用。
2. 提示窗进入对应快捷键树的下一层。
3. 用户输入第二段，应用正常执行该序列。
4. 匹配完成后隐藏提示窗。

### 7.3 显示全部模式

默认热键为 `Win+Shift+/`，后续允许用户配置：

- 热键由 Which-Key 拦截，不传给前台应用。
- 展示当前应用全部推荐快捷键，包括 `F2`、`F12` 等无修饰键条目。
- 再次按热键关闭面板。
- 面板打开时按 `Esc` 关闭，并拦截该次 `Esc`。
- 第一版面板只用于浏览，不选择或执行快捷键。

### 7.4 应用上下文

第一版通过 `category` 表达应用内部上下文。例如 PowerPoint 的 `B`、`W` 和 `E` 归入“幻灯片放映”，但程序不会声称已检测当前是否处于放映状态。

## 8. 错误处理

启动和 reload 都先构建完整临时快照，校验通过后再替换当前快照。

- Windows 全局配置无效：启动失败并报告原因。
- 内置插件无效：视为应用构建缺陷，启动失败。
- 单个用户插件无效：跳过该插件并报告错误，其余功能继续运行。
- 前台进程识别失败：只使用 Windows 全局映射。
- WebView2 初始化失败：明确报告，不显示无内容的空白窗口。
- reload 失败：保留上一个有效快照。
- 同一来源的不同插件匹配同一进程：报告配置冲突，不依赖文件扫描顺序决定结果。

## 9. 测试策略

### 9.1 纯逻辑测试

- 按键字符串规范化。
- 替代快捷键和多键序列解析。
- 三层映射合并和冲突优先级。
- 同名插件覆盖和 `disabled`。
- 分类和优先级排序。
- 无效用户插件隔离。
- 进程名大小写匹配。
- 未知应用回退全局映射。
- 长按修饰键和“显示全部”状态转换。

### 9.2 Windows 集成测试

- 从前台 `HWND` 获取正确进程名。
- 工具热键被拦截，普通应用快捷键继续传递。
- 切换前台应用后关闭过期面板。
- reload 失败时保留有效快照。

### 9.3 手工验收

- Windows 10 和 Windows 11。
- 单显示器和多显示器。
- VS Code、Word、Excel、PowerPoint。
- WebView2 缺失或初始化失败。
- 用户插件覆盖、禁用和格式错误。

## 10. 首批插件数据

首批插件使用用户提供的推荐快捷键清单作为产品输入。录入时应：

- 将“最值得先背”的条目标记为 `essential`。
- 将其他常用条目标记为 `recommended`，低频或上下文较强的条目标记为 `advanced`。
- 拆分表格中的组合写法，例如 `Ctrl+B / I / U`。
- 保留 PowerPoint“幻灯片放映”等上下文分类。
- 暂不录入 `Alt+Click`。
- 将 `Ctrl+K, Ctrl+F` 等录入为 `sequence = true`。

## 11. 验收标准

- 当前进程能够稳定选择正确的应用插件。
- 未匹配应用时只显示 Windows 全局快捷键。
- Windows、内置插件和用户插件按确定优先级合并。
- 用户可以新增、覆盖和禁用插件。
- 长按修饰键只显示匹配的快捷键。
- “显示全部”模式能够展示无修饰键条目。
- 多键序列在 UI 中逐层提示，同时按键继续传给前台应用。
- 损坏的用户插件不会导致整个程序不可用。
- 所有纯逻辑规则有自动化测试。
