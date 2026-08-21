use windows::Win32::Foundation::{HWND, HINSTANCE, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::w;

unsafe extern "system" fn overlay_window_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    DefWindowProcW(hwnd, msg, wparam, lparam)
}

fn main() {
    unsafe {
        let instance = GetModuleHandleW(None).unwrap();
        let instance = HINSTANCE(instance.0);
        let class_name = w!("WhichKeyOverlay");

        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(overlay_window_proc),
            hInstance: instance,
            lpszClassName: class_name,
            ..Default::default()
        };
        let atom = RegisterClassExW(&wc);
        println!("RegisterClassExW atom: {:x}", atom);
        if atom == 0 {
            println!("GetLastError: {:?}", windows::Win32::Foundation::GetLastError());
            return;
        }

        // Test 1: Minimal popup window, no extended styles
        let ex_styles = [
            "none",
            "NOACTIVATE",
            "TOPMOST",
            "TOOLWINDOW",
            "NOACTIVATE|TOPMOST|TOOLWINDOW",
        ];
        let styles: [WINDOW_EX_STYLE; 5] = [
            WINDOW_EX_STYLE::default(),
            WS_EX_NOACTIVATE,
            WS_EX_TOPMOST,
            WS_EX_TOOLWINDOW,
            WS_EX_NOACTIVATE | WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
        ];

        for (i, (name, ex)) in ex_styles.iter().zip(styles.iter()).enumerate() {
            let result = CreateWindowExW(
                *ex,
                class_name,
                w!("Which-Key"),
                WS_POPUP,
                0, 0, 400, 300,
                None,
                None,
                instance,
                None,
            );
            match result {
                Ok(hwnd) => {
                    println!("Test {} [{}]: OK, hwnd={:p}", i, name, hwnd.0);
                    let _ = DestroyWindow(hwnd);
                }
                Err(e) => {
                    let err = windows::Win32::Foundation::GetLastError();
                    println!("Test {} [{}]: FAILED {:?} (GetLastError={})", i, name, e, err.0);
                }
            }
        }
    }
}
