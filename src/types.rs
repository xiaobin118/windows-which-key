use bitflags::bitflags;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use windows::Win32::UI::Input::KeyboardAndMouse::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Key(pub u32);

impl Key {
    pub const A: Key = Key(0x41);
    pub const B: Key = Key(0x42);
    pub const C: Key = Key(0x43);
    pub const D: Key = Key(0x44);
    pub const E: Key = Key(0x45);
    pub const F: Key = Key(0x46);
    pub const G: Key = Key(0x47);
    pub const H: Key = Key(0x48);
    pub const I: Key = Key(0x49);
    pub const J: Key = Key(0x4A);
    pub const K: Key = Key(0x4B);
    pub const L: Key = Key(0x4C);
    pub const M: Key = Key(0x4D);
    pub const N: Key = Key(0x4E);
    pub const O: Key = Key(0x4F);
    pub const P: Key = Key(0x50);
    pub const Q: Key = Key(0x51);
    pub const R: Key = Key(0x52);
    pub const S: Key = Key(0x53);
    pub const T: Key = Key(0x54);
    pub const U: Key = Key(0x55);
    pub const V: Key = Key(0x56);
    pub const W: Key = Key(0x57);
    pub const X: Key = Key(0x58);
    pub const Y: Key = Key(0x59);
    pub const Z: Key = Key(0x5A);

    pub fn from_vk(vk: u32) -> Self {
        Key(vk)
    }
    pub fn vk_code(&self) -> u32 {
        self.0
    }
}

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            0x41..=0x5A => write!(f, "{}", (b'A' + (self.0 - 0x41) as u8) as char),
            _ => write!(f, "VK_{:02X}", self.0),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Modifier {
    Ctrl,
    Alt,
    Shift,
    Meta,
}

impl Modifier {
    pub fn from_vk(vk: u32) -> Option<Self> {
        match vk {
            v if v == VK_CONTROL.0 as u32
                || v == VK_LCONTROL.0 as u32
                || v == VK_RCONTROL.0 as u32 =>
            {
                Some(Modifier::Ctrl)
            }
            v if v == VK_MENU.0 as u32 || v == VK_LMENU.0 as u32 || v == VK_RMENU.0 as u32 => {
                Some(Modifier::Alt)
            }
            v if v == VK_SHIFT.0 as u32 || v == VK_LSHIFT.0 as u32 || v == VK_RSHIFT.0 as u32 => {
                Some(Modifier::Shift)
            }
            v if v == VK_LWIN.0 as u32 || v == VK_RWIN.0 as u32 => Some(Modifier::Meta),
            _ => None,
        }
    }
}

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct ModifierSet: u8 {
        const CTRL  = 0b0001;
        const ALT   = 0b0010;
        const SHIFT = 0b0100;
        const META  = 0b1000;
    }
}

impl ModifierSet {
    pub fn from_modifier(m: Modifier) -> Self {
        match m {
            Modifier::Ctrl => ModifierSet::CTRL,
            Modifier::Alt => ModifierSet::ALT,
            Modifier::Shift => ModifierSet::SHIFT,
            Modifier::Meta => ModifierSet::META,
        }
    }
    pub fn insert_modifier(&mut self, m: Modifier) {
        *self |= Self::from_modifier(m);
    }
    pub fn remove_modifier(&mut self, m: Modifier) {
        *self &= !Self::from_modifier(m);
    }
    pub fn contains_modifier(&self, m: Modifier) -> bool {
        self.contains(Self::from_modifier(m))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyEvent {
    KeyDown(Key),
    KeyUp(Key),
    ModifierDown(Modifier),
    ModifierUp(Modifier),
    ToggleShowAll,
    ToggleControlPanel,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ShortcutKey {
    pub modifiers: ModifierSet,
    pub key: Key,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BindingPriority {
    Essential,
    Recommended,
    Advanced,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BindingMetadata {
    pub category: String,
    pub priority: BindingPriority,
}

#[derive(Debug, Clone)]
pub struct Node {
    pub desc: Option<String>,
    pub metadata: Option<BindingMetadata>,
    pub children: HashMap<ShortcutKey, Node>,
    pub group_name: Option<String>,
}

impl Node {
    pub fn new(desc: Option<String>) -> Self {
        Self {
            desc,
            metadata: None,
            children: HashMap::new(),
            group_name: None,
        }
    }
    pub fn new_binding(desc: String, metadata: BindingMetadata) -> Self {
        Self {
            desc: Some(desc),
            metadata: Some(metadata),
            children: HashMap::new(),
            group_name: None,
        }
    }
    pub fn is_leaf(&self) -> bool {
        self.children.is_empty()
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct DisplayEntry {
    pub key: String,
    pub desc: String,
    pub is_group: bool,
    pub category: String,
    pub priority: BindingPriority,
}

#[derive(Debug)]
pub enum UiCommand {
    Show {
        position: (i32, i32),
        app_name: String,
        entries: Vec<DisplayEntry>,
        breadcrumb: Vec<String>,
    },
    ShowAll {
        app_name: String,
        entries: Vec<DisplayEntry>,
    },
    UpdateEntries {
        app_name: String,
        entries: Vec<DisplayEntry>,
        breadcrumb: Vec<String>,
    },
    ApplyTheme {
        theme: crate::theme::ThemeConfig,
    },
    Hide,
}

#[derive(Debug)]
pub enum ResolveResult {
    Leaf(DisplayEntry),
    Group(Vec<String>),
    NotFound,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_key_display() {
        assert_eq!(Key::C.to_string(), "C");
        assert_eq!(Key::A.to_string(), "A");
    }

    #[test]
    fn test_modifier_set_operations() {
        let mut mods = ModifierSet::empty();
        mods.insert_modifier(Modifier::Ctrl);
        assert!(mods.contains_modifier(Modifier::Ctrl));
        assert!(!mods.contains_modifier(Modifier::Alt));
        mods.insert_modifier(Modifier::Alt);
        assert!(mods.contains_modifier(Modifier::Alt));
        mods.remove_modifier(Modifier::Ctrl);
        assert!(!mods.contains_modifier(Modifier::Ctrl));
        assert!(mods.contains_modifier(Modifier::Alt));
    }

    #[test]
    fn test_node_is_leaf() {
        let leaf = Node::new(Some("Copy".to_string()));
        assert!(leaf.is_leaf());
        let mut group = Node::new(Some("Git".to_string()));
        group.children.insert(
            ShortcutKey {
                modifiers: ModifierSet::empty(),
                key: Key::S,
            },
            Node::new(Some("Status".to_string())),
        );
        assert!(!group.is_leaf());
    }
}
