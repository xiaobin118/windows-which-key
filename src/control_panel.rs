//! 控制面板：独立的常规窗口（可激活、带标题栏），内嵌 WebView2
//! 展示 `control_panel.html`。只负责窗口生命周期与消息转发，业务
//! 动作（如重载配置）通过 `PanelCommand` 通道交回主循环执行。

use crate::plugin::{parse_plugin_toml, PluginOrigin, BUILTIN_PLUGINS};
use crate::theme::ThemeConfig;
use crate::webview_bridge::WebView2Bridge;
use anyhow::{Context, Result};
use std::collections::HashSet;
use std::path::Path;
use std::sync::mpsc::Sender;
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::{AdjustWindowRectExForDpi, GetDpiForWindow};
use windows::Win32::UI::WindowsAndMessaging::*;

pub const CONTROL_PANEL_HTML: &str = include_str!("control_panel.html");

const PANEL_WIDTH: i32 = 780;
const PANEL_HEIGHT: i32 = 560;

/// 控制面板请求主循环执行的动作。
#[derive(Debug, Clone, PartialEq)]
pub enum PanelCommand {
    ReloadConfig,
    SetTheme(ThemeConfig),
    /// 编辑过程中实时预览主题（只改浮层，不写文件）。
    PreviewTheme(ThemeConfig),
    /// 在记事本中打开全局配置文件。
    OpenConfig,
    /// 在资源管理器中打开用户插件目录。
    OpenPluginDir,
    /// 页面加载完成，请求推送完整状态（解决 NavigateToString 异步竞态）。
    RequestState,
}

/// 窗口过程需要访问 HWND 来拦截 WM_CLOSE（隐藏而非销毁）。
static PANEL_HWND: std::sync::Mutex<Option<isize>> = std::sync::Mutex::new(None);

pub struct ControlPanel {
    hwnd: HWND,
    bridge: WebView2Bridge,
}

unsafe extern "system" fn panel_window_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if msg == WM_CLOSE {
        // 关闭按钮只隐藏窗口，保留 WebView 实例以便快速再次打开。
        let _ = ShowWindow(hwnd, SW_HIDE);
        return LRESULT(0);
    }
    DefWindowProcW(hwnd, msg, wparam, lparam)
}

impl ControlPanel {
    /// 创建窗口与 WebView，并把页面消息（JSON 字符串）转发到通道，
    /// 由主循环解析成 `PanelCommand` 执行。首次调用较慢（初始化
    /// WebView2），建议按需调用而不是随程序启动。
    pub fn open(message_sender: Sender<String>) -> Result<Self> {
        unsafe {
            let instance = GetModuleHandleW(None).context("获取模块句柄失败")?;
            let instance = HINSTANCE(instance.0);
            let class_name = windows::core::w!("WhichKeyControlPanel");

            let wc = WNDCLASSEXW {
                cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
                style: CS_HREDRAW | CS_VREDRAW,
                lpfnWndProc: Some(panel_window_proc),
                hInstance: instance,
                lpszClassName: class_name,
                ..Default::default()
            };
            if RegisterClassExW(&wc) == 0 {
                let err = windows::Win32::Foundation::GetLastError();
                // 重复注册（第二次打开）不算失败
                if err.0 != 1410 {
                    anyhow::bail!("注册控制面板窗口类失败: {:?}", err);
                }
            }

            // 固定尺寸：去掉 WS_THICKFRAME / WS_MAXIMIZEBOX。PANEL_WIDTH/HEIGHT
            // 是 HTML 内容区尺寸，不是含标题栏和边框的外窗尺寸。
            let style = WS_OVERLAPPEDWINDOW & !(WS_THICKFRAME | WS_MAXIMIZEBOX);
            let ex_style = WINDOW_EX_STYLE::default();
            let hwnd = CreateWindowExW(
                ex_style,
                class_name,
                windows::core::w!("Which-Key 控制面板"),
                style,
                100,
                100,
                PANEL_WIDTH,
                PANEL_HEIGHT,
                None,
                None,
                instance,
                None,
            )
            .context("创建控制面板窗口失败")?;

            let factor = crate::window_manager::dpi_factor_for(hwnd);
            log::info!("控制面板 DPI 因子: {factor}");
            let client_width = (PANEL_WIDTH as f64 * factor).round() as i32;
            let client_height = (PANEL_HEIGHT as f64 * factor).round() as i32;
            let dpi = GetDpiForWindow(hwnd);
            let mut window_rect = RECT {
                left: 0,
                top: 0,
                right: client_width,
                bottom: client_height,
            };
            AdjustWindowRectExForDpi(&mut window_rect, style, false, ex_style, dpi)
                .context("计算控制面板窗口尺寸失败")?;
            let width = window_rect.right - window_rect.left;
            let height = window_rect.bottom - window_rect.top;
            let (screen_w, screen_h) =
                (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN));
            let pos_x = ((screen_w - width) / 2).max(0);
            let pos_y = ((screen_h - height) / 2).max(0);
            SetWindowPos(
                hwnd,
                None,
                pos_x,
                pos_y,
                width,
                height,
                SWP_NOZORDER | SWP_NOACTIVATE,
            )
            .context("调整控制面板窗口失败")?;

            let mut client_rect = RECT::default();
            GetClientRect(hwnd, &mut client_rect).context("读取控制面板内容区尺寸失败")?;
            let bridge = WebView2Bridge::new(
                hwnd,
                client_rect.right - client_rect.left,
                client_rect.bottom - client_rect.top,
            )
            .context("控制面板 WebView2 初始化失败")?;
            bridge.forward_web_messages(message_sender)?;
            bridge.load_html(CONTROL_PANEL_HTML)?;

            *PANEL_HWND.lock().unwrap() = Some(hwnd.0 as isize);

            let panel = ControlPanel { hwnd, bridge };
            panel.show()?;
            Ok(panel)
        }
    }

    pub fn hwnd(&self) -> HWND {
        self.hwnd
    }

    pub fn show(&self) -> Result<()> {
        unsafe {
            // SW_SHOW 不会还原最小化的窗口（会一直缩在任务栏），
            // 先检查 IsIconic 再决定用 SW_RESTORE
            if IsIconic(self.hwnd).as_bool() {
                let _ = ShowWindow(self.hwnd, SW_RESTORE);
            } else {
                let _ = ShowWindow(self.hwnd, SW_SHOW);
            }
            let _ = SetForegroundWindow(self.hwnd);
        }
        Ok(())
    }

    /// 推送完整状态（版本、主题、插件清单、路径）给页面。
    pub fn send_state(&self, theme: &ThemeConfig) -> Result<()> {
        let version = env!("CARGO_PKG_VERSION");
        let config_path = global_config_path()?;
        let plugin_dir = config_path
            .parent()
            .context("配置路径缺少父目录")?
            .join("plugins");
        let state = build_state_json(version, &config_path, &plugin_dir, theme);
        self.bridge
            .execute_script(&format!("window.postMessage({state}, '*');"))
    }

    /// 通知页面一次动作已完成（当前仅重载配置）。
    pub fn notify_reload_done(&self) -> Result<()> {
        self.bridge
            .execute_script("window.postMessage({\"type\":\"reloadDone\"}, '*');")
    }

    /// 通知页面主题已保存并生效。
    pub fn notify_theme_saved(&self) -> Result<()> {
        self.bridge
            .execute_script("window.postMessage({\"type\":\"themeSaved\"}, '*');")
    }
}

impl Drop for ControlPanel {
    fn drop(&mut self) {
        *PANEL_HWND.lock().unwrap() = None;
    }
}

/// 把 JSON 字符串解析成 PanelCommand；无法识别的消息记日志后忽略。
pub fn parse_panel_message(raw: &str) -> Option<PanelCommand> {
    let mut value: serde_json::Value = match serde_json::from_str(raw) {
        Ok(value) => value,
        Err(error) => {
            log::warn!("控制面板消息解析失败: {error}");
            return None;
        }
    };
    if let Some(inner) = value.as_str() {
        value = match serde_json::from_str(inner) {
            Ok(value) => value,
            Err(error) => {
                log::warn!("控制面板嵌套消息解析失败: {error}");
                return None;
            }
        };
    }
    match value.get("type").and_then(|t| t.as_str()) {
        Some("reload") => Some(PanelCommand::ReloadConfig),
        Some("ready") => Some(PanelCommand::RequestState),
        Some("openConfig") => Some(PanelCommand::OpenConfig),
        Some("openPluginDir") => Some(PanelCommand::OpenPluginDir),
        Some(kind @ ("setTheme" | "previewTheme")) => {
            match serde_json::from_value::<ThemeConfig>(
                value
                    .get("theme")
                    .cloned()
                    .unwrap_or(serde_json::Value::Null),
            ) {
                Ok(theme) => Some(if kind == "setTheme" {
                    PanelCommand::SetTheme(theme)
                } else {
                    PanelCommand::PreviewTheme(theme)
                }),
                Err(error) => {
                    log::warn!("控制面板主题数据不合法: {error}");
                    None
                }
            }
        }
        _ => {
            log::debug!("忽略未知控制面板消息: {raw}");
            None
        }
    }
}

/// 用 toml_edit 把主题写回配置文本：只替换/追加 `[theme]` 表，
/// 其余内容（包括注释和顺序）原样保留。
pub fn write_theme_to_toml(source: &str, theme: &ThemeConfig) -> Result<String> {
    let mut document = source
        .parse::<toml_edit::DocumentMut>()
        .context("全局配置 TOML 解析失败")?;

    let item = document.entry("theme").or_insert(toml_edit::Item::Table({
        let mut table = toml_edit::Table::new();
        table.set_implicit(false);
        table
    }));
    let table = item
        .as_table_mut()
        .context("配置中的 [theme] 不是 TOML 表")?;

    table["background"] = toml_edit::value(theme.background.as_str());
    table["background_opacity"] = toml_edit::value(theme.background_opacity);
    table["border"] = toml_edit::value(theme.border.as_str());
    table["border_opacity"] = toml_edit::value(theme.border_opacity);
    table["accent"] = toml_edit::value(theme.accent.as_str());
    table["text_primary"] = toml_edit::value(theme.text_primary.as_str());
    table["text_secondary"] = toml_edit::value(theme.text_secondary.as_str());
    table["radius"] = toml_edit::value(theme.radius as i64);
    table["blur"] = toml_edit::value(theme.blur as i64);
    table["row_spacing"] = toml_edit::value(theme.row_spacing as i64);

    Ok(document.to_string())
}

/// 构建推送给页面的状态 JSON。
pub fn build_state_json(
    version: &str,
    config_path: &Path,
    plugin_dir: &Path,
    theme: &ThemeConfig,
) -> serde_json::Value {
    let user_plugins = read_user_plugins(plugin_dir);
    let disabled_builtin_ids = user_plugins
        .iter()
        .filter(|plugin| plugin["disabled"].as_bool().unwrap_or(false))
        .filter_map(|plugin| plugin["id"].as_str())
        .map(ToOwned::to_owned)
        .collect::<HashSet<_>>();
    let built_in = BUILTIN_PLUGINS
        .iter()
        .filter_map(|(_, source)| parse_plugin_toml(source, PluginOrigin::BuiltIn).ok())
        .map(|plugin| {
            let disabled = plugin.disabled || disabled_builtin_ids.contains(&plugin.id);
            serde_json::json!({
                "id": plugin.id,
                "name": plugin.name,
                "processes": plugin.processes,
                "bindings": plugin.bindings.len(),
                "disabled": disabled,
                "origin": "builtIn",
            })
        })
        .collect::<Vec<_>>();

    serde_json::json!({
        "type": "state",
        "version": version,
        "configPath": config_path.to_string_lossy(),
        "pluginDir": plugin_dir.to_string_lossy(),
        "theme": theme,
        "plugins": {
            "builtIn": built_in,
            "user": user_plugins,
            "dir": plugin_dir.to_string_lossy(),
        },
    })
}

fn read_user_plugins(plugin_dir: &Path) -> Vec<serde_json::Value> {
    let mut paths = plugin_dir
        .read_dir()
        .map(|entries| {
            entries
                .filter_map(|entry| entry.ok())
                .map(|entry| entry.path())
                .filter(|path| {
                    path.is_file()
                        && path
                            .extension()
                            .and_then(|extension| extension.to_str())
                            .is_some_and(|extension| extension.eq_ignore_ascii_case("toml"))
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    paths.sort_by(|left, right| {
        left.file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_ascii_lowercase()
            .cmp(
                &right
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_ascii_lowercase(),
            )
            .then_with(|| left.cmp(right))
    });

    paths
        .into_iter()
        .map(|path| {
            let file = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            match std::fs::read_to_string(&path)
                .with_context(|| format!("读取用户插件失败: {}", path.display()))
                .and_then(|source| parse_plugin_toml(&source, PluginOrigin::User(path.clone())))
            {
                Ok(plugin) => serde_json::json!({
                    "file": file,
                    "id": plugin.id,
                    "name": plugin.name,
                    "processes": plugin.processes,
                    "bindings": plugin.bindings.len(),
                    "disabled": plugin.disabled,
                    "origin": "user",
                    "valid": true,
                }),
                Err(error) => serde_json::json!({
                    "file": file,
                    "id": file.trim_end_matches(".toml"),
                    "name": file,
                    "processes": [],
                    "bindings": 0,
                    "disabled": false,
                    "origin": "user",
                    "valid": false,
                    "error": error.to_string(),
                }),
            }
        })
        .collect()
}

fn global_config_path() -> Result<std::path::PathBuf> {
    let app_data = std::env::var_os("APPDATA").context("APPDATA 未设置")?;
    Ok(std::path::PathBuf::from(app_data)
        .join("which-key-windows")
        .join("which-key.toml"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_includes_version_theme_and_builtin_plugins() {
        let temp = tempfile::tempdir().unwrap();
        let config_path = temp.path().join("which-key.toml");
        let plugin_dir = temp.path().join("plugins");
        std::fs::create_dir_all(&plugin_dir).unwrap();

        let theme = ThemeConfig::default_theme();
        let state = build_state_json("9.9.9", &config_path, &plugin_dir, &theme);

        assert_eq!(state["version"], "9.9.9");
        assert_eq!(state["theme"]["background"], "#0B0F1F");
        let built_in = state["plugins"]["builtIn"].as_array().unwrap();
        assert!(built_in.len() >= 4);
        assert!(built_in.iter().any(|p| p["id"] == "vscode"));
        assert_eq!(state["plugins"]["user"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn user_plugin_files_are_listed() {
        let temp = tempfile::tempdir().unwrap();
        let plugin_dir = temp.path().join("plugins");
        std::fs::create_dir_all(&plugin_dir).unwrap();
        std::fs::write(plugin_dir.join("my-tool.toml"), "# plugin").unwrap();
        std::fs::write(plugin_dir.join("notes.txt"), "ignore me").unwrap();

        let state = build_state_json(
            "0.0.0",
            &temp.path().join("c.toml"),
            &plugin_dir,
            &ThemeConfig::default_theme(),
        );

        let user = state["plugins"]["user"].as_array().unwrap();
        assert_eq!(user.len(), 1);
        assert_eq!(user[0]["file"], "my-tool.toml");
        assert_eq!(user[0]["valid"], false);
    }

    #[test]
    fn disabled_user_plugin_marks_builtin_disabled() {
        let temp = tempfile::tempdir().unwrap();
        let plugin_dir = temp.path().join("plugins");
        std::fs::create_dir_all(&plugin_dir).unwrap();
        std::fs::write(
            plugin_dir.join("vscode.toml"),
            r#"
schema_version = 1
id = "vscode"
name = "Visual Studio Code"
processes = ["Code.exe"]
disabled = true
"#,
        )
        .unwrap();

        let state = build_state_json(
            "0.0.0",
            &temp.path().join("c.toml"),
            &plugin_dir,
            &ThemeConfig::default_theme(),
        );

        let built_in = state["plugins"]["builtIn"].as_array().unwrap();
        let vscode = built_in
            .iter()
            .find(|plugin| plugin["id"] == "vscode")
            .unwrap();
        assert_eq!(vscode["disabled"], true);
    }

    #[test]
    fn panel_messages_map_to_commands() {
        assert_eq!(
            parse_panel_message(r#"{"type":"reload"}"#),
            Some(PanelCommand::ReloadConfig)
        );
        assert_eq!(
            parse_panel_message(r#"{"type":"ready"}"#),
            Some(PanelCommand::RequestState)
        );
        assert_eq!(parse_panel_message(r#"{"type":"unknown"}"#), None);
        assert_eq!(parse_panel_message("not json"), None);
        assert_eq!(
            parse_panel_message(r#""{\"type\":\"reload\"}""#),
            Some(PanelCommand::ReloadConfig)
        );
        assert_eq!(
            parse_panel_message(r#"{"type":"openConfig"}"#),
            Some(PanelCommand::OpenConfig)
        );
    }

    #[test]
    fn set_theme_message_carries_the_theme_fields() {
        let raw = r##"{"type":"setTheme","theme":{
            "background":"#101010","background_opacity":0.9,
            "border":"#FFFFFF","border_opacity":0.3,
            "accent":"#4488FF","text_primary":"#EEEEEE","text_secondary":"#CCCCCC",
            "radius":12,"blur":16,"row_spacing":8}}"##;

        match parse_panel_message(raw) {
            Some(PanelCommand::SetTheme(theme)) => {
                assert_eq!(theme.background, "#101010");
                assert_eq!(theme.radius, 12);
                assert_eq!(theme.row_spacing, 8);
                assert!(theme.validate().is_ok());
            }
            other => panic!("expected SetTheme, got {other:?}"),
        }
    }

    #[test]
    fn writing_theme_preserves_other_sections_and_comments() {
        let source = r##"# 我的重要注释
[globals]
"C-c" = { desc = "复制" }

[theme]
background = "#0B0F1F"
"##;
        let mut theme = ThemeConfig::default_theme();
        theme.accent = "#4488FF".to_string();

        let updated = write_theme_to_toml(source, &theme).unwrap();

        assert!(updated.contains("# 我的重要注释"));
        assert!(updated.contains("\"C-c\" = { desc = \"复制\" }"));
        assert!(updated.contains("accent = \"#4488FF\""));
        // 写回后能被配置服务重新解析
        let parsed = crate::config::parse_theme(&updated).unwrap();
        assert_eq!(parsed.accent, "#4488FF");
    }

    #[test]
    fn writing_theme_appends_a_missing_section() {
        let source = "[globals]\n\"C-c\" = { desc = \"复制\" }\n";
        let updated = write_theme_to_toml(source, &ThemeConfig::default_theme()).unwrap();

        assert!(updated.contains("[theme]"));
        assert!(updated.contains("background = \"#0B0F1F\""));
    }
}
