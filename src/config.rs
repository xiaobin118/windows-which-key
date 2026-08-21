use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use anyhow::{Context, Result};
use serde::Deserialize;
use crate::types::*;
use crate::registry::ShortcutRegistry;
use crate::shortcut::parse_shortcut;

#[derive(Debug, Deserialize)]
struct RawConfig {
    globals: Option<RawKeymap>,
    applications: Option<HashMap<String, RawKeymap>>,
}

#[derive(Debug, Deserialize)]
struct RawKeymap {
    #[serde(flatten)]
    bindings: HashMap<String, RawBinding>,
    groups: Option<HashMap<String, RawGroup>>,
}

#[derive(Debug, Deserialize)]
struct RawBinding {
    desc: String,
    group: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawGroup {
    #[serde(flatten)]
    bindings: HashMap<String, RawBinding>,
    groups: Option<HashMap<String, RawGroup>>,
}

pub struct Config {
    pub registry: ShortcutRegistry,
    path: PathBuf,
}

impl Config {
    pub fn load(path: PathBuf) -> Result<Self> {
        let content = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read config file: {:?}", path))?;
        let registry = parse_toml(&content)?;
        Ok(Config { registry, path })
    }

    pub fn reload(&mut self) -> Result<()> {
        let content = fs::read_to_string(&self.path)
            .with_context(|| format!("Failed to read config file: {:?}", self.path))?;
        self.registry = parse_toml(&content)?;
        Ok(())
    }
}

pub fn parse_toml(content: &str) -> Result<ShortcutRegistry> {
    let raw: RawConfig = toml::from_str(content)
        .context("Failed to parse TOML")?;

    let globals = if let Some(globals) = raw.globals {
        build_node_from_keymap(globals)?
    } else {
        Node::new(None)
    };

    let mut applications = HashMap::new();
    if let Some(apps) = raw.applications {
        for (app_name, keymap) in apps {
            applications.insert(app_name, build_node_from_keymap(keymap)?);
        }
    }

    Ok(ShortcutRegistry { globals, applications })
}

fn build_node_from_keymap(keymap: RawKeymap) -> Result<Node> {
    let mut node = Node::new(None);

    // Add direct bindings
    for (key_str, binding) in keymap.bindings {
        let shortcut_key = parse_shortcut(&key_str)?;
        let child_node = if let Some(group_name) = binding.group {
            // This is a group reference, create a group node
            let mut group_node = Node::new(Some(binding.desc));
            group_node.group_name = Some(group_name);
            group_node
        } else {
            // This is a leaf binding
            Node::new_binding(binding.desc, BindingMetadata { category: "Windows".into(), priority: BindingPriority::Recommended })
        };
        node.children.insert(shortcut_key, child_node);
    }

    // Add nested groups
    if let Some(groups) = keymap.groups {
        for (group_name, group_data) in groups {
            let group_node = build_node_from_group(group_name.clone(), group_data)?;
            // Find the group reference in children and merge
            for (_key, child) in node.children.iter_mut() {
                if child.group_name.as_ref() == Some(&group_name) {
                    // Merge the group's children into this node
                    for (k, v) in group_node.children {
                        child.children.insert(k, v);
                    }
                    break;
                }
            }
        }
    }

    Ok(node)
}

fn build_node_from_group(name: String, group: RawGroup) -> Result<Node> {
    let mut node = Node::new(Some(name));

    // Add direct bindings
    for (key_str, binding) in group.bindings {
        let shortcut_key = parse_shortcut(&key_str)?;
        let child_node = if let Some(group_name) = binding.group {
            let mut group_node = Node::new(Some(binding.desc));
            group_node.group_name = Some(group_name);
            group_node
        } else {
            Node::new_binding(binding.desc, BindingMetadata { category: "Windows".into(), priority: BindingPriority::Recommended })
        };
        node.children.insert(shortcut_key, child_node);
    }

    // Add nested groups
    if let Some(groups) = group.groups {
        for (group_name, group_data) in groups {
            let group_node = build_node_from_group(group_name.clone(), group_data)?;
            for (_key, child) in node.children.iter_mut() {
                if child.group_name.as_ref() == Some(&group_name) {
                    for (k, v) in group_node.children {
                        child.children.insert(k, v);
                    }
                    break;
                }
            }
        }
    }

    Ok(node)
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_binding() {
        let toml = r#"
[globals]
"C-c" = { desc = "Copy" }
"C-v" = { desc = "Paste" }
"#;
        let registry = parse_toml(toml).unwrap();
        let entries = registry.entries_at(&[]);
        assert_eq!(entries.len(), 2);

        let copy = entries.iter().find(|e| e.desc == "Copy").unwrap();
        assert_eq!(copy.key, "C-c");
        assert!(!copy.is_group);
    }

    #[test]
    fn test_parse_group() {
        let toml = r#"
[globals]
"g" = { desc = "Git", group = "git" }

[globals.groups.git]
"s" = { desc = "Git status" }
"c" = { desc = "Git commit" }
"#;
        let registry = parse_toml(toml).unwrap();
        let entries = registry.entries_at(&[]);
        assert_eq!(entries.len(), 1);
        assert!(entries[0].is_group);

        let git_key = ShortcutKey {
            modifiers: ModifierSet::empty(),
            key: Key::G,
        };
        let git_entries = registry.entries_at(&[git_key]);
        assert_eq!(git_entries.len(), 2);
    }

    #[test]
    fn test_parse_nested_group() {
        let toml = r#"
[globals]
"g" = { desc = "Git", group = "git" }

[globals.groups.git]
"d" = { desc = "Diff", group = "diff" }

[globals.groups.git.groups.diff]
"f" = { desc = "Diff file" }
"b" = { desc = "Diff branch" }
"#;
        let registry = parse_toml(toml).unwrap();

        let git_key = ShortcutKey {
            modifiers: ModifierSet::empty(),
            key: Key::G,
        };
        let git_entries = registry.entries_at(&[git_key.clone()]);
        assert_eq!(git_entries.len(), 1);
        assert!(git_entries[0].is_group);

        let diff_key = ShortcutKey {
            modifiers: ModifierSet::empty(),
            key: Key::D,
        };
        let diff_entries = registry.entries_at(&[git_key, diff_key]);
        assert_eq!(diff_entries.len(), 2);
    }
}
