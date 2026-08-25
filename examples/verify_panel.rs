//! 最终验证：启动 app，注入 Win+Shift+C，截取面板窗口。

use std::path::PathBuf;
use std::time::Duration;
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC, GetDIBits,
    BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, SRCCOPY,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VK_C, VK_LSHIFT,
    VK_LWIN,
};
use windows::Win32::UI::WindowsAndMessaging::{FindWindowW, GetWindowRect, IsWindowVisible};

fn press(vk: u16, up: bool) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: windows::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY(vk),
                wScan: 0,
                dwFlags: if up {
                    KEYEVENTF_KEYUP
                } else {
                    windows::Win32::UI::Input::KeyboardAndMouse::KEYBD_EVENT_FLAGS(0)
                },
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

fn capture(hwnd: HWND, path: &str) {
    unsafe {
        let mut rect = RECT::default();
        if GetWindowRect(hwnd, &mut rect).is_err() {
            eprintln!("GetWindowRect failed");
            return;
        }
        let (w, h) = (rect.right - rect.left, rect.bottom - rect.top);
        let screen = GetDC(None);
        let mem = CreateCompatibleDC(screen);
        let bmp = CreateCompatibleBitmap(screen, w, h);
        let _ = windows::Win32::Graphics::Gdi::SelectObject(mem, bmp);
        let _ = BitBlt(mem, 0, 0, w, h, screen, rect.left, rect.top, SRCCOPY);

        let mut info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                biHeight: -h,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut pixels = vec![0u8; (w * h * 4) as usize];
        let _ = GetDIBits(
            mem,
            bmp,
            0,
            h as u32,
            Some(pixels.as_mut_ptr() as *mut _),
            &mut info,
            DIB_RGB_COLORS,
        );
        let _ = DeleteObject(bmp);
        let _ = DeleteDC(mem);
        let _ = windows::Win32::Graphics::Gdi::ReleaseDC(None, screen);

        let mut header = [0u8; 14];
        header[0..2].copy_from_slice(&0x4D42u16.to_le_bytes());
        header[2..6].copy_from_slice(&((14 + 40 + pixels.len()) as u32).to_le_bytes());
        header[10..14].copy_from_slice(&((14 + 40) as u32).to_le_bytes());
        let mut file = std::fs::File::create(path).unwrap();
        use std::io::Write;
        file.write_all(&header).unwrap();
        file.write_all(std::slice::from_raw_parts(
            (&info.bmiHeader as *const _) as *const u8,
            40,
        ))
        .unwrap();
        file.write_all(&pixels).unwrap();
        eprintln!("saved {path} ({w}x{h})");
    }
}

fn main() {
    unsafe {
        let seq = [
            press(VK_LWIN.0, false),
            press(VK_LSHIFT.0, false),
            press(VK_C.0, false),
            press(VK_C.0, true),
            press(VK_LSHIFT.0, true),
            press(VK_LWIN.0, true),
        ];
        let _ = SendInput(&seq, std::mem::size_of::<INPUT>() as i32);

        for _ in 0..24 {
            std::thread::sleep(Duration::from_millis(500));
            if let Ok(hwnd) = FindWindowW(windows::core::w!("WhichKeyControlPanel"), None) {
                if IsWindowVisible(hwnd).as_bool() && !IsIconic(hwnd).as_bool() {
                    std::thread::sleep(Duration::from_millis(1500));
                    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target").join("verification_panel.bmp");
                    capture(hwnd, path.to_string_lossy().as_ref());
                    return;
                }
            }
        }
        eprintln!("panel not found or not restored");
    }
}

use windows::Win32::UI::WindowsAndMessaging::IsIconic;
