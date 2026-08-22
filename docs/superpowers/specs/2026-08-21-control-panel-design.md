# Which-Key 控制面板设计

## 1. 背景与目标

Which-Key 需要一个独立 GUI，让用户在不直接编辑配置文件的情况下自定义浮层主题，并管理声明式 TOML 应用插件。控制面板复用现有 WebView2 技术栈，但使用独立页面、独立窗口状态和正式的双向 WebView 消息协议。

第一版只交付：

- Appearance：完整主题编辑、实时预览、保存、重置。
- Plugins：查看、启用/禁用、打开目录、创建基础用户插件。
- About：显示版本、目录和 WebView2 状态。
- Global Keys：仅提供打开原始 `which-key.toml`，暂不实现可视化 CRUD。

插件仍是纯数据 TOML，不执行脚本、DLL 或其他代码。

## 2. 架构边界

控制面板不直接读写配置文件，也不负责插件解析和快照构建。

```text
ControlPanelController
  ├─ 管理独立 WebView2 窗口生命周期
  ├─ 接收/发送 Web Message JSON
  └─ 调用 ConfigurationService

ConfigurationService
  ├─ 反序列化与语义校验
  ├─ revision 冲突检测
  ├─ 构建候选 RuntimeSnapshot
  ├─ 主题和插件原子保存
  └─ 成功后替换 RuntimeSnapshotStore

RuntimeSnapshotStore
  └─ 保存当前不可变 RuntimeSnapshot
```

统一运行时快照：

```rust
struct RuntimeSnapshot {
    generation: u64,
    revisions: ResourceRevisions,
    settings: AppSettings,
    theme: ThemeConfig,
    global_keymap: ShortcutRegistry,
    plugins: PluginSnapshot,
}

struct ResourceRevisions {
    theme: ContentRevision,
    global_config: ContentRevision,
    plugins: HashMap<PluginId, ContentRevision>,
}
```

控制面板、托盘 reload 和程序启动必须复用同一条 `ConfigurationService` 配置路径，避免出现不同的解析或校验行为。

## 3. 资源组织

浮层和控制面板共享设计 token、组件样式和 bridge 协议，但不共享 DOM 或页面状态：

```text
ui/
├── shared/
│   ├── tokens.css
│   ├── components.css
│   └── bridge.js
├── overlay.html
└── control-panel.html
```

控制面板使用独立 WebView2 窗口；关闭、Dirty 状态、导航和表单状态不影响快捷键浮层。

## 4. 保存事务与快照替换

主题或用户插件保存必须按以下顺序执行：

```text
JSON request
→ deserialize
→ semantic validation
→ serialize to temporary file
→ build candidate snapshot from memory/temporary data
→ flush temporary file
→ atomic replace target
→ infallibly swap RuntimeSnapshot
→ notify overlay
```

所有可能失败的反序列化、语义校验、插件解析和候选快照构建都必须发生在目标文件替换前。保存失败时保留旧文件、旧快照和表单内容；原子替换成功后只执行内存快照交换和通知。

### 4.1 外部修改冲突

配置资源分别维护 revision：主题、全局配置和每个插件各有独立的 `ContentRevision`；`generation` 只表示整体运行时快照发生过变化，不作为所有保存请求的冲突依据。控制面板打开页面时记录目标资源 revision，保存请求只携带对应资源的 revision。这样外部新增插件不会导致未修改主题的保存产生错误冲突。

保存操作由 `ConfigurationService` 内部的写锁串行化，并在原子替换前再次检查目标资源 revision：

```text
取得配置写锁
→ 读取并检查目标资源 revision
→ 校验请求
→ 构建候选文件和候选快照
→ flush 临时文件
→ 再次确认目标文件未变化
→ atomic replace
→ 递增 snapshot generation
→ 交换 RuntimeSnapshot
→ 释放写锁
→ 通知 UI
```

如果目标资源已被外部编辑，服务返回 `revision_conflict`，拒绝静默覆盖。

用户必须显式选择：

- Reload：放弃当前表单修改，重新读取磁盘。
- Overwrite：基于当前表单重新构建候选快照并明确覆盖。
- Cancel：留在 Conflict 状态。

## 5. 主题模型与约束

主题保存到 `%APPDATA%\\which-key-windows\\theme.toml`，使用稳定 token，不保存 CSS 片段：

```toml
[theme]
background = "#0B0F1F"
background_opacity = 0.92
border = "#91A4FF"
border_opacity = 0.20
accent = "#618CFF"
text_primary = "#EDF1FF"
text_secondary = "#D2DCFF"
radius = 20
blur = 24
row_spacing = 6
```

验证约束：

- 颜色只接受 `#RRGGBB`。
- opacity 范围为 `0.0..=1.0`。
- radius 范围为 `0..=32`。
- blur 范围为 `0..=64`。
- row spacing 范围为 `0..=24`。
- 低对比度只产生 warning，不阻止保存。

Dirty 状态下，主题只更新控制面板内的预览组件，不修改真实浮层或运行时快照。

## 6. 控制面板页面

### 6.1 Appearance

表单编辑背景、边框、强调色、主/次文字颜色、透明度、圆角、模糊和行间距。右侧显示快捷键浮层预览；`Reset` 由前端从已加载的 `defaultTheme` 恢复表单并保持 Dirty，不直接持久化；只有 `saveTheme` 会写入文件，保存成功后刷新主题 revision 并通知 overlay 使用新主题。

页面状态为：`Loading`、`Ready`、`Dirty`、`Saving`、`Saved`、`Conflict`、`Error`。

### 6.2 Plugins

列表显示插件名称、ID、来源、匹配进程、启用状态、快捷键条目数和错误状态。

用户插件支持：

- 查看详情。
- 启用/禁用。
- 创建基础插件。
- 打开已发现的插件文件。
- 打开用户插件目录。

内置插件只读、不可删除。用户插件可复用内置插件 ID 进行覆盖或扩展。

创建基础插件的字段为 ID、名称、描述、进程名列表和一条初始快捷键记录。保存必须复用正式 `PluginLoader` 验证。

### 6.3 About

显示程序版本、全局配置目录、用户插件目录和 WebView2 状态，并提供打开目录操作。

### 6.4 Global Keys

第一版只显示说明和“打开原始配置文件”按钮，不在控制面板中实现全局快捷键可视化 CRUD。

关闭控制面板时：

- Ready/Saved：直接关闭。
- Dirty：显示保存、放弃、取消。
- Saving：禁用关闭或等待响应。
- Conflict：显示 Reload、Overwrite、Cancel。
- Error：保留表单内容，不自动关闭。

## 7. WebView2 消息协议

控制面板使用 WebView2 Web Message API 双向通信。Rust 不把用户内容拼接进动态 JavaScript 字符串。WebView2 必须拒绝外部导航和新窗口请求；Release 构建关闭 DevTools。

请求格式：

```json
{
  "requestId": "uuid",
  "type": "saveTheme",
  "resourceRevision": "sha256:theme-content",
  "payload": {}
}
```

创建新插件不依赖已有插件资源，因此 `resourceRevision` 为 `null`；保存已有插件时必须携带该插件 ID 对应的 revision。

```json
{
  "requestId": "uuid",
  "type": "createPlugin",
  "resourceRevision": null,
  "payload": {
    "id": "my-app"
  }
}
```

第一版请求类型：

- `getControlPanelState`
- `saveTheme`
- `listPlugins`
- `createPlugin`
- `setPluginEnabled`
- `openPluginFile`（只提交插件 `id`，由 Rust 从已发现插件索引解析规范化路径）
- `openPluginDirectory`
- `reloadFromDisk`

响应格式：

```json
{
  "replyTo": "uuid",
  "ok": true,
  "data": null,
  "warnings": [],
  "error": null
}
```

失败响应将 `ok` 设为 `false` 并填写 `error`。成功响应可以携带 `warnings`，例如低对比度主题警告；warning 不伪装成失败。错误码固定为：`invalid_json`、`unknown_message`、`validation_failed`、`revision_conflict`、`path_not_allowed`、`plugin_invalid`、`file_io_failed`、`snapshot_build_failed`、`webview_unavailable`。

字段错误携带 `field`；列表项错误携带 `itemIndex`，使前端可以精确定位错误。

## 8. 插件文件权限边界

前端不能提交任意保存路径。Rust 侧必须：

- 只允许写入 `%APPDATA%\\which-key-windows\\plugins\\`。
- 从经过校验的插件 ID 生成文件名。
- 拒绝 `..`、路径分隔符和 Windows 保留设备名。
- 不允许修改 `plugins/builtin`。
- “打开文件”只能打开 PluginLoader 已发现且规范化后的路径。
- 新建插件前必须通过正式插件解析器和 schema 校验。

## 9. 测试与验收

纯逻辑自动化测试覆盖：主题字段约束、默认值和前端 reset、JSON 协议、资源级 revision 冲突、候选快照失败时文件不变、成功保存后的单次快照交换、插件 ID 路径安全、插件校验和启停后的 resolver 结果。

WebView2 集成测试覆盖：合法/非法 Web Message、结构化错误、用户内容不进入动态 JavaScript、拒绝外部导航和新窗口、Release 不暴露 DevTools、保存成功后的 overlay 通知。

Windows 手工验收覆盖：主题预览与保存、外部修改冲突、创建和禁用用户插件、非法路径拒绝、损坏插件隔离、Dirty 关闭确认和 WebView2 初始化失败提示。

## 10. 非目标

第一版不包含在线插件市场、自动下载、脚本或 DLL 插件、应用内部模式检测、鼠标快捷键、Web Office、快捷键自动执行，以及全局快捷键可视化 CRUD。
