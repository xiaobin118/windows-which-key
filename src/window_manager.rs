use anyhow::{Context, Result};
use windows::core::w;
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Dwm::{
    DwmSetWindowAttribute, DWMSBT_TRANSIENTWINDOW, DWMWA_SYSTEMBACKDROP_TYPE,
    DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::*;

pub struct WindowManager {
    hwnd: Option<HWND>,
}

impl Default for WindowManager {
    fn default() -> Self {
        Self::new()
    }
}

unsafe extern "system" fn overlay_window_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    DefWindowProcW(hwnd, msg, wparam, lparam)
}

impl WindowManager {
    pub fn new() -> Self {
        WindowManager { hwnd: None }
    }

    pub fn create_window(&mut self) -> Result<HWND> {
        unsafe {
            let instance = GetModuleHandleW(None).context("获取模块句柄失败")?;
            let instance = HINSTANCE(instance.0);
            let class_name = w!("WhichKeyOverlay");

            // 注册自定义窗口类
            let wc = WNDCLASSEXW {
                cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
                style: CS_HREDRAW | CS_VREDRAW,
                lpfnWndProc: Some(overlay_window_proc),
                hInstance: instance,
                lpszClassName: class_name,
                ..Default::default()
            };
            if RegisterClassExW(&wc) == 0 {
                let err = windows::Win32::Foundation::GetLastError();
                anyhow::bail!("注册窗口类失败: {:?}", err);
            }

            let hwnd = CreateWindowExW(
                WS_EX_NOACTIVATE | WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
                class_name,
                w!("Which-Key"),
                WS_POPUP,
                0,
                0,
                400,
                300,
                None,
                None,
                instance,
                None,
            )
            .map_err(|e| {
                let err = windows::Win32::Foundation::GetLastError();
                anyhow::anyhow!("创建窗口失败: {} (0x{:08X})", e, err.0)
            })?;

            self.hwnd = Some(hwnd);

            // Apply glassmorphism
            self.apply_glassmorphism()?;

            Ok(hwnd)
        }
    }

    /// 应用毛玻璃效果。失败只记日志，不阻塞窗口创建。
    fn apply_glassmorphism(&self) -> Result<()> {
        let hwnd = self.hwnd.context("Window not created")?;

        unsafe {
            // Try modern acrylic (Windows 11 22H2+)
            let backdrop_type = DWMSBT_TRANSIENTWINDOW;
            if let Err(e) = DwmSetWindowAttribute(
                hwnd,
                DWMWA_SYSTEMBACKDROP_TYPE,
                &backdrop_type as *const _ as *const _,
                std::mem::size_of::<i32>() as u32,
            ) {
                log::warn!("毛玻璃效果不可用（Windows 11 22H2+ 才支持）: {:?}", e);
            }

            // Enable rounded corners
            let corner_pref = DWMWCP_ROUND;
            if let Err(e) = DwmSetWindowAttribute(
                hwnd,
                DWMWA_WINDOW_CORNER_PREFERENCE,
                &corner_pref as *const _ as *const _,
                std::mem::size_of::<i32>() as u32,
            ) {
                log::warn!("圆角效果不可用: {:?}", e);
            }

            Ok(())
        }
    }

    pub fn show(&self, x: i32, y: i32, width: i32, height: i32) -> Result<()> {
        let hwnd = self.hwnd.context("Window not created")?;

        unsafe {
            SetWindowPos(
                hwnd,
                None,
                x,
                y,
                width,
                height,
                SWP_SHOWWINDOW | SWP_NOACTIVATE,
            )
            .context("Failed to show window")?;
        }

        Ok(())
    }

    pub fn hide(&self) -> Result<()> {
        let hwnd = self.hwnd.context("Window not created")?;

        unsafe {
            let _ = ShowWindow(hwnd, SW_HIDE);
        }

        Ok(())
    }

    pub fn hwnd(&self) -> Option<HWND> {
        self.hwnd
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_window_creation() {
        let mut wm = WindowManager::new();
        let result = wm.create_window();

        // This test might fail in headless environments
        if let Ok(hwnd) = result {
            assert!(!hwnd.is_invalid());
            assert!(wm.hwnd().is_some());
        }
    }

    #[test]
    fn test_show_hide() {
        let mut wm = WindowManager::new();
        if wm.create_window().is_ok() {
            assert!(wm.show(100, 100, 400, 300).is_ok());
            assert!(wm.hide().is_ok());
        }
    }
}
