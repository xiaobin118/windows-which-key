use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use serde::Deserialize;

use crate::shortcut::parse_shortcut;
use crate::types::{BindingMetadata, BindingPriority, ShortcutKey};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginOrigin {
    BuiltIn,
    User(PathBuf),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BindingKeys {
    Alternatives(Vec<ShortcutKey>),
    Sequence(Vec<ShortcutKey>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginBinding {
    pub keys: BindingKeys,
    pub description: String,
    pub metadata: BindingMetadata,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginDefinition {
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    pub description: String,
    pub processes: Vec<String>,
    pub disabled: bool,
    pub bindings: Vec<PluginBinding>,
    pub origin: PluginOrigin,
}

#[derive(Deserialize)]
struct RawPlugin {
    schema_version: u32,
    id: String,
    name: String,
    description: String,
    processes: Vec<String>,
    #[serde(default)]
    disabled: bool,
    #[serde(default)]
    bindings: Vec<RawBinding>,
}

#[derive(Deserialize)]
struct RawBinding {
    keys: Vec<String>,
    description: String,
    category: String,
    priority: String,
    #[serde(default)]
    sequence: bool,
}

pub fn parse_plugin_toml(source: &str, origin: PluginOrigin) -> Result<PluginDefinition> {
    let raw: RawPlugin = toml::from_str(source).context("invalid plugin TOML")?;
    if raw.schema_version != 1 {
        bail!("unsupported plugin schema version: {}", raw.schema_version);
    }
    let id = require_text("id", raw.id)?.to_ascii_lowercase();
    let name = require_text("name", raw.name)?;
    let description = require_text("description", raw.description)?;
    if raw.processes.is_empty() {
        bail!("processes must not be empty");
    }
    let processes = raw.processes.into_iter().map(|p| require_text("process", p).map(|p| p.to_ascii_lowercase())).collect::<Result<Vec<_>>>()?;

    let mut bindings = Vec::with_capacity(raw.bindings.len());
    for raw_binding in raw.bindings {
        if raw_binding.keys.is_empty() {
            bail!("binding keys must not be empty");
        }
        let keys = raw_binding.keys.into_iter().map(|key| parse_shortcut(&key).with_context(|| format!("invalid shortcut: {key}"))).collect::<Result<Vec<_>>>()?;
        let binding_keys = if raw_binding.sequence { BindingKeys::Sequence(keys) } else { BindingKeys::Alternatives(keys) };
        let binding = PluginBinding {
            keys: binding_keys,
            description: require_text("binding description", raw_binding.description)?,
            metadata: BindingMetadata { category: require_text("category", raw_binding.category)?, priority: parse_priority(&raw_binding.priority)? },
        };
        if bindings.iter().any(|existing| same_binding_keys(&existing.keys, &binding.keys)) {
            bail!("duplicate normalized binding");
        }
        bindings.push(binding);
    }

    Ok(PluginDefinition { schema_version: 1, id, name, description, processes, disabled: raw.disabled, bindings, origin })
}

fn require_text(field: &str, value: String) -> Result<String> {
    if value.trim().is_empty() { bail!("{field} must not be blank"); }
    Ok(value)
}

fn parse_priority(value: &str) -> Result<BindingPriority> {
    match value {
        "essential" => Ok(BindingPriority::Essential),
        "recommended" => Ok(BindingPriority::Recommended),
        "advanced" => Ok(BindingPriority::Advanced),
        _ => bail!("invalid binding priority: {value}"),
    }
}

fn same_binding_keys(left: &BindingKeys, right: &BindingKeys) -> bool {
    match (left, right) {
        (BindingKeys::Sequence(a), BindingKeys::Sequence(b)) => a == b,
        (BindingKeys::Alternatives(a), BindingKeys::Alternatives(b)) => a.len() == b.len() && a.iter().all(|key| b.contains(key)),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID: &str = r#"
schema_version = 1
id = "VSCode"
name = "Visual Studio Code"
description = "Editor shortcuts"
processes = ["Code.exe", "CODE-INSIDERS.EXE"]

[[bindings]]
keys = ["C-S-p", "F1"]
description = "Command palette"
category = "Navigation"
priority = "essential"

[[bindings]]
keys = ["C-k", "C-f"]
description = "Format selection"
category = "Editing"
priority = "recommended"
sequence = true
"#;

    #[test]
    fn parses_and_normalizes_plugin_and_bindings() {
        let plugin = parse_plugin_toml(VALID, PluginOrigin::BuiltIn).unwrap();
        assert_eq!(plugin.id, "vscode");
        assert_eq!(plugin.processes, vec!["code.exe", "code-insiders.exe"]);
        assert_eq!(plugin.bindings.len(), 2);
        assert!(matches!(plugin.bindings[0].keys, BindingKeys::Alternatives(ref keys) if keys.len() == 2));
        assert!(matches!(plugin.bindings[1].keys, BindingKeys::Sequence(ref keys) if keys.len() == 2));
        assert_eq!(plugin.bindings[0].metadata.category, "Navigation");
        assert_eq!(plugin.origin, PluginOrigin::BuiltIn);
    }

    #[test]
    fn rejects_invalid_plugin_values() {
        for (field, value) in [
            ("schema_version", "2"),
            ("id", "\"  \""),
            ("name", "\"\""),
            ("description", "\" \""),
            ("processes", "[]"),
        ] {
            let source = format!("schema_version = 1\nid = \"id\"\nname = \"name\"\ndescription = \"desc\"\nprocesses = [\"app.exe\"]\n{field} = {value}");
            assert!(parse_plugin_toml(&source, PluginOrigin::BuiltIn).is_err(), "{field}");
        }
    }

    #[test]
    fn rejects_invalid_bindings_and_duplicates() {
        for priority in ["invalid", "", "ESSENTIAL"] {
            let source = VALID.replace("priority = \"essential\"", &format!("priority = \"{priority}\""));
            assert!(parse_plugin_toml(&source, PluginOrigin::BuiltIn).is_err());
        }
        let duplicate = VALID.replace("keys = [\"C-k\", \"C-f\"]", "keys = [\"C-S-p\"]");
        assert!(parse_plugin_toml(&duplicate, PluginOrigin::BuiltIn).is_err());
        let empty = VALID.replace("keys = [\"C-S-p\", \"F1\"]", "keys = []");
        assert!(parse_plugin_toml(&empty, PluginOrigin::BuiltIn).is_err());
        let malformed = VALID.replace("keys = [\"C-S-p\", \"F1\"]", "keys = [\"Ctrl-\"]");
        assert!(parse_plugin_toml(&malformed, PluginOrigin::BuiltIn).is_err());
    }
}
