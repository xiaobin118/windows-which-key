use anyhow::{Context, Result};
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Dwm::{
    DwmSetWindowAttribute, DWMWA_SYSTEMBACKDROP_TYPE, DWMSBT_TRANSIENTWINDOW,
    DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND,
};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::w;

pub struct WindowManager {
    hwnd: Option<HWND>,
}

impl WindowManager {
    pub fn new() -> Self {
        WindowManager { hwnd: None }
    }

    pub fn create_window(&mut self) -> Result<HWND> {
        unsafe {
            // Use a simple approach without custom window class
            let hwnd = CreateWindowExW(
                WS_EX_NOACTIVATE | WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
                w!("Static"),  // Use predefined system class
                w!("Which-Key"),
                WS_POPUP,
                0, 0, 400, 300,
                None,
                None,
                None,
                None,
            )
            .context("Failed to create window")?;

            self.hwnd = Some(hwnd);

            // Apply glassmorphism
            self.apply_glassmorphism()?;

            Ok(hwnd)
        }
    }

    fn apply_glassmorphism(&self) -> Result<()> {
        let hwnd = self.hwnd.context("Window not created")?;

        unsafe {
            // Try modern acrylic (Windows 11 22H2+)
            let backdrop_type = DWMSBT_TRANSIENTWINDOW;
            let result = DwmSetWindowAttribute(
                hwnd,
                DWMWA_SYSTEMBACKDROP_TYPE,
                &backdrop_type as *const _ as *const _,
                std::mem::size_of::<i32>() as u32,
            );

            if result.is_err() {
                log::warn!("Modern acrylic effect not available, falling back to basic styling");
            }

            // Enable rounded corners
            let corner_pref = DWMWCP_ROUND;
            DwmSetWindowAttribute(
                hwnd,
                DWMWA_WINDOW_CORNER_PREFERENCE,
                &corner_pref as *const _ as *const _,
                std::mem::size_of::<i32>() as u32,
            )?;

            Ok(())
        }
    }

    pub fn show(&self, x: i32, y: i32, width: i32, height: i32) -> Result<()> {
        let hwnd = self.hwnd.context("Window not created")?;

        unsafe {
            SetWindowPos(
                hwnd,
                None,
                x, y,
                width, height,
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
