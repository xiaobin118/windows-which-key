use std::collections::HashSet;
use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use serde::Deserialize;

use crate::shortcut::{format_shortcut, parse_shortcut};
use crate::types::{BindingMetadata, BindingPriority, ShortcutKey};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginOrigin {
    BuiltIn,
    User(PathBuf),
}

#[derive(Debug, Clone)]
pub struct PluginDefinition {
    pub id: String,
    pub name: String,
    pub processes: Vec<String>,
    pub bindings: Vec<PluginBinding>,
    pub origin: PluginOrigin,
}

#[derive(Debug, Clone)]
pub enum BindingKeys {
    Alternatives(Vec<ShortcutKey>),
    Sequence(Vec<ShortcutKey>),
}

#[derive(Debug, Clone)]
pub struct PluginBinding {
    pub keys: BindingKeys,
    pub description: String,
    pub metadata: BindingMetadata,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawPluginDefinition {
    schema_version: u32,
    id: String,
    name: String,
    processes: Vec<String>,
    #[serde(default)]
    bindings: Vec<RawPluginBinding>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawPluginBinding {
    keys: Vec<String>,
    description: String,
    category: String,
    priority: BindingPriority,
    #[serde(default)]
    sequence: bool,
}

pub fn parse_plugin_toml(source: &str, origin: PluginOrigin) -> Result<PluginDefinition> {
    let raw: RawPluginDefinition = toml::from_str(source).context("Invalid plugin TOML")?;
    if raw.schema_version != 1 {
        bail!("Unsupported plugin schema version: {}", raw.schema_version);
    }

    let id = normalized_required("id", raw.id)?.to_ascii_lowercase();
    let name = normalized_required("name", raw.name)?;
    if raw.processes.is_empty() {
        bail!("Plugin processes cannot be empty");
    }
    let processes = raw
        .processes
        .into_iter()
        .map(|process| normalized_required("process", process).map(|value| value.to_lowercase()))
        .collect::<Result<Vec<_>>>()?;

    let mut seen_bindings = HashSet::new();
    let bindings = raw
        .bindings
        .into_iter()
        .map(|binding| parse_binding(binding, &mut seen_bindings))
        .collect::<Result<Vec<_>>>()?;

    Ok(PluginDefinition {
        id,
        name,
        processes,
        bindings,
        origin,
    })
}

fn parse_binding(raw: RawPluginBinding, seen_bindings: &mut HashSet<String>) -> Result<PluginBinding> {
    if raw.keys.is_empty() {
        bail!("Binding keys cannot be empty");
    }

    let keys = raw
        .keys
        .into_iter()
        .map(|key| parse_shortcut(&key).with_context(|| format!("Invalid binding shortcut: {key}")))
        .collect::<Result<Vec<_>>>()?;
    let description = normalized_required("binding description", raw.description)?;
    let category = normalized_required("binding category", raw.category)?;

    let identity = binding_identity(&keys, raw.sequence);
    if !seen_bindings.insert(identity) {
        bail!("Duplicate normalized binding");
    }

    let keys = if raw.sequence {
        BindingKeys::Sequence(keys)
    } else {
        BindingKeys::Alternatives(keys)
    };
    Ok(PluginBinding {
        keys,
        description,
        metadata: BindingMetadata {
            category,
            priority: raw.priority,
        },
    })
}

fn normalized_required(field: &str, value: String) -> Result<String> {
    let value = value.trim().to_string();
    if value.is_empty() {
        bail!("Plugin {field} cannot be blank");
    }
    Ok(value)
}

fn binding_identity(keys: &[ShortcutKey], sequence: bool) -> String {
    let mut normalized = keys.iter().map(format_shortcut).collect::<Vec<_>>();
    if !sequence {
        normalized.sort_unstable();
    }
    format!("{}:{}", if sequence { "sequence" } else { "alternatives" }, normalized.join(","))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_alternatives_and_sequences() {
        let source = r#"
schema_version = 1
id = "vscode"
name = "Visual Studio Code"
processes = ["Code.exe"]

[[bindings]]
keys = ["C-S-p", "F1"]
description = "Command Palette"
category = "Common"
priority = "essential"

[[bindings]]
keys = ["C-k", "C-f"]
description = "Format Selection"
category = "Editing"
priority = "recommended"
sequence = true
"#;

        let plugin = parse_plugin_toml(source, PluginOrigin::BuiltIn).unwrap();
        assert_eq!(plugin.id, "vscode");
        assert_eq!(plugin.processes, vec!["code.exe"]);
        assert_eq!(plugin.bindings.len(), 2);
        assert!(matches!(plugin.bindings[0].keys, BindingKeys::Alternatives(_)));
        assert!(matches!(plugin.bindings[1].keys, BindingKeys::Sequence(_)));
    }

    #[test]
    fn rejects_unsupported_schema_and_empty_fields() {
        let bad_version = "schema_version=2\nid='x'\nname='X'\nprocesses=['x.exe']";
        assert!(parse_plugin_toml(bad_version, PluginOrigin::BuiltIn).is_err());

        let empty_keys = r#"
schema_version=1
id="x"
name="X"
processes=["x.exe"]
[[bindings]]
keys=[]
description="Missing"
category="Common"
priority="essential"
"#;
        assert!(parse_plugin_toml(empty_keys, PluginOrigin::BuiltIn).is_err());
    }

    #[test]
    fn normalizes_identifiers_and_rejects_duplicate_bindings() {
        let source = r#"
schema_version = 1
id = "VSCode"
name = "Visual Studio Code"
processes = ["Code.EXE", "ÄPP.EXE"]

[[bindings]]
keys = ["Ctrl-P"]
description = "Open"
category = "Common"
priority = "essential"

[[bindings]]
keys = ["C-p"]
description = "Open again"
category = "Common"
priority = "essential"
"#;

        assert!(parse_plugin_toml(source, PluginOrigin::BuiltIn).is_err());
    }
}
