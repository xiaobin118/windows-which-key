use crate::plugin::{BindingKeys, PluginDefinition};
use crate::shortcut::format_shortcut;
use crate::types::*;
use anyhow::{bail, Result};
use std::collections::HashMap;

#[derive(Clone)]
pub struct ShortcutRegistry {
    pub globals: Node,
    pub applications: HashMap<String, Node>,
}

impl ShortcutRegistry {
    pub fn from_plugin(plugin: &PluginDefinition) -> Result<Self> {
        let mut globals = Node::new(None);
        for binding in &plugin.bindings {
            match &binding.keys {
                BindingKeys::Alternatives(keys) => {
                    for key in keys {
                        insert_binding(
                            &mut globals,
                            std::slice::from_ref(key),
                            &binding.description,
                            &binding.metadata,
                        )?;
                    }
                }
                BindingKeys::Sequence(keys) => {
                    insert_binding(&mut globals, keys, &binding.description, &binding.metadata)?;
                }
            }
        }
        Ok(Self {
            globals,
            applications: HashMap::new(),
        })
    }

    pub fn merge_from(&mut self, higher_priority: &ShortcutRegistry) {
        merge_node(&mut self.globals, &higher_priority.globals);
        for (application, node) in &higher_priority.applications {
            match self.applications.get_mut(application) {
                Some(existing) => merge_node(existing, node),
                None => {
                    self.applications.insert(application.clone(), node.clone());
                }
            }
        }
    }

    pub fn all_entries(&self) -> Vec<DisplayEntry> {
        let mut entries = Vec::new();
        collect_entries(&self.globals, &mut Vec::new(), &mut entries);
        sort_entries(&mut entries);
        entries
    }

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
                display_entry(key_str, node)
            })
            .collect();

        sort_entries(&mut entries);

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
                    ResolveResult::Leaf(display_entry(format_shortcut(&key), node))
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

fn insert_binding(
    root: &mut Node,
    keys: &[ShortcutKey],
    description: &str,
    metadata: &BindingMetadata,
) -> Result<()> {
    if keys.is_empty() {
        bail!("Plugin binding keys cannot be empty");
    }

    let mut current = root;
    for (index, key) in keys.iter().enumerate() {
        let is_last = index + 1 == keys.len();
        if is_last {
            match current.children.get(key) {
                Some(existing) if !existing.children.is_empty() => {
                    bail!(
                        "Binding {} is both a leaf and a sequence prefix",
                        format_shortcut(key)
                    );
                }
                _ => {
                    current.children.insert(
                        key.clone(),
                        Node::new_binding(description.to_string(), metadata.clone()),
                    );
                }
            }
        } else {
            let next = current
                .children
                .entry(key.clone())
                .or_insert_with(|| Node::new(None));
            if next.desc.is_some() {
                bail!(
                    "Binding {} is both a leaf and a sequence prefix",
                    format_shortcut(key)
                );
            }
            update_group_metadata(next, metadata);
            current = next;
        }
    }
    Ok(())
}

fn update_group_metadata(group: &mut Node, candidate: &BindingMetadata) {
    let should_replace = match group.metadata.as_ref() {
        None => true,
        Some(existing) => {
            candidate.priority < existing.priority
                || (candidate.priority == existing.priority
                    && candidate.category < existing.category)
        }
    };
    if should_replace {
        group.metadata = Some(candidate.clone());
    }
}

fn merge_node(lower_priority: &mut Node, higher_priority: &Node) {
    if higher_priority.children.is_empty() {
        *lower_priority = higher_priority.clone();
        return;
    }
    if lower_priority.children.is_empty() && lower_priority.desc.is_some() {
        *lower_priority = higher_priority.clone();
        return;
    }

    if higher_priority.desc.is_some() {
        lower_priority.desc = higher_priority.desc.clone();
    }
    if higher_priority.metadata.is_some() {
        lower_priority.metadata = higher_priority.metadata.clone();
    }
    if higher_priority.group_name.is_some() {
        lower_priority.group_name = higher_priority.group_name.clone();
    }
    for (key, higher_child) in &higher_priority.children {
        match lower_priority.children.get_mut(key) {
            Some(lower_child) => merge_node(lower_child, higher_child),
            None => {
                lower_priority
                    .children
                    .insert(key.clone(), higher_child.clone());
            }
        }
    }
}

fn collect_entries(node: &Node, path: &mut Vec<String>, entries: &mut Vec<DisplayEntry>) {
    for (key, child) in &node.children {
        path.push(format_shortcut(key));
        if child.children.is_empty() {
            entries.push(display_entry(path.join(", "), child));
        } else {
            collect_entries(child, path, entries);
        }
        path.pop();
    }
}

fn display_entry(key: String, node: &Node) -> DisplayEntry {
    let metadata = node.metadata.as_ref();
    DisplayEntry {
        key,
        desc: node.desc.clone().unwrap_or_default(),
        is_group: !node.children.is_empty(),
        category: metadata
            .map(|value| value.category.clone())
            .unwrap_or_default(),
        priority: metadata
            .map(|value| value.priority)
            .unwrap_or(BindingPriority::Advanced),
    }
}

fn sort_entries(entries: &mut [DisplayEntry]) {
    entries.sort_by(|left, right| {
        left.priority
            .cmp(&right.priority)
            .then_with(|| left.category.cmp(&right.category))
            .then_with(|| right.is_group.cmp(&left.is_group))
            .then_with(|| left.key.cmp(&right.key))
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin::{parse_plugin_toml, PluginOrigin};
    use crate::shortcut::parse_shortcut;

    const TEST_PLUGIN: &str = r#"
schema_version = 1
id = "editor"
name = "Editor"
processes = ["editor.exe"]

[[bindings]]
keys = ["C-S-p", "F1"]
description = "Command Palette"
category = "Navigation"
priority = "essential"

[[bindings]]
keys = ["C-k", "C-f"]
description = "Format Selection"
category = "Editing"
priority = "recommended"
sequence = true
"#;

    fn key(value: &str) -> ShortcutKey {
        parse_shortcut(value).unwrap()
    }

    fn registry_with(
        key_label: &str,
        description: &str,
        category: &str,
        priority: BindingPriority,
    ) -> ShortcutRegistry {
        let plugin = format!(
            r#"
schema_version = 1
id = "test"
name = "Test"
processes = ["test.exe"]

[[bindings]]
keys = ["{key_label}"]
description = "{description}"
category = "{category}"
priority = "{priority}"
"#,
            priority = match priority {
                BindingPriority::Essential => "essential",
                BindingPriority::Recommended => "recommended",
                BindingPriority::Advanced => "advanced",
            }
        );
        ShortcutRegistry::from_plugin(&parse_plugin_toml(&plugin, PluginOrigin::BuiltIn).unwrap())
            .unwrap()
    }

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

    #[test]
    fn compiles_alternative_and_sequence_bindings() {
        let plugin = parse_plugin_toml(TEST_PLUGIN, PluginOrigin::BuiltIn).unwrap();
        let registry = ShortcutRegistry::from_plugin(&plugin).unwrap();

        assert!(matches!(
            registry.resolve(&[], key("C-S-p")),
            ResolveResult::Leaf(_)
        ));
        assert!(matches!(
            registry.resolve(&[], key("F1")),
            ResolveResult::Leaf(_)
        ));
        assert!(matches!(
            registry.resolve(&[], key("C-k")),
            ResolveResult::Group(_)
        ));
        assert!(matches!(
            registry.resolve(&[key("C-k")], key("C-f")),
            ResolveResult::Leaf(_)
        ));
    }

    #[test]
    fn higher_priority_registry_replaces_same_path_only() {
        let mut global = registry_with("C-p", "Global", "Windows", BindingPriority::Recommended);
        global
            .globals
            .children
            .insert(key("C-c"), Node::new(Some("Copy".to_string())));
        let app = registry_with(
            "C-p",
            "Quick Open",
            "Navigation",
            BindingPriority::Essential,
        );

        global.merge_from(&app);

        assert_eq!(leaf_desc(&global, "C-p"), "Quick Open");
        assert_eq!(leaf_desc(&global, "C-c"), "Copy");
    }

    #[test]
    fn all_entries_flattens_sequences_with_full_key_labels() {
        let plugin = parse_plugin_toml(TEST_PLUGIN, PluginOrigin::BuiltIn).unwrap();
        let registry = ShortcutRegistry::from_plugin(&plugin).unwrap();
        let entries = registry.all_entries();

        assert!(entries.iter().any(|entry| entry.key == "C-k, C-f"));
    }

    #[test]
    fn sequence_prefix_inherits_its_binding_metadata_for_display() {
        let plugin = parse_plugin_toml(TEST_PLUGIN, PluginOrigin::BuiltIn).unwrap();
        let registry = ShortcutRegistry::from_plugin(&plugin).unwrap();

        let prefix = registry
            .entries_at(&[])
            .into_iter()
            .find(|entry| entry.key == "C-k")
            .unwrap();

        assert!(prefix.is_group);
        assert_eq!(prefix.category, "Editing");
        assert_eq!(prefix.priority, BindingPriority::Recommended);
    }

    #[test]
    fn shared_sequence_prefix_uses_highest_priority_then_category_metadata() {
        let plugin = parse_plugin_toml(
            r#"
schema_version = 1
id = "shared-prefix"
name = "Shared Prefix"
processes = ["shared-prefix.exe"]

[[bindings]]
keys = ["C-k", "C-z"]
description = "Zebra"
category = "Zebra"
priority = "essential"
sequence = true

[[bindings]]
keys = ["C-k", "C-a"]
description = "Alpha"
category = "Alpha"
priority = "essential"
sequence = true

[[bindings]]
keys = ["C-k", "C-r"]
description = "Recommended"
category = "Recommended"
priority = "recommended"
sequence = true
"#,
            PluginOrigin::BuiltIn,
        )
        .unwrap();
        let registry = ShortcutRegistry::from_plugin(&plugin).unwrap();

        let prefix = registry
            .entries_at(&[])
            .into_iter()
            .find(|entry| entry.key == "C-k")
            .unwrap();

        assert_eq!(prefix.category, "Alpha");
        assert_eq!(prefix.priority, BindingPriority::Essential);
    }

    #[test]
    fn display_entries_sort_by_priority_category_group_and_key() {
        let mut root = Node::new(None);
        root.children.insert(
            key("C-z"),
            Node::new_binding(
                "Recommended".to_string(),
                BindingMetadata {
                    category: "A".to_string(),
                    priority: BindingPriority::Recommended,
                },
            ),
        );
        let mut group = Node::new_binding(
            "Group".to_string(),
            BindingMetadata {
                category: "B".to_string(),
                priority: BindingPriority::Essential,
            },
        );
        group.children.insert(
            key("C-a"),
            Node::new_binding(
                "Child".to_string(),
                BindingMetadata {
                    category: "B".to_string(),
                    priority: BindingPriority::Essential,
                },
            ),
        );
        root.children.insert(key("C-y"), group);
        root.children.insert(
            key("C-x"),
            Node::new_binding(
                "Essential".to_string(),
                BindingMetadata {
                    category: "A".to_string(),
                    priority: BindingPriority::Essential,
                },
            ),
        );
        let registry = ShortcutRegistry {
            globals: root,
            applications: HashMap::new(),
        };

        let entries = registry.entries_at(&[]);

        assert_eq!(
            entries
                .iter()
                .map(|entry| entry.key.as_str())
                .collect::<Vec<_>>(),
            vec!["C-x", "C-y", "C-z"]
        );
        assert_eq!(entries[0].category, "A");
        assert_eq!(entries[0].priority, BindingPriority::Essential);
    }

    #[test]
    fn rejects_leaf_prefix_ambiguity() {
        let plugin = parse_plugin_toml(
            r#"
schema_version = 1
id = "ambiguous"
name = "Ambiguous"
processes = ["ambiguous.exe"]

[[bindings]]
keys = ["C-k"]
description = "Prefix"
category = "Editing"
priority = "recommended"

[[bindings]]
keys = ["C-k", "C-f"]
description = "Child"
category = "Editing"
priority = "recommended"
sequence = true
"#,
            PluginOrigin::BuiltIn,
        )
        .unwrap();

        assert!(ShortcutRegistry::from_plugin(&plugin).is_err());
    }

    fn leaf_desc(registry: &ShortcutRegistry, key_label: &str) -> String {
        match registry.resolve(&[], key(key_label)) {
            ResolveResult::Leaf(entry) => entry.desc,
            _ => panic!("Expected leaf"),
        }
    }
}
