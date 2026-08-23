use anyhow::{Context, Result};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::Duration;
use which_key_windows::foreground_app::ForegroundAppProvider;
use which_key_windows::*;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::Dialogs::{GetOpenFileNameW, GetSaveFileNameW, OPENFILENAMEW, OFN_EXPLORER, OFN_HIDEREADONLY, OFN_PATHMUSTEXIST};
use windows::Win32::UI::WindowsAndMessaging::*;

const WM_TRAY_CALLBACK: u32 = WM_USER + 1;

fn main() -> Result<()> {
    // 必须在创建任何窗口之前声明 DPI 感知，否则高缩放屏幕上
    // 整个窗口（含 WebView 文字）会被系统位图拉伸而发虚。
    let dpi_awareness = unsafe {
        windows::Win32::UI::HiDpi::SetProcessDpiAwarenessContext(
            windows::Win32::UI::HiDpi::DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
        )
    };

    env_logger::init();
    log::info!("Which-Key Windows 启动中...");
    log::info!("SetProcessDpiAwarenessContext(PMv2): {:?}", dpi_awareness);

    // ── 加载配置 ──
    let config_path = global_config_path()?;
    let config = if config_path.exists() {
        config::Config::load(config_path.clone())?
    } else {
        std::fs::create_dir_all(config_path.parent().expect("config path has parent"))?;
        create_default_config(&config_path)?;
        config::Config::load(config_path.clone())?
    };
    log::info!("配置加载完成: {}", config_path.display());

    // 注册表是纯数据树，可共享给状态机
    let source = std::fs::read_to_string(&config_path)?;
    let plugin_dir = config_path
        .parent()
        .expect("config path has parent")
        .join("plugins");
    let (configuration, warnings) =
        config::ConfigurationService::from_sources(&source, plugin::BUILTIN_PLUGINS, &plugin_dir)?;
    for warning in warnings {
        log::warn!(
            "跳过用户插件 {}: {}",
            warning.path.display(),
            warning.message
        );
    }
    let registry = Arc::new(config.registry.clone());

    // ── 创建键盘事件通道 ──
    let (tx, rx) = mpsc::channel::<types::KeyEvent>();

    // ── 安装键盘钩子 ──
    let hook = hook::KeyboardHook::new(tx).context("键盘钩子创建失败")?;
    hook.install().context("键盘钩子安装失败")?;
    log::info!("键盘钩子已安装");

    // ── 初始化核心模块 ──
    let mut state_machine = state_machine::StateMachine::new(registry.clone());
    let foreground = foreground_app::Win32ForegroundAppProvider;
    let mut show_all_process: Option<String> = None;
    let mut overlay = overlay_controller::OverlayController::new().context("覆盖层初始化失败")?;
    log::info!("覆盖层已初始化");
    overlay.execute(types::UiCommand::ApplyTheme {
        theme: configuration.current().theme.clone(),
    })?;

    // ── 创建托盘图标用的消息窗口 ──
    let tray_hwnd = create_message_window()?;
    let tray = tray_icon::TrayIcon::new(tray_hwnd)?;
    tray.show().context("托盘图标创建失败")?;
    log::info!("托盘图标已创建");

    // ── 控制面板（按需创建，首次从托盘打开时初始化 WebView2） ──
    let (panel_tx, panel_rx) = mpsc::channel::<String>();
    let mut control_panel: Option<control_panel::ControlPanel> = None;

    log::info!("就绪。按住 Ctrl 约 300ms 显示快捷键提示。");

    // ── 主事件循环 ──
    let mut running = true;
    let mut tick_accumulator = Duration::ZERO;
    let tick_interval = Duration::from_millis(10);

    while running {
        let loop_start = std::time::Instant::now();

        // 1. 处理键盘事件
        while let Ok(event) = rx.try_recv() {
            if let types::KeyEvent::ToggleControlPanel = event {
                open_control_panel(&panel_tx, &mut control_panel);
                continue;
            }
            if matches!(event, types::KeyEvent::ToggleShowAll) && !show_all_is_open(&state_machine)
            {
                let process = match capture_show_all_process(foreground.foreground_executable()) {
                    Ok(process) => process,
                    Err(error) => {
                        log::warn!("拒绝打开 show-all: {error:#}");
                        continue;
                    }
                };
                let resolved =
                    keymap_resolver::KeymapResolver::from_snapshot(&configuration.current())
                        .resolve(Some(&process));
                state_machine.replace_registry(resolved.registry, resolved.app_name);
                show_all_process = Some(process);
            }
            if matches!(event, types::KeyEvent::ModifierDown(_)) {
                let process = foreground.foreground_executable().unwrap_or_else(|error| {
                    log::warn!("读取前台进程失败: {error:#}");
                    None
                });
                let resolved =
                    keymap_resolver::KeymapResolver::from_snapshot(&configuration.current())
                        .resolve(process.as_deref());
                state_machine.replace_registry(resolved.registry, resolved.app_name);
            }
            if let Some(cmd) = state_machine.handle_event(event) {
                overlay.execute(cmd)?;
                hook.set_show_all_open(show_all_is_open(&state_machine));
            }
        }

        // 2. 检查状态机定时器
        if let Some(cmd) = state_machine.tick() {
            overlay.execute(cmd)?;
            hook.set_show_all_open(show_all_is_open(&state_machine));
        }

        // 2.5 处理控制面板的请求（就绪推送状态 / 重载配置 / 保存主题）
        while let Ok(raw) = panel_rx.try_recv() {
            match control_panel::parse_panel_message(&raw) {
                Some(control_panel::PanelCommand::RequestState) => {
                    if let Some(panel) = control_panel.as_ref() {
                        let _ = panel.send_state(&configuration.current().theme);
                    }
                }
                Some(control_panel::PanelCommand::ReloadConfig) => {
                    match reload_configuration(
                        &configuration,
                        &config_path,
                        &plugin_dir,
                        &mut state_machine,
                        &mut overlay,
                        &hook,
                    ) {
                        Ok(()) => {
                            if let Some(panel) = control_panel.as_ref() {
                                let _ = panel.notify_reload_done();
                            }
                        }
                        Err(error) => log::warn!("控制面板触发的重载失败: {error:#}"),
                    }
                }
                Some(control_panel::PanelCommand::PreviewTheme(theme)) => {
                    // 实时预览：只应用到浮层，不写配置文件
                    overlay.execute(types::UiCommand::ApplyTheme { theme })?;
                }
                Some(control_panel::PanelCommand::ToggleAutostart(enabled)) => {
                    autostart::set_enabled(enabled, &std::env::current_exe().context("获取当前可执行文件失败")?)?;
                    if let Some(panel) = control_panel.as_ref() {
                        let _ = panel.send_state(&configuration.current().theme);
                    }
                }
                Some(control_panel::PanelCommand::OpenPluginDir) => {
                    let plugin_dir = config_path
                        .parent()
                        .context("配置路径缺少父目录")?
                        .join("plugins");
                    std::fs::create_dir_all(&plugin_dir).ok();
                    std::process::Command::new("explorer.exe")
                        .arg(&plugin_dir)
                        .spawn()
                        .context("打开插件目录失败")?;
                }
                Some(control_panel::PanelCommand::CreatePluginTemplate) => {
                    let plugin_dir = config_path
                        .parent()
                        .context("配置路径缺少父目录")?
                        .join("plugins");
                    std::fs::create_dir_all(&plugin_dir).ok();
                    let plugin_path = next_plugin_template_path(&plugin_dir)?;
                    std::fs::write(&plugin_path, control_panel::default_plugin_template())
                        .with_context(|| format!("写入插件模板失败: {}", plugin_path.display()))?;
                    std::process::Command::new("notepad.exe")
                        .arg(&plugin_path)
                        .spawn()
                        .with_context(|| format!("打开插件模板失败: {}", plugin_path.display()))?;
                    if let Some(panel) = control_panel.as_ref() {
                        let _ = panel.send_state(&configuration.current().theme);
                    }
                }
                Some(control_panel::PanelCommand::OpenPluginFile(file)) => {
                    let plugin_dir = config_path
                        .parent()
                        .context("配置路径缺少父目录")?
                        .join("plugins");
                    let plugin_path = plugin_dir.join(&file);
                    if !plugin_path.starts_with(&plugin_dir) {
                        anyhow::bail!("插件文件路径非法");
                    }
                    if !plugin_path.exists() {
                        anyhow::bail!("插件文件不存在: {}", plugin_path.display());
                    }
                    std::process::Command::new("notepad.exe")
                        .arg(&plugin_path)
                        .spawn()
                        .with_context(|| format!("打开插件文件失败: {}", plugin_path.display()))?;
                }
                Some(control_panel::PanelCommand::ExportPlugins) => {
                    let plugin_dir = config_path
                        .parent()
                        .context("配置路径缺少父目录")?
                        .join("plugins");
                    let bundle = plugin_bundle::export_user_plugin_bundle(&plugin_dir)?;
                    if let Some(path) = pick_bundle_save_path()? {
                        std::fs::write(&path, bundle)
                            .with_context(|| format!("写出插件包失败: {}", path.display()))?;
                        log::info!("已导出插件包: {}", path.display());
                    }
                }
                Some(control_panel::PanelCommand::ImportPlugins) => {
                    if let Some(path) = pick_bundle_open_path()? {
                        let plugin_dir = config_path
                            .parent()
                            .context("配置路径缺少父目录")?
                            .join("plugins");
                        let bundle = std::fs::read_to_string(&path)
                            .with_context(|| format!("读取插件包失败: {}", path.display()))?;
                        let count = plugin_bundle::import_user_plugin_bundle(&bundle, &plugin_dir)?;
                        log::info!("已导入插件包，写入 {} 个文件", count);
                        reload_configuration(
                            &configuration,
                            &config_path,
                            &plugin_dir,
                            &mut state_machine,
                            &mut overlay,
                            &hook,
                        )?;
                        if let Some(panel) = control_panel.as_ref() {
                            let _ = panel.notify_reload_done();
                            let _ = panel.send_state(&configuration.current().theme);
                        }
                    }
                }
                Some(control_panel::PanelCommand::OpenConfig) => {
                    std::process::Command::new("notepad.exe")
                        .arg(&config_path)
                        .spawn()
                        .context("打开全局配置失败")?;
                }
                Some(control_panel::PanelCommand::OpenConfigDir) => {
                    let config_dir = config_path
                        .parent()
                        .context("配置路径缺少父目录")?;
                    std::process::Command::new("explorer.exe")
                        .arg(config_dir)
                        .spawn()
                        .context("打开配置目录失败")?;
                }
                Some(control_panel::PanelCommand::SetTheme(theme)) => {
                    match apply_theme_configuration(
                        &config_path,
                        &theme,
                        &configuration,
                        &plugin_dir,
                        &mut state_machine,
                        &mut overlay,
                        &hook,
                    ) {
                        Ok(()) => {
                            if let Some(panel) = control_panel.as_ref() {
                                let _ = panel.notify_theme_saved();
                                let _ = panel.send_state(&configuration.current().theme);
                            }
                        }
                        Err(error) => log::warn!("保存主题失败: {error:#}"),
                    }
                }
                None => {}
            }
        }

        if show_all_is_open(&state_machine) {
            match foreground.foreground_executable() {
                Ok(process) if process == show_all_process => {}
                Ok(_) => {
                    if let Some(cmd) = state_machine.handle_event(types::KeyEvent::ToggleShowAll) {
                        overlay.execute(cmd)?;
                    }
                    show_all_process = None;
                    hook.set_show_all_open(false);
                }
                Err(error) => {
                    log::warn!("前台进程读取失败，关闭 show-all: {error:#}");
                    if let Some(cmd) = state_machine.handle_event(types::KeyEvent::ToggleShowAll) {
                        overlay.execute(cmd)?;
                    }
                    show_all_process = None;
                    hook.set_show_all_open(false);
                }
            }
        }

        // 3. 处理 Windows 消息（托盘图标）
        running = pump_messages(
            &tray_hwnd,
            &tray,
            running,
            &configuration,
            &config_path,
            &plugin_dir,
            &mut state_machine,
            &mut overlay,
            &hook,
            &panel_tx,
            &mut control_panel,
        )?;

        // 4. 节流，避免忙等待
        let elapsed = loop_start.elapsed();
        if elapsed < tick_interval {
            std::thread::sleep(tick_interval - elapsed);
        }
        tick_accumulator += loop_start.elapsed();
        if tick_accumulator >= Duration::from_secs(1) {
            tick_accumulator = Duration::ZERO;
            // 周期日志，确认事件循环活着
        }
    }

    // ── 清理 ──
    hook.uninstall()?;
    log::info!("Which-Key Windows 已退出");
    Ok(())
}

/// 处理托盘图标的窗口消息，返回是否需要继续运行
#[allow(clippy::too_many_arguments)]
fn pump_messages(
    tray_hwnd: &HWND,
    tray: &tray_icon::TrayIcon,
    mut running: bool,
    configuration: &config::ConfigurationService,
    config_path: &std::path::Path,
    plugin_dir: &std::path::Path,
    state_machine: &mut state_machine::StateMachine,
    overlay: &mut overlay_controller::OverlayController,
    hook: &hook::KeyboardHook,
    panel_tx: &mpsc::Sender<String>,
    control_panel: &mut Option<control_panel::ControlPanel>,
) -> Result<bool> {
    unsafe {
        let mut msg = MSG::default();
        while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).into() {
            // 托盘图标消息
            if msg.hwnd == *tray_hwnd && msg.message == WM_TRAY_CALLBACK {
                if let Some(cmd) = tray.handle_message(msg.message, msg.lParam) {
                    match cmd {
                        tray_icon::TrayCommand::Quit => running = false,
                        tray_icon::TrayCommand::ReloadConfig => {
                            if let Err(error) = reload_configuration(
                                configuration,
                                config_path,
                                plugin_dir,
                                state_machine,
                                overlay,
                                hook,
                            ) {
                                log::warn!("配置重载失败，继续使用旧快照: {error:#}");
                            }
                        }
                        tray_icon::TrayCommand::OpenConfig => {
                            std::process::Command::new("notepad.exe")
                                .arg(config_path)
                                .spawn()
                                .context("打开全局配置失败")?;
                        }
                        tray_icon::TrayCommand::OpenConfigDir => {
                            let config_dir = config_path
                                .parent()
                                .context("配置路径缺少父目录")?;
                            std::process::Command::new("explorer.exe")
                                .arg(config_dir)
                                .spawn()
                                .context("打开配置目录失败")?;
                        }
                        tray_icon::TrayCommand::OpenPluginDir => {
                            let plugin_dir = config_path
                                .parent()
                                .context("配置路径缺少父目录")?
                                .join("plugins");
                            std::fs::create_dir_all(&plugin_dir).ok();
                            std::process::Command::new("explorer.exe")
                                .arg(&plugin_dir)
                                .spawn()
                                .context("打开插件目录失败")?;
                        }
                        tray_icon::TrayCommand::OpenControlPanel => {
                            open_control_panel(panel_tx, control_panel);
                        }
                    }
                }
            } else {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    }
    Ok(running)
}

/// 打开（或重新显示）控制面板。首次打开会初始化 WebView2，较慢。
fn open_control_panel(
    panel_tx: &mpsc::Sender<String>,
    control_panel: &mut Option<control_panel::ControlPanel>,
) {
    if control_panel.is_none() {
        match control_panel::ControlPanel::open(panel_tx.clone()) {
            Ok(panel) => *control_panel = Some(panel),
            Err(error) => log::error!("控制面板打开失败: {error:#}"),
        }
    } else if let Some(panel) = control_panel.as_ref() {
        let _ = panel.show();
    }
}

/// 从磁盘重新读取配置并原子替换运行时快照；成功时收起当前的
/// 浮层/浏览状态，失败时保留旧快照仅记录日志。
#[allow(clippy::too_many_arguments)]
fn reload_configuration(
    configuration: &config::ConfigurationService,
    config_path: &std::path::Path,
    plugin_dir: &std::path::Path,
    state_machine: &mut state_machine::StateMachine,
    overlay: &mut overlay_controller::OverlayController,
    hook: &hook::KeyboardHook,
) -> Result<()> {
    let source = std::fs::read_to_string(config_path).context("读取全局配置失败")?;
    apply_configuration(
        &source,
        configuration,
        config_path,
        plugin_dir,
        state_machine,
        overlay,
        hook,
    )
}

/// 重载配置并把（可能变化的）主题推送到浮层。写入文件由调用方
/// 完成或无需写入（普通重载）。
#[allow(clippy::too_many_arguments)]
fn apply_configuration(
    source: &str,
    configuration: &config::ConfigurationService,
    _config_path: &std::path::Path,
    plugin_dir: &std::path::Path,
    state_machine: &mut state_machine::StateMachine,
    overlay: &mut overlay_controller::OverlayController,
    hook: &hook::KeyboardHook,
) -> Result<()> {
    let warnings = configuration.reload(source, plugin::BUILTIN_PLUGINS, plugin_dir)?;
    for warning in warnings {
        log::warn!(
            "跳过用户插件 {}: {}",
            warning.path.display(),
            warning.message
        );
    }
    if let Some(command) = dismiss_after_successful_reload(state_machine, hook) {
        overlay.execute(command)?;
    }
    overlay.execute(types::UiCommand::ApplyTheme {
        theme: configuration.current().theme.clone(),
    })?;
    Ok(())
}

/// 把新主题写进配置文件（保留其他段落与注释），随后走一次完整的
/// 配置重载，让主题与其他配置一起原子生效。
#[allow(clippy::too_many_arguments)]
fn apply_theme_configuration(
    config_path: &std::path::Path,
    theme: &theme::ThemeConfig,
    configuration: &config::ConfigurationService,
    plugin_dir: &std::path::Path,
    state_machine: &mut state_machine::StateMachine,
    overlay: &mut overlay_controller::OverlayController,
    hook: &hook::KeyboardHook,
) -> Result<()> {
    let source = std::fs::read_to_string(config_path).context("读取全局配置失败")?;
    let updated = control_panel::write_theme_to_toml(&source, theme)?;
    std::fs::write(config_path, &updated).context("写回全局配置失败")?;
    apply_configuration(
        &updated,
        configuration,
        config_path,
        plugin_dir,
        state_machine,
        overlay,
        hook,
    )
}

fn global_config_path() -> Result<std::path::PathBuf> {
    let app_data = std::env::var_os("APPDATA").context("APPDATA 未设置")?;
    Ok(std::path::PathBuf::from(app_data)
        .join("which-key-windows")
        .join("which-key.toml"))
}

fn show_all_is_open(state_machine: &state_machine::StateMachine) -> bool {
    matches!(state_machine.state, state_machine::State::BrowsingAll)
}

fn capture_show_all_process(process: Result<Option<String>>) -> Result<String> {
    process?.context("未找到前台进程")
}

fn dismiss_after_successful_reload(
    state_machine: &mut state_machine::StateMachine,
    hook: &hook::KeyboardHook,
) -> Option<types::UiCommand> {
    let command = state_machine.dismiss();
    hook.set_show_all_open(false);
    command
}

unsafe extern "system" fn tray_window_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    DefWindowProcW(hwnd, msg, wparam, lparam)
}

/// 创建 message-only 窗口，用于接收托盘图标消息
fn create_message_window() -> Result<HWND> {
    use windows::Win32::Foundation::HINSTANCE;

    unsafe {
        let instance = GetModuleHandleW(None).context("获取模块句柄失败")?;
        let instance = HINSTANCE(instance.0);
        let class_name = windows::core::w!("WhichKeyTrayWindow");

        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(tray_window_proc),
            hInstance: instance,
            lpszClassName: class_name,
            ..Default::default()
        };
        RegisterClassExW(&wc);

        // HWND_MESSAGE = (HWND)-3, message-only window
        let message_hwnd = HWND(-3_isize as *mut _);
        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            class_name,
            windows::core::w!("Which-Key Tray"),
            WINDOW_STYLE::default(),
            0,
            0,
            0,
            0,
            message_hwnd,
            None,
            instance,
            None,
        )
        .context("创建托盘消息窗口失败")?;

        Ok(hwnd)
    }
}

fn pick_bundle_save_path() -> Result<Option<std::path::PathBuf>> {
    pick_file_path(false)
}

fn pick_bundle_open_path() -> Result<Option<std::path::PathBuf>> {
    pick_file_path(true)
}

fn pick_file_path(open: bool) -> Result<Option<std::path::PathBuf>> {
    use windows::core::PWSTR;

    unsafe {
        let mut file_buffer = [0u16; 260];
        let mut ofn = OPENFILENAMEW {
            lStructSize: std::mem::size_of::<OPENFILENAMEW>() as u32,
            lpstrFile: PWSTR(file_buffer.as_mut_ptr()),
            nMaxFile: file_buffer.len() as u32,
            lpstrFilter: windows::core::w!("Plugin Bundle\0*.json\0All Files\0*.*\0\0"),
            nFilterIndex: 1,
            Flags: OFN_EXPLORER | OFN_PATHMUSTEXIST | OFN_HIDEREADONLY,
            ..Default::default()
        };

        let ok = if open {
            GetOpenFileNameW(&mut ofn).as_bool()
        } else {
            GetSaveFileNameW(&mut ofn).as_bool()
        };

        if !ok {
            return Ok(None);
        }

        let len = file_buffer.iter().position(|&ch| ch == 0).unwrap_or(file_buffer.len());
        let path = String::from_utf16_lossy(&file_buffer[..len]);
        Ok(Some(std::path::PathBuf::from(path)))
    }
}

/// 如果配置文件不存在，创建默认配置
fn create_default_config(path: &std::path::Path) -> Result<()> {
    let default_config = r#"# Which-Key Windows 配置文件
# 修饰键: C=Ctrl, A=Alt, S=Shift, M=Win/Meta
# 语法: "C-c" = { desc = "描述" }
#        "g" = { desc = "组名", group = "组名" }

[globals]
"C-c" = { desc = "复制" }
"C-v" = { desc = "粘贴" }
"C-x" = { desc = "剪切" }
"C-z" = { desc = "撤销" }
"C-y" = { desc = "重做" }
"C-s" = { desc = "保存" }
"C-f" = { desc = "查找" }
"C-a" = { desc = "全选" }
"C-p" = { desc = "向上移动/上一项" }
"C-n" = { desc = "向下移动/下一项" }
"C-w" = { desc = "关闭窗口/删除前一个单词" }
"C-t" = { desc = "新建标签页" }
"C-r" = { desc = "刷新/重新加载" }
"C-h" = { desc = "查找并替换" }
"C-o" = { desc = "打开文件" }
"C-l" = { desc = "聚焦地址栏（浏览器）" }
"C-d" = { desc = "收藏当前页面（浏览器）" }
"C-j" = { desc = "打开下载记录（浏览器）" }

# Alt 常用快捷键
"A-f" = { desc = "打开文件菜单（多数 Windows 程序）" }
"A-e" = { desc = "打开编辑菜单（多数 Windows 程序）" }
"A-v" = { desc = "打开视图菜单（多数 Windows 程序）" }
"A-h" = { desc = "打开帮助菜单（多数 Windows 程序）" }
"A-Tab" = { desc = "切换应用窗口" }
"A-S-Tab" = { desc = "反向切换应用窗口" }
"A-F4" = { desc = "关闭当前窗口" }
"A-Enter" = { desc = "查看所选文件属性" }
"A-Space" = { desc = "打开当前窗口菜单" }
"A-Left" = { desc = "后退" }
"A-Right" = { desc = "前进" }
"A-Up" = { desc = "返回上一级（资源管理器）" }
"A-Esc" = { desc = "按打开顺序切换窗口" }

# Ctrl+Shift 常用快捷键
"C-S-c" = { desc = "复制（部分终端/编辑器）" }
"C-S-v" = { desc = "粘贴（部分终端/编辑器）" }
"C-S-p" = { desc = "命令面板（VS Code 等程序）" }
"C-S-n" = { desc = "新建窗口（部分程序）" }
"C-S-t" = { desc = "恢复已关闭标签页（浏览器）" }
"C-S-w" = { desc = "关闭当前标签页（浏览器）" }
"C-S-z" = { desc = "重做（部分编辑器）" }
"C-S-s" = { desc = "另存为（多数应用）" }

# Win / Meta 常用系统快捷键
"M-d" = { desc = "显示或恢复桌面" }
"M-e" = { desc = "打开文件资源管理器" }
"M-i" = { desc = "打开 Windows 设置" }
"M-l" = { desc = "锁定电脑" }
"M-r" = { desc = "打开运行" }
"M-s" = { desc = "打开 Windows 搜索" }
"M-v" = { desc = "打开剪贴板历史" }
"M-a" = { desc = "打开快速设置" }
"M-n" = { desc = "打开通知中心" }
"M-x" = { desc = "打开高级系统菜单" }
"M-p" = { desc = "切换投影或显示模式" }
"M-k" = { desc = "打开无线显示连接" }
"M-g" = { desc = "打开 Xbox Game Bar" }
"M-u" = { desc = "打开辅助功能设置" }
"M-z" = { desc = "打开贴靠布局（Windows 11）" }
"M-m" = { desc = "最小化所有窗口" }
"M-S-m" = { desc = "恢复最小化的窗口" }
"M-S-s" = { desc = "打开截图工具" }
"M-C-d" = { desc = "新建虚拟桌面" }

"#;

    std::fs::write(path, default_config)
        .with_context(|| format!("创建默认配置失败: {}", path.display()))?;
    log::info!("已创建默认配置: {}", path.display());
    Ok(())
}

fn next_plugin_template_path(plugin_dir: &std::path::Path) -> Result<std::path::PathBuf> {
    for index in 1..=9999 {
        let name = if index == 1 {
            "plugin.toml".to_string()
        } else {
            format!("plugin-{index}.toml")
        };
        let candidate = plugin_dir.join(name);
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    anyhow::bail!("无法生成新的插件文件名")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn show_all_state_enables_hook_interception() {
        let registry = Arc::new(registry::ShortcutRegistry {
            globals: types::Node::new(None),
            applications: Default::default(),
        });
        let mut state_machine = state_machine::StateMachine::new(registry);
        assert!(!show_all_is_open(&state_machine));
        state_machine.handle_event(types::KeyEvent::ToggleShowAll);
        assert!(show_all_is_open(&state_machine));
        state_machine.handle_event(types::KeyEvent::ToggleShowAll);
        assert!(!show_all_is_open(&state_machine));
    }

    #[test]
    fn show_all_requires_a_foreground_process() {
        assert!(capture_show_all_process(Ok(None)).is_err());
        assert!(capture_show_all_process(Err(anyhow::anyhow!("access denied"))).is_err());
        assert_eq!(
            capture_show_all_process(Ok(Some("code.exe".to_string()))).unwrap(),
            "code.exe"
        );
    }

    #[test]
    fn reload_dismisses_an_active_state_machine() {
        let registry = Arc::new(registry::ShortcutRegistry {
            globals: types::Node::new(None),
            applications: Default::default(),
        });
        let mut state_machine = state_machine::StateMachine::new(registry);
        state_machine.handle_event(types::KeyEvent::ToggleShowAll);
        assert!(matches!(
            state_machine.dismiss(),
            Some(types::UiCommand::Hide)
        ));
        assert!(!show_all_is_open(&state_machine));
    }

    #[test]
    fn successful_reload_dismisses_show_all_and_uses_the_new_snapshot_on_next_trigger() {
        const OLD: &str = "[globals]\n\"C-p\" = { desc = \"Old command\" }\n";
        const NEW: &str = "[globals]\n\"C-p\" = { desc = \"New command\" }\n";

        let temp_dir = tempfile::tempdir().unwrap();
        let (configuration, _) = config::ConfigurationService::from_sources(
            OLD,
            plugin::BUILTIN_PLUGINS,
            temp_dir.path(),
        )
        .unwrap();
        let mut state_machine = state_machine::StateMachine::new(
            keymap_resolver::KeymapResolver::from_snapshot(&configuration.current())
                .resolve(None)
                .registry,
        );
        state_machine.handle_event(types::KeyEvent::ToggleShowAll);

        let (sender, _receiver) = mpsc::channel();
        let hook = hook::KeyboardHook::new(sender).unwrap();
        hook.set_show_all_open(true);

        configuration
            .reload(NEW, plugin::BUILTIN_PLUGINS, temp_dir.path())
            .unwrap();
        assert!(matches!(
            dismiss_after_successful_reload(&mut state_machine, &hook),
            Some(types::UiCommand::Hide)
        ));
        assert_eq!(state_machine.state, state_machine::State::Idle);
        assert!(!hook.show_all_open_for_test());

        let resolved =
            keymap_resolver::KeymapResolver::from_snapshot(&configuration.current()).resolve(None);
        state_machine.replace_registry(resolved.registry, resolved.app_name);
        match state_machine.handle_event(types::KeyEvent::ToggleShowAll) {
            Some(types::UiCommand::ShowAll { entries, .. }) => {
                assert_eq!(entries[0].desc, "New command");
            }
            command => panic!("expected show-all from new snapshot, got {command:?}"),
        }
    }
}
