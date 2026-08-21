use anyhow::{Context, Result};
use windows::core::PWSTR;
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

pub trait ForegroundAppProvider {
    fn foreground_executable(&self) -> Result<Option<String>>;
}

pub struct Win32ForegroundAppProvider;

struct OwnedProcessHandle(HANDLE);

impl Drop for OwnedProcessHandle {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

fn get_window_thread_process_id_error(error: windows::core::Error) -> anyhow::Error {
    anyhow::Error::new(error).context("GetWindowThreadProcessId failed")
}

impl ForegroundAppProvider for Win32ForegroundAppProvider {
    fn foreground_executable(&self) -> Result<Option<String>> {
        unsafe {
            let foreground_window = GetForegroundWindow();
            if foreground_window.0.is_null() {
                return Ok(None);
            }

            let mut process_id = 0;
            if GetWindowThreadProcessId(foreground_window, Some(&mut process_id)) == 0 {
                return Err(get_window_thread_process_id_error(
                    windows::core::Error::from_win32(),
                ));
            }

            let process = OwnedProcessHandle(
                OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process_id)
                    .context("OpenProcess failed")?,
            );
            let mut executable_path = vec![0u16; 32_768];
            let mut length = executable_path.len() as u32;
            QueryFullProcessImageNameW(
                process.0,
                PROCESS_NAME_WIN32,
                PWSTR(executable_path.as_mut_ptr()),
                &mut length,
            )
            .context("QueryFullProcessImageNameW failed")?;

            let executable_path = String::from_utf16(&executable_path[..length as usize])
                .context("foreground executable path is not valid UTF-16")?;
            Ok(normalize_executable_path(&executable_path))
        }
    }
}

pub fn normalize_executable_path(path: &str) -> Option<String> {
    let executable_name = path.rsplit(['\\', '/']).next()?.trim();
    (!executable_name.is_empty()).then(|| executable_name.to_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_and_normalizes_windows_executable_name() {
        assert_eq!(
            normalize_executable_path(r"C:\Program Files\Microsoft VS Code\Code.exe"),
            Some("code.exe".to_string())
        );
        assert_eq!(normalize_executable_path(""), None);
    }

    #[test]
    fn normalizes_unicode_executable_names_case_insensitively() {
        assert_eq!(
            normalize_executable_path(r"C:\\Apps\\ÄPP.EXE"),
            Some("äpp.exe".to_string())
        );
    }

    #[test]
    fn thread_process_id_error_preserves_context_and_win32_error() {
        unsafe {
            windows::Win32::Foundation::SetLastError(windows::Win32::Foundation::WIN32_ERROR(5));
        }

        let error = get_window_thread_process_id_error(windows::core::Error::from_win32());

        assert_eq!(error.to_string(), "GetWindowThreadProcessId failed");
        let win32_error = error
            .chain()
            .find_map(|cause| cause.downcast_ref::<windows::core::Error>())
            .expect("error chain should preserve the Win32 error");
        assert_eq!(
            win32_error.code(),
            windows::core::HRESULT(0x8007_0005u32 as i32)
        );
    }
}
