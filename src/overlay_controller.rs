use anyhow::{Context, Result};
use crate::types::UiCommand;
use crate::window_manager::WindowManager;
use crate::webview_bridge::WebView2Bridge;
use crate::webview_bridge::FRONTEND_HTML;

const DEFAULT_WIDTH: i32 = 400;
const DEFAULT_HEIGHT: i32 = 300;

pub struct OverlayController {
    window_manager: WindowManager,
    webview_bridge: Option<WebView2Bridge>,
    window_size: (i32, i32),
}

impl OverlayController {
    pub fn new() -> Result<Self> {
        let mut window_manager = WindowManager::new();
        let hwnd = window_manager.create_window()
            .context("Failed to create overlay window")?;

        // Initialize WebView2 bridge (may fail gracefully on stub)
        let webview_bridge = WebView2Bridge::new(hwnd).ok();

        // Load frontend HTML
        if let Some(ref bridge) = webview_bridge {
            bridge.load_html(FRONTEND_HTML).ok();
        }

        Ok(OverlayController {
            window_manager,
            webview_bridge,
            window_size: (DEFAULT_WIDTH, DEFAULT_HEIGHT),
        })
    }

    pub fn execute(&mut self, cmd: UiCommand) -> Result<()> {
        match cmd {
            UiCommand::Show { position, entries, breadcrumb } => {
                log::debug!("Show overlay at {:?}, {} entries", position, entries.len());
                let (x, y) = self.adjust_position(position);
                self.window_manager.show(x, y, self.window_size.0, self.window_size.1)?;

                if let Some(ref bridge) = self.webview_bridge {
                    bridge.send_command(&UiCommand::Show {
                        position,
                        entries,
                        breadcrumb,
                    })?;
                }
            }
            UiCommand::UpdateEntries { entries, breadcrumb } => {
                log::debug!("Update entries: {} entries", entries.len());
                if let Some(ref bridge) = self.webview_bridge {
                    bridge.send_command(&UiCommand::UpdateEntries {
                        entries,
                        breadcrumb,
                    })?;
                }
            }
            UiCommand::ShowAll { app_name, entries } => {
                self.window_manager
                    .show(100, 100, self.window_size.0, self.window_size.1)?;
                if let Some(ref bridge) = self.webview_bridge {
                    bridge.send_command(&UiCommand::ShowAll { app_name, entries })?;
                }
            }
            UiCommand::Hide => {
                log::debug!("Hide overlay");
                self.window_manager.hide()?;
                if let Some(ref bridge) = self.webview_bridge {
                    bridge.send_command(&UiCommand::Hide)?;
                }
            }
        }
        Ok(())
    }

    /// Keep window on screen if the cursor is near the right/bottom edge
    fn adjust_position(&self, (x, y): (i32, i32)) -> (i32, i32) {
        use windows::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN};

        let screen_w = unsafe { GetSystemMetrics(SM_CXSCREEN) };
        let screen_h = unsafe { GetSystemMetrics(SM_CYSCREEN) };

        let max_x = screen_w - self.window_size.0;
        let max_y = screen_h - self.window_size.1;

        (x.min(max_x).max(0), y.min(max_y).max(0))
    }
}
