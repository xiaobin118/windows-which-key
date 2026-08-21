use std::collections::HashSet;
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
static SHOW_ALL_OPEN: AtomicBool = AtomicBool::new(false);
static DECISION_STATE: std::sync::LazyLock<std::sync::Mutex<HookDecisionState>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(HookDecisionState::default()));

const VK_SLASH: u32 = 0xBF;
const VK_ESCAPE: u32 = 0x1B;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HookAction {
    SendAndPass(KeyEvent),
    SendAndSwallow(KeyEvent),
    Swallow,
}

pub struct HookDecisionState {
    pub modifiers: ModifierSet,
    swallowed_keys: HashSet<Key>,
    toggle_pressed: bool,
}

impl HookDecisionState {
    fn new() -> Self {
        Self {
            modifiers: ModifierSet::empty(),
            swallowed_keys: HashSet::new(),
            toggle_pressed: false,
        }
    }

    pub fn on_key_down(&mut self, key: Key, show_all_open: bool) -> HookAction {
        if key.vk_code() == VK_SLASH && self.toggle_pressed {
            return HookAction::Swallow;
        }

        if key.vk_code() == VK_SLASH && self.modifiers == (ModifierSet::META | ModifierSet::SHIFT) {
            self.toggle_pressed = true;
            self.swallowed_keys.insert(key);
            return HookAction::SendAndSwallow(KeyEvent::ToggleShowAll);
        }

        if key.vk_code() == VK_ESCAPE && show_all_open {
            self.swallowed_keys.insert(key);
            return HookAction::SendAndSwallow(KeyEvent::KeyDown(key));
        }

        HookAction::SendAndPass(KeyEvent::KeyDown(key))
    }

    pub fn on_key_up(&mut self, key: Key) -> HookAction {
        if self.swallowed_keys.remove(&key) {
            if key.vk_code() == VK_SLASH {
                self.toggle_pressed = false;
            }
            return HookAction::Swallow;
        }
        HookAction::SendAndPass(KeyEvent::KeyUp(key))
    }

    fn on_modifier(&mut self, modifier: Modifier, is_down: bool) -> HookAction {
        if is_down {
            self.modifiers.insert_modifier(modifier);
            HookAction::SendAndPass(KeyEvent::ModifierDown(modifier))
        } else {
            self.modifiers.remove_modifier(modifier);
            HookAction::SendAndPass(KeyEvent::ModifierUp(modifier))
        }
    }
}

impl Default for HookDecisionState {
    fn default() -> Self {
        Self::new()
    }
}

pub struct KeyboardHook {
    active: Arc<AtomicBool>,
}

impl KeyboardHook {
    pub fn new(sender: Sender<KeyEvent>) -> Result<Self> {
        *EVENT_SENDER.lock().unwrap() = Some(sender);
        *DECISION_STATE.lock().unwrap() = HookDecisionState::default();
        SHOW_ALL_OPEN.store(false, Ordering::SeqCst);
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

    pub fn set_show_all_open(&self, open: bool) {
        SHOW_ALL_OPEN.store(open, Ordering::SeqCst);
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

        let is_key_down = w_param.0 as u32 == WM_KEYDOWN || w_param.0 as u32 == WM_SYSKEYDOWN;
        let action = if let Some(modifier) = Modifier::from_vk(vk) {
            DECISION_STATE
                .lock()
                .map(|mut state| state.on_modifier(modifier, is_key_down))
        } else {
            let key = Key::from_vk(vk);
            DECISION_STATE.lock().map(|mut state| {
                if is_key_down {
                    state.on_key_down(key, SHOW_ALL_OPEN.load(Ordering::SeqCst))
                } else {
                    state.on_key_up(key)
                }
            })
        };

        if let Ok(action) = action {
            match action {
                HookAction::SendAndPass(event) => {
                    if let Ok(sender_lock) = EVENT_SENDER.lock() {
                        if let Some(sender) = sender_lock.as_ref() {
                            let _ = sender.send(event);
                        }
                    }
                }
                HookAction::SendAndSwallow(event) => {
                    if let Ok(sender_lock) = EVENT_SENDER.lock() {
                        if let Some(sender) = sender_lock.as_ref() {
                            let _ = sender.send(event);
                        }
                    }
                    return LRESULT(1);
                }
                HookAction::Swallow => return LRESULT(1),
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

    fn key(name: &str) -> Key {
        match name {
            "/" => Key::from_vk(0xBF),
            "Esc" => Key::from_vk(0x1B),
            _ => panic!("unknown test key: {name}"),
        }
    }

    #[test]
    fn show_all_hotkey_is_swallowed_but_normal_shortcuts_pass() {
        let mut decision = HookDecisionState::default();
        decision.modifiers = ModifierSet::META | ModifierSet::SHIFT;

        assert_eq!(
            decision.on_key_down(key("/"), false),
            HookAction::SendAndSwallow(KeyEvent::ToggleShowAll)
        );
        assert_eq!(
            decision.on_key_down(Key::P, false),
            HookAction::SendAndPass(KeyEvent::KeyDown(Key::P))
        );
    }

    #[test]
    fn escape_is_swallowed_only_while_show_all_is_open() {
        let mut decision = HookDecisionState::default();

        assert_eq!(
            decision.on_key_down(key("Esc"), false),
            HookAction::SendAndPass(KeyEvent::KeyDown(key("Esc")))
        );
        assert_eq!(
            decision.on_key_down(key("Esc"), true),
            HookAction::SendAndSwallow(KeyEvent::KeyDown(key("Esc")))
        );
    }

    #[test]
    fn show_all_hotkey_does_not_repeat_or_leak_key_up() {
        let mut decision = HookDecisionState::default();
        decision.modifiers = ModifierSet::META | ModifierSet::SHIFT;

        assert!(matches!(
            decision.on_key_down(key("/"), false),
            HookAction::SendAndSwallow(KeyEvent::ToggleShowAll)
        ));
        assert_eq!(decision.on_key_down(key("/"), false), HookAction::Swallow);
        assert_eq!(decision.on_key_up(key("/")), HookAction::Swallow);
        assert!(matches!(
            decision.on_key_down(key("/"), false),
            HookAction::SendAndSwallow(KeyEvent::ToggleShowAll)
        ));
    }

    #[test]
    fn repeat_is_swallowed_after_shift_is_released_until_slash_key_up() {
        let mut decision = HookDecisionState::default();
        decision.modifiers = ModifierSet::META | ModifierSet::SHIFT;
        assert!(matches!(
            decision.on_key_down(key("/"), false),
            HookAction::SendAndSwallow(KeyEvent::ToggleShowAll)
        ));

        decision.on_modifier(Modifier::Shift, false);
        assert_eq!(
            decision.on_key_down(key("/"), false),
            HookAction::Swallow
        );
        assert_eq!(decision.on_key_up(key("/")), HookAction::Swallow);
        assert_eq!(
            decision.on_key_down(key("/"), false),
            HookAction::SendAndPass(KeyEvent::KeyDown(key("/")))
        );
    }

    #[test]
    fn repeat_is_swallowed_after_ctrl_is_pressed_until_slash_key_up() {
        let mut decision = HookDecisionState::default();
        decision.modifiers = ModifierSet::META | ModifierSet::SHIFT;
        assert!(matches!(
            decision.on_key_down(key("/"), false),
            HookAction::SendAndSwallow(KeyEvent::ToggleShowAll)
        ));

        decision.on_modifier(Modifier::Ctrl, true);
        assert_eq!(
            decision.on_key_down(key("/"), false),
            HookAction::Swallow
        );
        assert_eq!(decision.on_key_up(key("/")), HookAction::Swallow);
        assert_eq!(
            decision.on_key_down(key("/"), false),
            HookAction::SendAndPass(KeyEvent::KeyDown(key("/")))
        );
    }

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
