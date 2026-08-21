use std::collections::HashMap;
use crate::types::*;

#[derive(Clone)]
pub struct ShortcutRegistry {
    pub globals: Node,
    pub applications: HashMap<String, Node>,
}

impl ShortcutRegistry {
    pub fn entries_at(&self, path: &[ShortcutKey]) -> Vec<DisplayEntry> {
        self.entries_at_with_modifiers(path, None)
    }

    /// Return entries relevant to the currently held modifiers. Unmodified
    /// entries are retained because they can be used as which-key prefixes.
    pub fn entries_at_with_modifiers(
        &self,
        path: &[ShortcutKey],
        active_modifiers: Option<ModifierSet>,
    ) -> Vec<DisplayEntry> {
        let mut current = &self.globals;

        for key in path {
            match current.children.get(key) {
                Some(node) => current = node,
                None => return vec![],
            }
        }

        let mut entries: Vec<DisplayEntry> = current
            .children
            .iter()
            .filter(|(shortcut_key, _)| {
                active_modifiers
                    .map(|active| shortcut_key.modifiers.is_empty() || shortcut_key.modifiers == active)
                    .unwrap_or(true)
            })
            .map(|(shortcut_key, node)| {
                let key_str = format_shortcut_key(shortcut_key);
                DisplayEntry {
                    key: key_str,
                    desc: node.desc.clone().unwrap_or_default(),
                    is_group: !node.children.is_empty(),
                }
            })
            .collect();

        // Sort: groups first, then by key
        entries.sort_by(|a, b| {
            b.is_group.cmp(&a.is_group).then_with(|| a.key.cmp(&b.key))
        });

        entries
    }

    pub fn resolve(&self, path: &[ShortcutKey], key: ShortcutKey) -> ResolveResult {
        let mut current = &self.globals;

        for path_key in path {
            match current.children.get(path_key) {
                Some(node) => current = node,
                None => return ResolveResult::NotFound,
            }
        }

        match current.children.get(&key) {
            Some(node) => {
                if node.children.is_empty() {
                    ResolveResult::Leaf(DisplayEntry {
                        key: format_shortcut_key(&key),
                        desc: node.desc.clone().unwrap_or_default(),
                        is_group: false,
                    })
                } else {
                    let mut breadcrumb: Vec<String> = path.iter()
                        .map(|k| format_shortcut_key(k))
                        .collect();
                    breadcrumb.push(format_shortcut_key(&key));
                    ResolveResult::Group(breadcrumb)
                }
            }
            None => ResolveResult::NotFound,
        }
    }
}

fn format_shortcut_key(key: &ShortcutKey) -> String {
    let mut parts = Vec::new();

    if key.modifiers.contains(ModifierSet::CTRL) {
        parts.push("C");
    }
    if key.modifiers.contains(ModifierSet::ALT) {
        parts.push("A");
    }
    if key.modifiers.contains(ModifierSet::SHIFT) {
        parts.push("S");
    }
    if key.modifiers.contains(ModifierSet::META) {
        parts.push("M");
    }

    let key_char = match key.key.0 {
        0x41..=0x5A => {
            let ch = (b'a' + (key.key.0 - 0x41) as u8) as char;
            ch.to_string()
        }
        0x08 => "Backspace".to_string(),
        0x09 => "Tab".to_string(),
        0x0D => "Enter".to_string(),
        0x1B => "Esc".to_string(),
        0x20 => "Space".to_string(),
        0x23 => "End".to_string(),
        0x24 => "Home".to_string(),
        0x25 => "Left".to_string(),
        0x26 => "Up".to_string(),
        0x27 => "Right".to_string(),
        0x28 => "Down".to_string(),
        0x2E => "Delete".to_string(),
        0x73 => "F4".to_string(),
        _ => format!("VK_{:02X}", key.key.0),
    };

    parts.push(&key_char);
    parts.join("-")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_test_registry() -> ShortcutRegistry {
        let mut root = Node::new(None);

        let copy_key = ShortcutKey {
            modifiers: ModifierSet::CTRL,
            key: Key::C,
        };
        root.children.insert(copy_key, Node::new(Some("Copy".to_string())));

        let git_key = ShortcutKey {
            modifiers: ModifierSet::empty(),
            key: Key::G,
        };
        let mut git_node = Node::new(Some("Git".to_string()));

        let status_key = ShortcutKey {
            modifiers: ModifierSet::empty(),
            key: Key::S,
        };
        git_node.children.insert(status_key, Node::new(Some("Git status".to_string())));

        let commit_key = ShortcutKey {
            modifiers: ModifierSet::empty(),
            key: Key::C,
        };
        git_node.children.insert(commit_key, Node::new(Some("Git commit".to_string())));

        root.children.insert(git_key, git_node);

        ShortcutRegistry {
            globals: root,
            applications: HashMap::new(),
        }
    }

    #[test]
    fn test_entries_at_root() {
        let registry = build_test_registry();
        let entries = registry.entries_at(&[]);
        assert_eq!(entries.len(), 2);

        let git_entry = entries.iter().find(|e| e.desc == "Git").unwrap();
        assert!(git_entry.is_group);
    }

    #[test]
    fn test_entries_filter_by_active_modifiers() {
        let registry = build_test_registry();
        let entries = registry.entries_at_with_modifiers(&[], Some(ModifierSet::CTRL));

        assert!(entries.iter().any(|entry| entry.key == "C-c"));
        assert!(entries.iter().any(|entry| entry.key == "g"));
        assert!(!entries.iter().any(|entry| entry.key == "A-c"));
    }

    #[test]
    fn test_entries_at_group() {
        let registry = build_test_registry();
        let git_key = ShortcutKey {
            modifiers: ModifierSet::empty(),
            key: Key::G,
        };
        let entries = registry.entries_at(&[git_key]);
        assert_eq!(entries.len(), 2);

        let status = entries.iter().find(|e| e.desc == "Git status").unwrap();
        assert!(!status.is_group);
    }

    #[test]
    fn test_resolve_leaf() {
        let registry = build_test_registry();
        let copy_key = ShortcutKey {
            modifiers: ModifierSet::CTRL,
            key: Key::C,
        };
        match registry.resolve(&[], copy_key) {
            ResolveResult::Leaf(entry) => {
                assert_eq!(entry.desc, "Copy");
                assert!(!entry.is_group);
            }
            _ => panic!("Expected Leaf"),
        }
    }

    #[test]
    fn test_resolve_group() {
        let registry = build_test_registry();
        let git_key = ShortcutKey {
            modifiers: ModifierSet::empty(),
            key: Key::G,
        };
        match registry.resolve(&[], git_key) {
            ResolveResult::Group(breadcrumb) => {
                assert_eq!(breadcrumb.len(), 1);
                assert_eq!(breadcrumb[0], "g");
            }
            _ => panic!("Expected Group"),
        }
    }

    #[test]
    fn test_resolve_not_found() {
        let registry = build_test_registry();
        let unknown_key = ShortcutKey {
            modifiers: ModifierSet::empty(),
            key: Key::Z,
        };
        match registry.resolve(&[], unknown_key) {
            ResolveResult::NotFound => {}
            _ => panic!("Expected NotFound"),
        }
    }
}
