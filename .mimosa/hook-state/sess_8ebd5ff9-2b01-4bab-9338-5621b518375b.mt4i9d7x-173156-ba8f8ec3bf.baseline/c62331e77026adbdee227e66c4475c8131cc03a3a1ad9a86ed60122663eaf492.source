use anyhow::{bail, Result};

use crate::types::{Key, ModifierSet, ShortcutKey};

const VK_BACK: u32 = 0x08;
const VK_TAB: u32 = 0x09;
const VK_RETURN: u32 = 0x0D;
const VK_ESCAPE: u32 = 0x1B;
const VK_SPACE: u32 = 0x20;
const VK_PRIOR: u32 = 0x21;
const VK_NEXT: u32 = 0x22;
const VK_END: u32 = 0x23;
const VK_HOME: u32 = 0x24;
const VK_LEFT: u32 = 0x25;
const VK_UP: u32 = 0x26;
const VK_RIGHT: u32 = 0x27;
const VK_DOWN: u32 = 0x28;
const VK_DELETE: u32 = 0x2E;
const VK_F1: u32 = 0x70;

// Windows keyboard hooks report OEM virtual keys for punctuation.
const VK_OEM_1: u32 = 0xBA;
const VK_OEM_PLUS: u32 = 0xBB;
const VK_OEM_COMMA: u32 = 0xBC;
const VK_OEM_MINUS: u32 = 0xBD;
const VK_OEM_PERIOD: u32 = 0xBE;
const VK_OEM_2: u32 = 0xBF;
const VK_OEM_3: u32 = 0xC0;

pub fn parse_shortcut(input: &str) -> Result<ShortcutKey> {
    let input = input.trim();
    if input.is_empty() {
        bail!("Shortcut cannot be empty");
    }

    let parts = split_shortcut(input)?;
    let (key_part, modifier_parts) = parts
        .split_last()
        .ok_or_else(|| anyhow::anyhow!("Shortcut cannot be empty"))?;

    let mut modifiers = ModifierSet::empty();
    for part in modifier_parts {
        match part.to_ascii_lowercase().as_str() {
            "c" | "ctrl" | "control" => modifiers |= ModifierSet::CTRL,
            "a" | "alt" => modifiers |= ModifierSet::ALT,
            "s" | "shift" => modifiers |= ModifierSet::SHIFT,
            "m" | "meta" | "win" => modifiers |= ModifierSet::META,
            _ => bail!("Unknown modifier: {part}"),
        }
    }

    Ok(ShortcutKey {
        modifiers,
        key: parse_key(key_part)?,
    })
}

pub fn format_shortcut(key: &ShortcutKey) -> String {
    let mut parts = Vec::new();
    if key.modifiers.contains(ModifierSet::CTRL) {
        parts.push("C".to_string());
    }
    if key.modifiers.contains(ModifierSet::ALT) {
        parts.push("A".to_string());
    }
    if key.modifiers.contains(ModifierSet::SHIFT) {
        parts.push("S".to_string());
    }
    if key.modifiers.contains(ModifierSet::META) {
        parts.push("M".to_string());
    }
    parts.push(format_key(key.key));
    parts.join("-")
}

fn split_shortcut(input: &str) -> Result<Vec<&str>> {
    if matches!(input, "+" | "-") {
        return Ok(vec![input]);
    }

    let (prefix, final_key) = match input.chars().last() {
        Some(key @ ('+' | '-')) if input[..input.len() - key.len_utf8()].ends_with(['+', '-']) => (
            &input[..input.len() - key.len_utf8()],
            Some(&input[input.len() - key.len_utf8()..]),
        ),
        _ => (input, None),
    };

    let mut parts: Vec<&str> = prefix
        .split(['+', '-'])
        .filter(|part| !part.is_empty())
        .collect();
    if let Some(key) = final_key {
        parts.push(key);
    }
    if parts.len() < 2 && input.ends_with(['+', '-']) {
        bail!("Shortcut is missing a key");
    }
    if parts.is_empty() {
        bail!("Shortcut cannot be empty");
    }
    Ok(parts)
}

fn parse_key(input: &str) -> Result<Key> {
    let normalized = input.to_ascii_lowercase();
    let vk = match normalized.as_str() {
        "/" => VK_OEM_2,
        "`" => VK_OEM_3,
        "+" | "=" => VK_OEM_PLUS,
        "<" => VK_OEM_COMMA,
        "-" => VK_OEM_MINUS,
        ">" => VK_OEM_PERIOD,
        ";" => VK_OEM_1,
        "backspace" | "back" => VK_BACK,
        "delete" | "del" => VK_DELETE,
        "enter" | "return" => VK_RETURN,
        "esc" | "escape" => VK_ESCAPE,
        "space" => VK_SPACE,
        "tab" => VK_TAB,
        "home" => VK_HOME,
        "end" => VK_END,
        "pageup" | "pgup" => VK_PRIOR,
        "pagedown" | "pgdown" => VK_NEXT,
        "left" | "leftarrow" => VK_LEFT,
        "up" | "uparrow" => VK_UP,
        "right" | "rightarrow" => VK_RIGHT,
        "down" | "downarrow" => VK_DOWN,
        _ if normalized.len() == 1 && normalized.as_bytes()[0].is_ascii_alphanumeric() => {
            normalized.as_bytes()[0].to_ascii_uppercase() as u32
        }
        _ if normalized.starts_with('f') => match normalized[1..].parse::<u32>() {
            Ok(number @ 1..=24) => VK_F1 + number - 1,
            _ => bail!("Unsupported key: {input}"),
        },
        _ => bail!("Unsupported key: {input}"),
    };
    Ok(Key::from_vk(vk))
}

fn format_key(key: Key) -> String {
    match key.vk_code() {
        0x30..=0x39 => (key.vk_code() as u8 as char).to_string(),
        0x41..=0x5A => ((key.vk_code() as u8 as char).to_ascii_lowercase()).to_string(),
        VK_OEM_1 => ";".to_string(),
        VK_OEM_PLUS => "+".to_string(),
        VK_OEM_COMMA => "<".to_string(),
        VK_OEM_MINUS => "-".to_string(),
        VK_OEM_PERIOD => ">".to_string(),
        VK_OEM_2 => "/".to_string(),
        VK_OEM_3 => "`".to_string(),
        VK_BACK => "Backspace".to_string(),
        VK_DELETE => "Delete".to_string(),
        VK_RETURN => "Enter".to_string(),
        VK_ESCAPE => "Esc".to_string(),
        VK_SPACE => "Space".to_string(),
        VK_TAB => "Tab".to_string(),
        VK_HOME => "Home".to_string(),
        VK_END => "End".to_string(),
        VK_PRIOR => "PageUp".to_string(),
        VK_NEXT => "PageDown".to_string(),
        VK_LEFT => "Left".to_string(),
        VK_UP => "Up".to_string(),
        VK_RIGHT => "Right".to_string(),
        VK_DOWN => "Down".to_string(),
        VK_F1..=0x87 => format!("F{}", key.vk_code() - VK_F1 + 1),
        vk => format!("VK_{vk:02X}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_canonicalizes_named_and_symbol_keys() {
        for (input, expected) in [
            ("Ctrl-Shift-P", "C-S-p"),
            ("F12", "F12"),
            ("PageDown", "PageDown"),
            ("/", "/"),
            ("`", "`"),
            ("Ctrl-+", "C-+"),
            ("Alt+=", "A-+"),
            ("Ctrl+Shift++", "C-S-+"),
            ("Ctrl+Shift+<", "C-S-<"),
            ("Ctrl+Shift+>", "C-S->"),
        ] {
            let key = parse_shortcut(input).unwrap();
            assert_eq!(format_shortcut(&key), expected);
        }
    }

    #[test]
    fn rejects_modifier_without_a_key() {
        assert!(parse_shortcut("Ctrl-").is_err());
    }
}
