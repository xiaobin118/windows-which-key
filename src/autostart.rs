use anyhow::{bail, Context, Result};
use std::iter::once;
use std::path::Path;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
use windows::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyW, RegDeleteValueW, RegGetValueW, RegSetValueExW,
    HKEY, HKEY_CURRENT_USER, RRF_RT_REG_SZ, REG_SZ,
};

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const VALUE_NAME: &str = "Which-Key Windows";

pub fn is_enabled() -> Result<bool> {
    unsafe {
        let mut buffer = [0u16; 512];
        let mut size = (buffer.len() * 2) as u32;
        let run_key = to_wide(RUN_KEY);
        let value_name = to_wide(VALUE_NAME);
        let status = RegGetValueW(
            HKEY_CURRENT_USER,
            PCWSTR(run_key.as_ptr()),
            PCWSTR(value_name.as_ptr()),
            RRF_RT_REG_SZ,
            None,
            Some(buffer.as_mut_ptr() as *mut _),
            Some(&mut size),
        );
        if status == ERROR_FILE_NOT_FOUND {
            return Ok(false);
        }
        if status != ERROR_SUCCESS {
            bail!("读取开机自启项失败: {status:?}");
        }
        Ok(true)
    }
}

pub fn set_enabled(enabled: bool, exe_path: &Path) -> Result<()> {
    unsafe {
        let mut key = HKEY::default();
        let run_key = to_wide(RUN_KEY);
        RegCreateKeyW(
            HKEY_CURRENT_USER,
            PCWSTR(run_key.as_ptr()),
            &mut key,
        )
        .ok()
        .context("打开启动项注册表失败")?;

        let result = if enabled {
            let value = format!("\"{}\"", exe_path.display());
            let data = to_wide(&value);
            let value_name = to_wide(VALUE_NAME);
            RegSetValueExW(
                key,
                PCWSTR(value_name.as_ptr()),
                0,
                REG_SZ,
                Some(std::slice::from_raw_parts(
                    data.as_ptr() as *const u8,
                    data.len() * 2,
                )),
            )
            .ok()
            .context("写入开机自启项失败")
        } else {
            let value_name = to_wide(VALUE_NAME);
            let status = RegDeleteValueW(key, PCWSTR(value_name.as_ptr()));
            if status == ERROR_FILE_NOT_FOUND {
                Ok(())
            } else {
                status.ok().context("删除开机自启项失败")
            }
        };

        let _ = RegCloseKey(key);
        result
    }
}

fn to_wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(once(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wide_conversion_is_null_terminated() {
        let wide = to_wide("abc");
        assert_eq!(wide.last().copied(), Some(0));
    }
}
