use anyhow::{bail, Result};
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use crate::types::{Key, ModifierSet, ShortcutKey};

pub fn parse_shortcut(input: &str) -> Result<ShortcutKey> {
    if input == "-" {
        return Ok(ShortcutKey { modifiers: ModifierSet::empty(), key: Key(VK_OEM_MINUS.0 as u32) });
    }
    let mut parts: Vec<&str> = input.split('-').collect();
    if parts.len() > 1 && parts.last() == Some(&"") {
        parts.pop();
    }
    if parts.is_empty() || parts.last() == Some(&"") {
        bail!("shortcut must include a key: {input}");
    }
    let key_part = parts.pop().unwrap();
    let mut modifiers = ModifierSet::empty();
    for part in parts {
        match part.to_ascii_lowercase().as_str() {
            "c" | "ctrl" | "control" => modifiers |= ModifierSet::CTRL,
            "a" | "alt" => modifiers |= ModifierSet::ALT,
            "s" | "shift" => modifiers |= ModifierSet::SHIFT,
            "m" | "meta" | "win" => modifiers |= ModifierSet::META,
            _ => bail!("unknown modifier: {part}"),
        }
    }
    let key = parse_key(key_part)?;
    Ok(ShortcutKey { modifiers, key })
}

fn parse_key(value: &str) -> Result<Key> {
    if value.len() == 1 {
        let byte = value.as_bytes()[0];
        return Ok(match byte {
            b'a'..=b'z' | b'A'..=b'Z' => Key(byte.to_ascii_uppercase() as u32),
            b'0'..=b'9' => Key(byte as u32),
            b'/' => Key(VK_OEM_2.0 as u32),
            b'`' => Key(VK_OEM_3.0 as u32),
            b'+' => Key(VK_OEM_PLUS.0 as u32),
            b'-' => Key(VK_OEM_MINUS.0 as u32),
            b';' => Key(VK_OEM_1.0 as u32),
            _ => bail!("unsupported key: {value}"),
        });
    }
    let normalized = value.to_ascii_lowercase();
    let vk = match normalized.as_str() {
        "backspace" => VK_BACK.0,
        "delete" => VK_DELETE.0,
        "enter" => VK_RETURN.0,
        "esc" => VK_ESCAPE.0,
        "space" => VK_SPACE.0,
        "tab" => VK_TAB.0,
        "home" => VK_HOME.0,
        "end" => VK_END.0,
        "pageup" => VK_PRIOR.0,
        "pagedown" => VK_NEXT.0,
        "left" => VK_LEFT.0,
        "right" => VK_RIGHT.0,
        "up" => VK_UP.0,
        "down" => VK_DOWN.0,
        value if value.starts_with('f') => {
            let number: u32 = value[1..].parse().map_err(|_| anyhow::anyhow!("unsupported key: {value}"))?;
            if !(1..=24).contains(&number) { bail!("unsupported key: {value}"); }
            VK_F1.0 + number - 1
        }
        _ => bail!("unsupported key: {value}"),
    };
    Ok(Key(vk as u32))
}

pub fn format_shortcut(key: &ShortcutKey) -> String {
    let mut parts = Vec::new();
    for (flag, name) in [(ModifierSet::CTRL, "C"), (ModifierSet::ALT, "A"), (ModifierSet::SHIFT, "S"), (ModifierSet::META, "M")] {
        if key.modifiers.contains(flag) { parts.push(name); }
    }
    parts.push(match key.key.0 {
        0x41..=0x5a => Box::leak(((b'a' + (key.key.0 - 0x41) as u8) as char).to_string().into_boxed_str()),
        0x30..=0x39 => Box::leak((key.key.0 as u8 as char).to_string().into_boxed_str()),
        v if v == VK_OEM_2.0 as u32 => "/",
        v if v == VK_OEM_3.0 as u32 => "`",
        v if v == VK_OEM_PLUS.0 as u32 => "+",
        v if v == VK_OEM_MINUS.0 as u32 => "-",
        v if v == VK_OEM_1.0 as u32 => ";",
        v if v == VK_BACK.0 as u32 => "Backspace",
        v if v == VK_DELETE.0 as u32 => "Delete",
        v if v == VK_RETURN.0 as u32 => "Enter",
        v if v == VK_ESCAPE.0 as u32 => "Esc",
        v if v == VK_SPACE.0 as u32 => "Space",
        v if v == VK_TAB.0 as u32 => "Tab",
        v if v == VK_HOME.0 as u32 => "Home",
        v if v == VK_END.0 as u32 => "End",
        v if v == VK_PRIOR.0 as u32 => "PageUp",
        v if v == VK_NEXT.0 as u32 => "PageDown",
        v if v == VK_LEFT.0 as u32 => "Left",
        v if v == VK_RIGHT.0 as u32 => "Right",
        v if v == VK_UP.0 as u32 => "Up",
        v if v == VK_DOWN.0 as u32 => "Down",
        v if (VK_F1.0 as u32..=VK_F24.0 as u32).contains(&v) => Box::leak(format!("F{}", v - VK_F1.0 as u32 + 1).into_boxed_str()),
        v => Box::leak(format!("VK_{v:02X}").into_boxed_str()),
    });
    parts.join("-")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn canonical_shortcuts() {
        for (input, expected) in [("Ctrl-Shift-P", "C-S-p"), ("F12", "F12"), ("PageDown", "PageDown"), ("/", "/"), ("`", "`"), ("Ctrl-+", "C-+")] {
            assert_eq!(format_shortcut(&parse_shortcut(input).unwrap()), expected);
        }
    }
    #[test] fn rejects_incomplete_shortcut() { assert!(parse_shortcut("Ctrl-").is_err()); }
}
