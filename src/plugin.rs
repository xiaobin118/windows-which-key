use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginWarning {
    pub path: PathBuf,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct PluginSnapshot {
    plugins: Vec<PluginDefinition>,
    process_index: HashMap<String, usize>,
}

#[derive(Debug, Clone)]
pub struct PluginLoadReport {
    pub snapshot: Arc<PluginSnapshot>,
    pub warnings: Vec<PluginWarning>,
}

impl PluginSnapshot {
    pub fn load(built_ins: &[(&str, &str)], user_dir: &Path) -> Result<PluginLoadReport> {
        let mut builtins = Vec::with_capacity(built_ins.len());
        for (name, source) in built_ins {
            builtins.push(parse_plugin_toml(source, PluginOrigin::BuiltIn)
                .with_context(|| format!("invalid built-in plugin: {name}"))?);
        }
        validate_origin_conflicts(&builtins)?;

        let mut user_files = fs::read_dir(user_dir)
            .with_context(|| format!("failed to read user plugin directory: {}", user_dir.display()))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        user_files.retain(|entry| {
            entry.file_type().is_ok_and(|file_type| file_type.is_file())
                && entry.path().extension().is_some_and(|ext| ext.eq_ignore_ascii_case("toml"))
        });
        user_files.sort_by_key(|entry| entry.file_name().to_string_lossy().to_ascii_lowercase());

        let mut warnings = Vec::new();
        let mut users = Vec::new();
        for entry in user_files {
            let path = entry.path();
            match fs::read_to_string(&path).and_then(|source| {
                parse_plugin_toml(&source, PluginOrigin::User(path.clone())).map_err(|error| std::io::Error::other(error.to_string()))
            }) {
                Ok(plugin) => users.push(plugin),
                Err(error) => warnings.push(PluginWarning { path, message: error.to_string() }),
            }
        }
        validate_origin_conflicts(&users)?;
        let plugins = merge_plugins(builtins, users);
        let mut process_index = HashMap::new();
        for (index, plugin) in plugins.iter().enumerate() {
            if !plugin.disabled {
                for process in &plugin.processes { process_index.insert(process.clone(), index); }
            }
        }
        Ok(PluginLoadReport { snapshot: Arc::new(Self { plugins, process_index }), warnings })
    }

    pub fn for_process(&self, normalized_exe: &str) -> Option<&PluginDefinition> {
        self.process_index.get(&normalized_exe.to_ascii_lowercase()).map(|index| &self.plugins[*index])
    }
}

fn validate_origin_conflicts(plugins: &[PluginDefinition]) -> Result<()> {
    let mut claims = HashMap::<&str, &str>::new();
    for plugin in plugins {
        for process in &plugin.processes {
            if let Some(previous) = claims.insert(process, &plugin.id) {
                if previous != &plugin.id { bail!("conflicting plugin IDs {previous} and {} claim process {process}", plugin.id); }
            }
        }
    }
    Ok(())
}

fn merge_plugins(builtins: Vec<PluginDefinition>, users: Vec<PluginDefinition>) -> Vec<PluginDefinition> {
    let mut merged = builtins;
    for user in users {
        if let Some(existing) = merged.iter_mut().find(|plugin| plugin.id == user.id) {
            let mut bindings = existing.bindings.clone();
            for binding in user.bindings {
                if let Some(index) = bindings.iter().position(|old| same_binding_keys(&old.keys, &binding.keys)) { bindings[index] = binding; } else { bindings.push(binding); }
            }
            *existing = PluginDefinition { bindings, ..user };
        } else {
            merged.push(user);
        }
    }
    merged
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
    use std::fs;
    use tempfile::tempdir;

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

    #[test]
    fn user_plugin_overrides_binding_and_retains_unmentioned_builtin_binding() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("override.toml"), r#"
schema_version = 1
id = "VSCode"
name = "User VS Code"
description = "Override"
processes = ["code.exe"]
[[bindings]]
keys = ["C-S-p", "F1"]
description = "User command palette"
category = "User"
priority = "recommended"
"#).unwrap();
        let report = PluginSnapshot::load(&[("builtin.toml", VALID)], dir.path()).unwrap();
        let plugin = report.snapshot.for_process("CoDe.ExE").unwrap();
        assert_eq!(plugin.name, "User VS Code");
        assert_eq!(plugin.bindings.len(), 2);
        assert_eq!(plugin.bindings[0].description, "User command palette");
    }

    #[test]
    fn disabled_user_plugin_removes_builtin_from_process_index() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("disable.toml"), r#"
schema_version = 1
id = "vscode"
name = "Disabled"
description = "Disabled"
processes = ["code.exe"]
disabled = true
"#).unwrap();
        let report = PluginSnapshot::load(&[("builtin.toml", VALID)], dir.path()).unwrap();
        assert!(report.snapshot.for_process("code.exe").is_none());
    }

    #[test]
    fn invalid_user_plugin_is_warning_and_builtin_remains() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("bad.toml"), "not valid toml = [").unwrap();
        let report = PluginSnapshot::load(&[("builtin.toml", VALID)], dir.path()).unwrap();
        assert_eq!(report.warnings.len(), 1);
        assert!(report.snapshot.for_process("code.exe").is_some());
    }

    #[test]
    fn conflicting_ids_for_process_in_same_origin_are_fatal() {
        let first = VALID.replace("id = \"VSCode\"", "id = \"one\"");
        let second = VALID.replace("id = \"VSCode\"", "id = \"two\"");
        let error = PluginSnapshot::load(&[("one.toml", &first), ("two.toml", &second)], tempdir().unwrap().path()).unwrap_err();
        let message = error.to_string();
        assert!(message.contains("one") && message.contains("two") && message.contains("code.exe"));
    }

    #[test]
    fn user_discovery_ignores_directories_and_nested_toml_files() {
        let dir = tempdir().unwrap();
        fs::create_dir(dir.path().join("ignored.toml")).unwrap();
        let nested = dir.path().join("nested");
        fs::create_dir(&nested).unwrap();
        fs::write(nested.join("plugin.toml"), VALID).unwrap();
        let report = PluginSnapshot::load(&[], dir.path()).unwrap();
        assert!(report.snapshot.for_process("code.exe").is_none());
        assert!(report.warnings.is_empty());
    }

    #[test]
    fn user_files_are_sorted_case_insensitively_before_same_id_merge() {
        let dir = tempdir().unwrap();
        let first = VALID.replace("id = \"VSCode\"", "id = \"same\"").replace("description = \"Command palette\"", "description = \"A first\"");
        let last = first.replace("description = \"A first\"", "description = \"Z last\"");
        fs::write(dir.path().join("z.toml"), last).unwrap();
        fs::write(dir.path().join("A.toml"), first).unwrap();
        let report = PluginSnapshot::load(&[], dir.path()).unwrap();
        assert_eq!(report.snapshot.for_process("code.exe").unwrap().bindings[0].description, "Z last");
    }

    #[test]
    fn different_user_ids_claiming_one_process_are_fatal() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("one.toml"), VALID.replace("id = \"VSCode\"", "id = \"one\"")).unwrap();
        fs::write(dir.path().join("two.toml"), VALID.replace("id = \"VSCode\"", "id = \"two\"")).unwrap();
        let error = PluginSnapshot::load(&[], dir.path()).unwrap_err();
        let message = error.to_string();
        assert!(message.contains("one") && message.contains("two") && message.contains("code.exe"));
    }

    #[test]
    fn user_only_plugin_is_indexed() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("user.toml"), VALID).unwrap();
        assert!(PluginSnapshot::load(&[], dir.path()).unwrap().snapshot.for_process("code.exe").is_some());
    }

    #[test]
    fn disabled_user_only_plugin_is_absent_from_index() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("disabled.toml"), VALID.replace("processes = [\"Code.exe\", \"CODE-INSIDERS.EXE\"]", "processes = [\"code.exe\"]\ndisabled = true")).unwrap();
        assert!(PluginSnapshot::load(&[], dir.path()).unwrap().snapshot.for_process("code.exe").is_none());
    }
}
