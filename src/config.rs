use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use anyhow::{Context, Result};
use serde::Deserialize;
use crate::types::*;
use crate::registry::ShortcutRegistry;

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
        let shortcut_key = parse_key_string(&key_str)?;
        let child_node = if let Some(group_name) = binding.group {
            // This is a group reference, create a group node
            let mut group_node = Node::new(Some(binding.desc));
            group_node.group_name = Some(group_name);
            group_node
        } else {
            // This is a leaf binding
            Node::new(Some(binding.desc))
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
        let shortcut_key = parse_key_string(&key_str)?;
        let child_node = if let Some(group_name) = binding.group {
            let mut group_node = Node::new(Some(binding.desc));
            group_node.group_name = Some(group_name);
            group_node
        } else {
            Node::new(Some(binding.desc))
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

fn parse_key_string(s: &str) -> Result<ShortcutKey> {
    let parts: Vec<&str> = s.split('-').collect();

    let mut modifiers = ModifierSet::empty();
    let key_part;

    if parts.len() == 1 {
        key_part = parts[0];
    } else {
        // Parse modifiers
        for &part in &parts[..parts.len() - 1] {
            match part.to_lowercase().as_str() {
                "c" | "ctrl" | "control" => modifiers |= ModifierSet::CTRL,
                "a" | "alt" => modifiers |= ModifierSet::ALT,
                "s" | "shift" => modifiers |= ModifierSet::SHIFT,
                "m" | "meta" | "win" => modifiers |= ModifierSet::META,
                _ => anyhow::bail!("Unknown modifier: {}", part),
            }
        }
        key_part = parts[parts.len() - 1];
    }

    // Parse key. Besides letters, support common Windows virtual-key names.
    let key = if key_part.len() == 1 {
        let ch = key_part.chars().next().unwrap();
        Key::from_vk(ch.to_ascii_uppercase() as u32)
    } else {
        let vk = match key_part.to_ascii_lowercase().as_str() {
            "tab" => 0x09,
            "enter" | "return" => 0x0D,
            "esc" | "escape" => 0x1B,
            "space" => 0x20,
            "left" => 0x25,
            "up" => 0x26,
            "right" => 0x27,
            "down" => 0x28,
            "f4" => 0x73,
            "home" => 0x24,
            "end" => 0x23,
            "backspace" => 0x08,
            "delete" | "del" => 0x2E,
            _ => anyhow::bail!("Unsupported key: {}", key_part),
        };
        Key::from_vk(vk)
    };

    Ok(ShortcutKey { modifiers, key })
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
"t" = { desc = "Tools", group = "tools" }

[globals.groups.tools]
"s" = { desc = "Tool status" }
"c" = { desc = "Tool command" }
"#;
        let registry = parse_toml(toml).unwrap();
        let entries = registry.entries_at(&[]);
        assert_eq!(entries.len(), 1);
        assert!(entries[0].is_group);

        let tools_key = ShortcutKey {
            modifiers: ModifierSet::empty(),
            key: Key::T,
        };
        let tools_entries = registry.entries_at(&[tools_key]);
        assert_eq!(tools_entries.len(), 2);
    }

    #[test]
    fn test_parse_nested_group() {
        let toml = r#"
[globals]
"t" = { desc = "Tools", group = "tools" }

[globals.groups.tools]
"d" = { desc = "Tool details", group = "details" }

[globals.groups.tools.groups.details]
"f" = { desc = "Tool file" }
"b" = { desc = "Tool branch" }
"#;
        let registry = parse_toml(toml).unwrap();

        let tools_key = ShortcutKey {
            modifiers: ModifierSet::empty(),
            key: Key::T,
        };
        let tools_entries = registry.entries_at(&[tools_key.clone()]);
        assert_eq!(tools_entries.len(), 1);
        assert!(tools_entries[0].is_group);

        let diff_key = ShortcutKey {
            modifiers: ModifierSet::empty(),
            key: Key::D,
        };
        let diff_entries = registry.entries_at(&[tools_key, diff_key]);
        assert_eq!(diff_entries.len(), 2);
    }

    #[test]
    fn test_parse_named_windows_keys() {
        let toml = r#"
[globals]
"A-Tab" = { desc = "Switch window" }
"A-S-F4" = { desc = "Close window" }
"M-Left" = { desc = "Snap left" }
"#;
        let registry = parse_toml(toml).unwrap();
        let entries = registry.entries_at(&[]);

        assert!(entries.iter().any(|entry| entry.key == "A-Tab"));
        assert!(entries.iter().any(|entry| entry.key == "A-S-F4"));
        assert!(entries.iter().any(|entry| entry.key == "M-Left"));
    }
}
