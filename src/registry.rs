use std::collections::HashMap;
use crate::types::*;
use crate::shortcut::format_shortcut;
use crate::plugin::{BindingKeys, PluginDefinition};
use anyhow::{bail, Result};

#[derive(Clone)]
pub struct ShortcutRegistry {
    pub globals: Node,
    pub applications: HashMap<String, Node>,
}

impl ShortcutRegistry {
    pub fn from_plugin(plugin: &PluginDefinition) -> Result<Self> {
        let mut registry = Self { globals: Node::new(None), applications: HashMap::new() };
        for binding in &plugin.bindings {
            match &binding.keys {
                BindingKeys::Alternatives(keys) => for key in keys {
                    insert_path(&mut registry.globals, std::slice::from_ref(key), binding.description.clone(), binding.metadata.clone())?;
                },
                BindingKeys::Sequence(keys) => insert_path(&mut registry.globals, keys, binding.description.clone(), binding.metadata.clone())?,
            }
        }
        Ok(registry)
    }

    pub fn merge_from(&mut self, higher_priority: &ShortcutRegistry) {
        merge_nodes(&mut self.globals, &higher_priority.globals);
        for (name, node) in &higher_priority.applications {
            self.applications.entry(name.clone()).and_modify(|current| merge_nodes(current, node)).or_insert_with(|| node.clone());
        }
    }

    pub fn all_entries(&self) -> Vec<DisplayEntry> {
        let mut entries = Vec::new();
        flatten(&self.globals, &mut Vec::new(), &mut entries);
        entries.sort_by(|a, b| a.priority.cmp(&b.priority).then_with(|| a.category.cmp(&b.category)).then_with(|| b.is_group.cmp(&a.is_group)).then_with(|| a.key.cmp(&b.key)));
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
                DisplayEntry {
                    key: key_str,
                    desc: node.desc.clone().unwrap_or_default(),
                    is_group: !node.children.is_empty(),
                    category: node.metadata.as_ref().map_or_else(String::new, |m| m.category.clone()),
                    priority: node.metadata.as_ref().map_or(BindingPriority::Advanced, |m| m.priority),
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
                        key: format_shortcut(&key),
                        desc: node.desc.clone().unwrap_or_default(),
                        is_group: false,
                        category: node.metadata.as_ref().map_or_else(String::new, |m| m.category.clone()),
                        priority: node.metadata.as_ref().map_or(BindingPriority::Advanced, |m| m.priority),
                    })
                } else {
                    let mut breadcrumb: Vec<String> = path.iter()
                        .map(|k| format_shortcut(k))
                        .collect();
                    breadcrumb.push(format_shortcut(&key));
                    ResolveResult::Group(breadcrumb)
                }
            }
            None => ResolveResult::NotFound,
        }
    }
}

fn insert_path(root: &mut Node, path: &[ShortcutKey], desc: String, metadata: BindingMetadata) -> Result<()> {
    let mut node = root;
    for (index, key) in path.iter().enumerate() {
        node = node.children.entry(key.clone()).or_insert_with(|| Node::new(None));
        if index + 1 < path.len() && (node.desc.is_some() || node.metadata.is_some()) { bail!("leaf/prefix ambiguity"); }
    }
    if !node.children.is_empty() { bail!("leaf/prefix ambiguity"); }
    if node.desc.is_some() { bail!("duplicate binding path"); }
    node.desc = Some(desc);
    node.metadata = Some(metadata);
    Ok(())
}

fn merge_nodes(lower: &mut Node, higher: &Node) {
    for (key, higher_node) in &higher.children {
        match lower.children.get_mut(key) {
            Some(lower_node) if higher_node.is_leaf() => *lower_node = higher_node.clone(),
            Some(lower_node) => merge_nodes(lower_node, higher_node),
            None => { lower.children.insert(key.clone(), higher_node.clone()); }
        }
    }
}

fn flatten(node: &Node, path: &mut Vec<String>, output: &mut Vec<DisplayEntry>) {
    let mut children: Vec<_> = node.children.iter().collect();
    children.sort_by_key(|(key, _)| format_shortcut(key));
    for (key, child) in children {
        path.push(format_shortcut(key));
        if child.is_leaf() {
            let metadata = child.metadata.as_ref();
            output.push(DisplayEntry { key: path.join(", "), desc: child.desc.clone().unwrap_or_default(), is_group: false, category: metadata.map_or_else(String::new, |m| m.category.clone()), priority: metadata.map_or(BindingPriority::Advanced, |m| m.priority) });
        } else { flatten(child, path, output); }
        path.pop();
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin::{parse_plugin_toml, PluginOrigin};

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
    fn compiles_alternatives_and_sequences_and_flattens_labels() {
        let source = r#"
schema_version = 1
id = "demo"
name = "Demo"
description = "Demo"
processes = ["demo.exe"]
[[bindings]]
keys = ["C-S-p", "F1"]
description = "Palette"
category = "Navigation"
priority = "essential"
[[bindings]]
keys = ["C-k", "C-f"]
description = "Format"
category = "Editing"
priority = "recommended"
sequence = true
"#;
        let plugin = parse_plugin_toml(source, PluginOrigin::BuiltIn).unwrap();
        let entries = ShortcutRegistry::from_plugin(&plugin).unwrap().all_entries();
        assert_eq!(entries.iter().map(|entry| entry.key.as_str()).collect::<Vec<_>>(), vec!["C-S-p", "F1", "C-k, C-f"]);
        assert_eq!(entries[0].category, "Navigation");
    }

    #[test]
    fn rejects_leaf_prefix_and_merges_higher_priority_paths() {
        let lower = parse_plugin_toml(r#"
schema_version=1
id="lower"
name="Lower"
description="Lower"
processes=["x"]
[[bindings]]
keys=["C-S-p"]
description="Old"
category="Global"
priority="advanced"
[[bindings]]
keys=["F1"]
description="Keep"
category="Global"
priority="advanced"
"#, PluginOrigin::BuiltIn).unwrap();
        let higher = parse_plugin_toml(r#"
schema_version=1
id="higher"
name="Higher"
description="Higher"
processes=["x"]
[[bindings]]
keys=["C-S-p"]
description="New"
category="App"
priority="essential"
"#, PluginOrigin::BuiltIn).unwrap();
        let mut registry = ShortcutRegistry::from_plugin(&lower).unwrap();
        registry.merge_from(&ShortcutRegistry::from_plugin(&higher).unwrap());
        assert_eq!(registry.all_entries().iter().map(|entry| entry.desc.as_str()).collect::<Vec<_>>(), vec!["New", "Keep"]);
        let ambiguous = parse_plugin_toml(r#"
schema_version=1
id="bad"
name="Bad"
description="Bad"
processes=["x"]
[[bindings]]
keys=["C-k"]
description="Leaf"
category="X"
priority="advanced"
[[bindings]]
keys=["C-k", "C-f"]
description="Path"
category="X"
priority="advanced"
sequence=true
"#, PluginOrigin::BuiltIn).unwrap();
        assert!(ShortcutRegistry::from_plugin(&ambiguous).is_err());
    }
}
