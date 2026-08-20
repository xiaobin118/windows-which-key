use anyhow::{Context, Result};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::Duration;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::*;
use which_key_windows::*;

const WM_TRAY_CALLBACK: u32 = WM_USER + 1;

fn main() -> Result<()> {
    env_logger::init();
    log::info!("Which-Key Windows 启动中...");

    // ── 加载配置 ──
    let config_path = std::env::current_dir()?.join("which-key.toml");
    let config = if config_path.exists() {
        config::Config::load(config_path.clone())?
    } else {
        create_default_config(&config_path)?;
        config::Config::load(config_path.clone())?
    };
    log::info!("配置加载完成: {}", config_path.display());

    // 注册表是纯数据树，可共享给状态机
    let registry = Arc::new(config.registry.clone());

    // ── 创建键盘事件通道 ──
    let (tx, rx) = mpsc::channel::<types::KeyEvent>();

    // ── 安装键盘钩子 ──
    let hook = hook::KeyboardHook::new(tx).context("键盘钩子创建失败")?;
    hook.install().context("键盘钩子安装失败")?;
    log::info!("键盘钩子已安装");

    // ── 初始化核心模块 ──
    let mut state_machine = state_machine::StateMachine::new(registry.clone());
    let mut overlay = overlay_controller::OverlayController::new()
        .context("覆盖层初始化失败")?;
    log::info!("覆盖层已初始化");

    // ── 创建托盘图标用的消息窗口 ──
    let tray_hwnd = create_message_window()?;
    let tray = tray_icon::TrayIcon::new(tray_hwnd)?;
    tray.show().context("托盘图标创建失败")?;
    log::info!("托盘图标已创建");

    log::info!("就绪。按住 Ctrl 约 300ms 显示快捷键提示。");

    // ── 主事件循环 ──
    let mut running = true;
    let mut tick_accumulator = Duration::ZERO;
    let tick_interval = Duration::from_millis(10);

    while running {
        let loop_start = std::time::Instant::now();

        // 1. 处理键盘事件
        while let Ok(event) = rx.try_recv() {
            if let Some(cmd) = state_machine.handle_event(event) {
                overlay.execute(cmd)?;
            }
        }

        // 2. 检查状态机定时器
        if let Some(cmd) = state_machine.tick() {
            overlay.execute(cmd)?;
        }

        // 3. 处理 Windows 消息（托盘图标）
        running = pump_messages(&tray_hwnd, &tray, running)?;

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
fn pump_messages(
    tray_hwnd: &HWND,
    tray: &tray_icon::TrayIcon,
    mut running: bool,
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
                            log::info!("重新加载配置（暂未实现）");
                        }
                        tray_icon::TrayCommand::OpenConfig => {
                            log::info!("打开配置文件（暂未实现）");
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
            0, 0, 0, 0,
            message_hwnd,
            None,
            instance,
            None,
        )
        .context("创建托盘消息窗口失败")?;

        Ok(hwnd)
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

"g" = { desc = "Git", group = "git" }

[globals.groups.git]
"s" = { desc = "Git status" }
"c" = { desc = "Git commit" }
"p" = { desc = "Git push" }
"l" = { desc = "Git log" }
"d" = { desc = "Git diff" }
"a" = { desc = "Git add" }
"b" = { desc = "Git branch" }
"#;

    std::fs::write(path, default_config)
        .with_context(|| format!("创建默认配置失败: {}", path.display()))?;
    log::info!("已创建默认配置: {}", path.display());
    Ok(())
}
