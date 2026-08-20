use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use anyhow::{Context, Result};
use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::*;
use crate::types::*;

// Wrapper for HHOOK to make it Send+Sync
struct HookHandle(HHOOK);
unsafe impl Send for HookHandle {}
unsafe impl Sync for HookHandle {}

static HOOK_HANDLE: std::sync::Mutex<Option<HookHandle>> = std::sync::Mutex::new(None);
static EVENT_SENDER: std::sync::Mutex<Option<Sender<KeyEvent>>> = std::sync::Mutex::new(None);
static HOOK_ACTIVE: AtomicBool = AtomicBool::new(false);

pub struct KeyboardHook {
    active: Arc<AtomicBool>,
}

impl KeyboardHook {
    pub fn new(sender: Sender<KeyEvent>) -> Result<Self> {
        *EVENT_SENDER.lock().unwrap() = Some(sender);
        Ok(KeyboardHook {
            active: Arc::new(AtomicBool::new(false)),
        })
    }

    pub fn install(&self) -> Result<()> {
        let active = self.active.clone();

        std::thread::spawn(move || {
            unsafe {
                let hook = SetWindowsHookExW(
                    WH_KEYBOARD_LL,
                    Some(keyboard_proc),
                    None,
                    0,
                ).context("Failed to install keyboard hook").unwrap();

                *HOOK_HANDLE.lock().unwrap() = Some(HookHandle(hook));
                HOOK_ACTIVE.store(true, Ordering::SeqCst);
                active.store(true, Ordering::SeqCst);

                // Message loop - required for hook to work
                let mut msg = MSG::default();
                while GetMessageW(&mut msg, None, 0, 0).into() {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }
        });

        // Wait for hook to be installed
        while !self.active.load(Ordering::SeqCst) {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        Ok(())
    }

    pub fn uninstall(&self) -> Result<()> {
        HOOK_ACTIVE.store(false, Ordering::SeqCst);
        self.active.store(false, Ordering::SeqCst);

        if let Some(handle) = HOOK_HANDLE.lock().unwrap().take() {
            unsafe {
                UnhookWindowsHookEx(handle.0).context("Failed to uninstall hook")?;
            }
        }

        Ok(())
    }
}

unsafe extern "system" fn keyboard_proc(
    n_code: i32,
    w_param: WPARAM,
    l_param: LPARAM,
) -> LRESULT {
    if n_code >= 0 && HOOK_ACTIVE.load(Ordering::SeqCst) {
        let kb_struct = *(l_param.0 as *const KBDLLHOOKSTRUCT);
        let vk = kb_struct.vkCode;

        // Determine if this is a key down or key up event
        let is_key_down = w_param.0 as u32 == WM_KEYDOWN || w_param.0 as u32 == WM_SYSKEYDOWN;

        // Check if it's a modifier key
        if let Some(modifier) = Modifier::from_vk(vk) {
            let event = if is_key_down {
                KeyEvent::ModifierDown(modifier)
            } else {
                KeyEvent::ModifierUp(modifier)
            };

            if let Ok(sender_lock) = EVENT_SENDER.lock() {
                if let Some(sender) = sender_lock.as_ref() {
                    let _ = sender.send(event);
                }
            }
        } else {
            // Regular key
            let key = Key::from_vk(vk);
            let event = if is_key_down {
                KeyEvent::KeyDown(key)
            } else {
                KeyEvent::KeyUp(key)
            };

            if let Ok(sender_lock) = EVENT_SENDER.lock() {
                if let Some(sender) = sender_lock.as_ref() {
                    let _ = sender.send(event);
                }
            }
        }
    }

    CallNextHookEx(None, n_code, w_param, l_param)
}

impl Drop for KeyboardHook {
    fn drop(&mut self) {
        let _ = self.uninstall();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_hook_install_uninstall() {
        let (tx, _rx) = mpsc::channel();
        let hook = KeyboardHook::new(tx).unwrap();

        // Install hook
        hook.install().unwrap();
        assert!(hook.active.load(Ordering::SeqCst));

        // Uninstall hook
        hook.uninstall().unwrap();
        assert!(!hook.active.load(Ordering::SeqCst));
    }

    #[test]
    fn test_hook_captures_keys() {
        let (tx, _rx) = mpsc::channel();
        let hook = KeyboardHook::new(tx).unwrap();
        hook.install().unwrap();

        // Wait a bit for hook to be ready
        thread::sleep(Duration::from_millis(100));

        // Note: This test requires manual key presses to verify
        // In a real scenario, you would press keys and verify they're captured
        // For now, we just verify the hook is installed

        hook.uninstall().unwrap();
    }
}
