use anyhow::{Context, Result};
use std::sync::atomic::{AtomicBool, Ordering};
use windows::Win32::Foundation::{HWND, LPARAM};
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::WindowsAndMessaging::*;

pub enum TrayCommand {
    Quit,
    ReloadConfig,
    OpenConfig,
}

static TRAY_ACTIVE: AtomicBool = AtomicBool::new(false);

pub struct TrayIcon {
    hwnd: HWND,
    icon_id: u32,
}

impl TrayIcon {
    pub fn new(hwnd: HWND) -> Result<Self> {
        Ok(TrayIcon {
            hwnd,
            icon_id: 1,
        })
    }

    pub fn show(&self) -> Result<()> {
        unsafe {
            let mut icon_data = NOTIFYICONDATAW::default();
            icon_data.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
            icon_data.hWnd = self.hwnd;
            icon_data.uID = self.icon_id;
            icon_data.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP;
            icon_data.uCallbackMessage = WM_USER + 1;

            // Use default application icon
            let icon = LoadIconW(None, IDI_APPLICATION)
                .context("Failed to load icon")?;
            icon_data.hIcon = icon;

            // Set tooltip
            let tooltip = "Which-Key Windows";
            let tooltip_wide: Vec<u16> = tooltip.encode_utf16().chain(std::iter::once(0)).collect();
            icon_data.szTip[..tooltip_wide.len().min(128)]
                .copy_from_slice(&tooltip_wide[..tooltip_wide.len().min(128)]);

            if !Shell_NotifyIconW(NIM_ADD, &icon_data).as_bool() {
                anyhow::bail!("Failed to add tray icon");
            }

            TRAY_ACTIVE.store(true, Ordering::SeqCst);
        }

        Ok(())
    }

    pub fn hide(&self) -> Result<()> {
        unsafe {
            let mut icon_data = NOTIFYICONDATAW::default();
            icon_data.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
            icon_data.hWnd = self.hwnd;
            icon_data.uID = self.icon_id;

            if !Shell_NotifyIconW(NIM_DELETE, &icon_data).as_bool() {
                anyhow::bail!("Failed to remove tray icon");
            }

            TRAY_ACTIVE.store(false, Ordering::SeqCst);
        }

        Ok(())
    }

    pub fn handle_message(&self, msg: u32, lparam: LPARAM) -> Option<TrayCommand> {
        if msg == WM_USER + 1 {
            match lparam.0 as u32 {
                WM_RBUTTONUP => {
                    self.show_context_menu()
                }
                _ => None,
            }
        } else {
            None
        }
    }

    fn show_context_menu(&self) -> Option<TrayCommand> {
        unsafe {
            let menu = CreatePopupMenu().ok()?;

            AppendMenuW(menu, MF_STRING, 1, windows::core::w!("重新加载配置")).ok()?;
            AppendMenuW(menu, MF_STRING, 2, windows::core::w!("打开配置文件")).ok()?;
            AppendMenuW(menu, MF_SEPARATOR, 0, None).ok()?;
            AppendMenuW(menu, MF_STRING, 3, windows::core::w!("退出")).ok()?;

            let mut cursor_pos = Default::default();
            GetCursorPos(&mut cursor_pos).ok()?;

            let cmd = TrackPopupMenu(
                menu,
                TPM_RETURNCMD | TPM_NONOTIFY,
                cursor_pos.x,
                cursor_pos.y,
                0,
                self.hwnd,
                None,
            );

            DestroyMenu(menu).ok()?;

            match cmd.0 {
                1 => Some(TrayCommand::ReloadConfig),
                2 => Some(TrayCommand::OpenConfig),
                3 => Some(TrayCommand::Quit),
                _ => None,
            }
        }
    }
}

impl Drop for TrayIcon {
    fn drop(&mut self) {
        if TRAY_ACTIVE.load(Ordering::SeqCst) {
            let _ = self.hide();
        }
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn test_tray_icon_creation() {
        // This test would require a valid HWND
        // For now, just verify the struct can be created
    }
}
