use crate::shortcut::format_shortcut;
use crate::types::*;
use std::collections::HashMap;

#[derive(Clone)]
pub struct ShortcutRegistry {
    pub globals: Node,
    pub applications: HashMap<String, Node>,
}

impl ShortcutRegistry {
    pub fn entries_at(&self, path: &[ShortcutKey]) -> Vec<DisplayEntry> {
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
            .map(|(shortcut_key, node)| {
                let key_str = format_shortcut(shortcut_key);
                DisplayEntry {
                    key: key_str,
                    desc: node.desc.clone().unwrap_or_default(),
                    is_group: !node.children.is_empty(),
                }
            })
            .collect();

        // Sort: groups first, then by key
        entries.sort_by(|a, b| b.is_group.cmp(&a.is_group).then_with(|| a.key.cmp(&b.key)));

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
                        key: format_shortcut(&key),
                        desc: node.desc.clone().unwrap_or_default(),
                        is_group: false,
                    })
                } else {
                    let mut breadcrumb: Vec<String> = path.iter().map(format_shortcut).collect();
                    breadcrumb.push(format_shortcut(&key));
                    ResolveResult::Group(breadcrumb)
                }
            }
            None => ResolveResult::NotFound,
        }
    }
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
        root.children
            .insert(copy_key, Node::new(Some("Copy".to_string())));

        let git_key = ShortcutKey {
            modifiers: ModifierSet::empty(),
            key: Key::G,
        };
        let mut git_node = Node::new(Some("Git".to_string()));

        let status_key = ShortcutKey {
            modifiers: ModifierSet::empty(),
            key: Key::S,
        };
        git_node
            .children
            .insert(status_key, Node::new(Some("Git status".to_string())));

        let commit_key = ShortcutKey {
            modifiers: ModifierSet::empty(),
            key: Key::C,
        };
        git_node
            .children
            .insert(commit_key, Node::new(Some("Git commit".to_string())));

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
