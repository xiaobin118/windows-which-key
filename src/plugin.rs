use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Deserializer, Serialize};

use crate::shortcut::{format_shortcut, parse_shortcut};
use crate::snapshot_fingerprint::{content_fingerprint, source_fingerprint};
use crate::types::{BindingMetadata, BindingPriority, ShortcutKey};

pub const BUILTIN_PLUGINS: &[(&str, &str)] = &[
    ("codex.toml", include_str!("../plugins/builtin/codex.toml")),
    (
        "codex-desktop.toml",
        include_str!("../plugins/builtin/codex-desktop.toml"),
    ),
    (
        "claude.toml",
        include_str!("../plugins/builtin/claude.toml"),
    ),
    (
        "vscode.toml",
        include_str!("../plugins/builtin/vscode.toml"),
    ),
    (
        "browser.toml",
        include_str!("../plugins/builtin/browser.toml"),
    ),
    (
        "windows-terminal.toml",
        include_str!("../plugins/builtin/windows-terminal.toml"),
    ),
    ("word.toml", include_str!("../plugins/builtin/word.toml")),
    ("excel.toml", include_str!("../plugins/builtin/excel.toml")),
    (
        "powerpoint.toml",
        include_str!("../plugins/builtin/powerpoint.toml"),
    ),
];

/// A validated, normalized plugin identifier used at resource boundaries.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct PluginId(String);

impl PluginId {
    pub fn parse(value: impl Into<String>) -> Result<Self> {
        Ok(Self(
            normalized_required("id", value.into())?.to_ascii_lowercase(),
        ))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for PluginId {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Self::parse(String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod plugin_id_tests {
    use super::PluginId;

    #[test]
    fn deserialization_normalizes_plugin_id_casing() {
        let id: PluginId = serde_json::from_str("\"Editor\"").unwrap();
        assert_eq!(id, PluginId::parse("editor").unwrap());
    }

    #[test]
    fn deserialization_rejects_an_empty_plugin_id() {
        assert!(serde_json::from_str::<PluginId>("\"   \\t\"").is_err());
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginOrigin {
    BuiltIn,
    User(PathBuf),
}

#[derive(Debug, Clone)]
pub struct PluginDefinition {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub processes: Vec<String>,
    pub bindings: Vec<PluginBinding>,
    pub origin: PluginOrigin,
    pub disabled: bool,
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
    description: Option<String>,
    processes: Vec<String>,
    #[serde(default)]
    bindings: Vec<RawPluginBinding>,
    #[serde(default)]
    disabled: bool,
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

    let id = PluginId::parse(raw.id)?.0;
    let name = normalized_required("name", raw.name)?;
    let description = raw
        .description
        .map(|description| normalized_required("description", description))
        .transpose()?;
    if raw.processes.is_empty() {
        bail!("Plugin processes cannot be empty");
    }
    let processes = raw
        .processes
        .into_iter()
        .map(|process| normalized_required("process", process).map(|value| value.to_lowercase()))
        .collect::<Result<Vec<_>>>()?;
    if raw.bindings.is_empty() && !raw.disabled {
        bail!("Plugin bindings cannot be empty");
    }

    let mut seen_bindings = HashSet::new();
    let bindings = raw
        .bindings
        .into_iter()
        .map(|binding| parse_binding(binding, &mut seen_bindings))
        .collect::<Result<Vec<_>>>()?;

    Ok(PluginDefinition {
        id,
        name,
        description,
        processes,
        bindings,
        origin,
        disabled: raw.disabled,
    })
}

impl PluginDefinition {
    pub fn binding_description(&self, shortcut: &str) -> Option<&str> {
        let shortcut = parse_shortcut(shortcut).ok()?;
        let identity = binding_identity(&[shortcut], false);
        self.bindings
            .iter()
            .find(|binding| binding_identity_from_binding(binding) == identity)
            .map(|binding| binding.description.as_str())
    }
}

#[derive(Debug, Clone)]
pub struct PluginWarning {
    pub path: PathBuf,
    pub message: String,
}

#[derive(Debug)]
pub struct PluginLoadReport {
    pub snapshot: Arc<PluginSnapshot>,
    pub catalog: Arc<PluginCatalog>,
    pub warnings: Vec<PluginWarning>,
}

#[derive(Debug, Default)]
pub struct PluginSnapshot {
    plugins_by_process: HashMap<String, PluginDefinition>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PluginCatalogEntry {
    pub file: Option<String>,
    pub id: String,
    pub name: String,
    pub processes: Vec<String>,
    pub bindings: usize,
    pub disabled: bool,
    pub origin: String,
    pub valid: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PluginCatalog {
    pub built_in: Vec<PluginCatalogEntry>,
    pub user: Vec<PluginCatalogEntry>,
    pub dir: PathBuf,
}

#[derive(Clone)]
enum CachedPluginResult {
    Valid(PluginDefinition),
    Invalid(String),
}

#[derive(Clone)]
struct CachedUserPlugin {
    fingerprint: u64,
    result: CachedPluginResult,
}

#[derive(Default)]
struct PluginLoadCache {
    builtins: HashMap<u64, BTreeMap<String, PluginDefinition>>,
    user_plugins: HashMap<PathBuf, CachedUserPlugin>,
}

static PLUGIN_LOAD_CACHE: OnceLock<Mutex<PluginLoadCache>> = OnceLock::new();

impl PluginSnapshot {
    pub fn load(built_ins: &[(&str, &str)], user_dir: &Path) -> Result<PluginLoadReport> {
        let built_in_plugins = load_builtin_plugins(built_ins)?;
        let catalog_built_in = built_in_plugins
            .values()
            .map(|plugin| plugin_catalog_entry_from_plugin(&Path::new(""), plugin, false, None))
            .collect();

        let mut warnings = Vec::new();
        let mut user_paths = match std::fs::read_dir(user_dir) {
            Ok(entries) => entries
                .filter_map(|entry| entry.ok())
                .map(|entry| entry.path())
                .filter(|path| path.is_file() && has_toml_extension(path))
                .collect::<Vec<_>>(),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(error) => {
                warnings.push(PluginWarning {
                    path: user_dir.to_path_buf(),
                    message: format!("Unable to read plugin directory: {error}"),
                });
                Vec::new()
            }
        };
        sort_plugin_paths(&mut user_paths);

        let mut user_plugins = Vec::new();
        let mut catalog_user = Vec::new();
        for path in user_paths {
            match load_user_plugin(&path) {
                Ok(Some(plugin)) => {
                    catalog_user.push(plugin_catalog_entry_from_plugin(&path, &plugin, true, None));
                    user_plugins.push(plugin);
                }
                Ok(None) => {}
                Err(error) => warnings.push(PluginWarning {
                    path,
                    message: error.to_string(),
                }),
            }
        }
        for warning in &warnings {
            if warning.path.extension().and_then(|ext| ext.to_str()).is_some_and(|ext| ext.eq_ignore_ascii_case("toml")) {
                catalog_user.push(PluginCatalogEntry {
                    file: warning.path.file_name().map(|name| name.to_string_lossy().into_owned()),
                    id: warning
                        .path
                        .file_stem()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_ascii_lowercase(),
                    name: warning
                        .path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned(),
                    processes: vec![],
                    bindings: 0,
                    disabled: false,
                    origin: "user".to_string(),
                    valid: false,
                    error: Some(warning.message.clone()),
                });
            }
        }
        let user_plugins = merge_origin_plugins(user_plugins, "user")?;

        let mut plugins = built_in_plugins;
        for (id, user_plugin) in user_plugins {
            if user_plugin.disabled {
                plugins.remove(&id);
            } else if let Some(built_in_plugin) = plugins.get(&id) {
                plugins.insert(id, merge_plugin(built_in_plugin, user_plugin));
            } else {
                plugins.insert(id, user_plugin);
            }
        }

        let mut plugins_by_process: HashMap<String, PluginDefinition> = HashMap::new();
        for plugin in plugins.into_values() {
            for process in &plugin.processes {
                match plugins_by_process.get(process) {
                    Some(existing)
                        if origin_rank(&existing.origin) > origin_rank(&plugin.origin) => {}
                    _ => {
                        plugins_by_process.insert(process.clone(), plugin.clone());
                    }
                }
            }
        }

        Ok(PluginLoadReport {
            snapshot: Arc::new(Self { plugins_by_process }),
            catalog: Arc::new(PluginCatalog {
                built_in: catalog_built_in,
                user: catalog_user,
                dir: user_dir.to_path_buf(),
            }),
            warnings,
        })
    }

    pub fn for_process(&self, normalized_exe: &str) -> Option<&PluginDefinition> {
        self.plugins_by_process.get(&normalized_exe.to_lowercase())
    }
}

pub(crate) fn load_builtin_plugins(
    built_ins: &[(&str, &str)],
) -> Result<BTreeMap<String, PluginDefinition>> {
    let fingerprint = source_fingerprint(built_ins.iter().map(|(name, source)| (*name, *source)));
    {
        let cache = plugin_load_cache().lock().expect("plugin cache poisoned");
        if let Some(plugins) = cache.builtins.get(&fingerprint) {
            return Ok(plugins.clone());
        }
    }

    let mut built_in_plugins = Vec::with_capacity(built_ins.len());
    for (name, source) in built_ins {
        built_in_plugins.push(
            parse_plugin_toml(source, PluginOrigin::BuiltIn)
                .with_context(|| format!("Invalid built-in plugin {name}"))?,
        );
    }
    let built_in_plugins = merge_origin_plugins(built_in_plugins, "built-in")?;

    let mut cache = plugin_load_cache().lock().expect("plugin cache poisoned");
    cache.builtins.insert(fingerprint, built_in_plugins.clone());
    Ok(built_in_plugins)
}

pub(crate) fn load_user_plugin(path: &Path) -> Result<Option<PluginDefinition>> {
    let source = std::fs::read_to_string(path)
        .with_context(|| format!("Unable to read user plugin {}", path.display()))?;
    let fingerprint = content_fingerprint(&source);

    {
        let cache = plugin_load_cache().lock().expect("plugin cache poisoned");
        if let Some(cached) = cache.user_plugins.get(path) {
            if cached.fingerprint == fingerprint {
                return match &cached.result {
                    CachedPluginResult::Valid(plugin) => Ok(Some(plugin.clone())),
                    CachedPluginResult::Invalid(message) => Err(anyhow!(message.clone())),
                };
            }
        }
    }

    let result = match parse_plugin_toml(&source, PluginOrigin::User(path.to_path_buf())) {
        Ok(plugin) => Ok(Some(plugin.clone())),
        Err(error) => Err(error),
    };

    let cached = CachedUserPlugin {
        fingerprint,
        result: match &result {
            Ok(Some(plugin)) => CachedPluginResult::Valid(plugin.clone()),
            Ok(None) => unreachable!(),
            Err(error) => CachedPluginResult::Invalid(error.to_string()),
        },
    };

    let mut cache = plugin_load_cache().lock().expect("plugin cache poisoned");
    cache.user_plugins.insert(path.to_path_buf(), cached);

    result
}

fn plugin_load_cache() -> &'static Mutex<PluginLoadCache> {
    PLUGIN_LOAD_CACHE.get_or_init(|| Mutex::new(PluginLoadCache::default()))
}

fn plugin_catalog_entry_from_plugin(
    path: &Path,
    plugin: &PluginDefinition,
    valid: bool,
    error: Option<String>,
) -> PluginCatalogEntry {
    PluginCatalogEntry {
        file: path.file_name().map(|name| name.to_string_lossy().into_owned()),
        id: plugin.id.clone(),
        name: plugin.name.clone(),
        processes: plugin.processes.clone(),
        bindings: plugin.bindings.len(),
        disabled: plugin.disabled,
        origin: match plugin.origin {
            PluginOrigin::BuiltIn => "builtIn".to_string(),
            PluginOrigin::User(_) => "user".to_string(),
        },
        valid,
        error,
    }
}

fn has_toml_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("toml"))
}

fn sort_plugin_paths(paths: &mut [PathBuf]) {
    paths.sort_by(|left, right| {
        left.to_string_lossy()
            .to_ascii_lowercase()
            .cmp(&right.to_string_lossy().to_ascii_lowercase())
            .then_with(|| left.cmp(right))
    });
}

fn merge_origin_plugins(
    plugins: Vec<PluginDefinition>,
    origin_name: &str,
) -> Result<BTreeMap<String, PluginDefinition>> {
    let mut merged = BTreeMap::new();
    let mut processes = HashMap::<String, String>::new();
    for plugin in plugins {
        for process in &plugin.processes {
            if let Some(existing_id) = processes.insert(process.clone(), plugin.id.clone()) {
                if existing_id != plugin.id {
                    bail!(
                        "Conflicting {origin_name} plugins {existing_id} and {} for process {process}",
                        plugin.id
                    );
                }
            }
        }
        match merged.remove(&plugin.id) {
            Some(existing) => {
                merged.insert(plugin.id.clone(), merge_plugin(&existing, plugin));
            }
            None => {
                merged.insert(plugin.id.clone(), plugin);
            }
        }
    }
    Ok(merged)
}

fn merge_plugin(base: &PluginDefinition, mut overlay: PluginDefinition) -> PluginDefinition {
    let mut bindings = base.bindings.clone();
    for binding in overlay.bindings.drain(..) {
        let identity = binding_identity_from_binding(&binding);
        if let Some(index) = bindings
            .iter()
            .position(|existing| binding_identity_from_binding(existing) == identity)
        {
            bindings[index] = binding;
        } else {
            bindings.push(binding);
        }
    }
    overlay.bindings = bindings;
    overlay
}

fn binding_identity_from_binding(binding: &PluginBinding) -> String {
    match &binding.keys {
        BindingKeys::Alternatives(keys) => binding_identity(keys, false),
        BindingKeys::Sequence(keys) => binding_identity(keys, true),
    }
}

fn origin_rank(origin: &PluginOrigin) -> u8 {
    match origin {
        PluginOrigin::BuiltIn => 0,
        PluginOrigin::User(_) => 1,
    }
}

fn parse_binding(
    raw: RawPluginBinding,
    seen_bindings: &mut HashSet<String>,
) -> Result<PluginBinding> {
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
    format!(
        "{}:{}",
        if sequence { "sequence" } else { "alternatives" },
        normalized.join(",")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    const BUILTIN_VSCODE: &str = r#"
schema_version = 1
id = "vscode"
name = "Visual Studio Code"
processes = ["Code.exe"]

[[bindings]]
keys = ["C-p"]
description = "Quick Open"
category = "Common"
priority = "essential"

[[bindings]]
keys = ["C-S-p"]
description = "Command Palette"
category = "Common"
priority = "essential"
"#;

    const USER_VSCODE: &str = r#"
schema_version = 1
id = "vscode"
name = "My VS Code"
processes = ["Code.exe"]

[[bindings]]
keys = ["C-p"]
description = "My Quick Open"
category = "Common"
priority = "essential"
"#;

    const DISABLED_VSCODE: &str = r#"
schema_version = 1
id = "vscode"
name = "Visual Studio Code"
processes = ["Code.exe"]
disabled = true
"#;

    const PLUGIN_ONE: &str = r#"
schema_version = 1
id = "one"
name = "One"
processes = ["shared.exe"]

[[bindings]]
keys = ["C-p"]
description = "One"
category = "Common"
priority = "essential"
"#;

    const PLUGIN_TWO_SAME_PROCESS: &str = r#"
schema_version = 1
id = "two"
name = "Two"
processes = ["shared.exe"]

[[bindings]]
keys = ["C-p"]
description = "Two"
category = "Common"
priority = "essential"
"#;

    #[test]
    fn every_builtin_plugin_loads_and_expected_processes_exist() {
        let report = PluginSnapshot::load(BUILTIN_PLUGINS, Path::new("missing-user-dir")).unwrap();
        assert!(report.warnings.is_empty());
        for process in [
            "codex.exe",
            "chrome.exe",
            "msedge.exe",
            "wt.exe",
            "windowsterminal.exe",
            "chatgpt.exe",
            "claude.exe",
            "code.exe",
            "winword.exe",
            "excel.exe",
            "powerpnt.exe",
        ] {
            assert!(
                report.snapshot.for_process(process).is_some(),
                "missing {process}"
            );
        }

        let powerpoint = report.snapshot.for_process("powerpnt.exe").unwrap();
        for shortcut in ["F5", "S-F5"] {
            let binding = powerpoint
                .bindings
                .iter()
                .find(|binding| {
                    binding_identity_from_binding(binding)
                        == binding_identity(&[parse_shortcut(shortcut).unwrap()], false)
                })
                .unwrap();
            assert_eq!(binding.metadata.priority, BindingPriority::Essential);
        }
    }

    #[test]
    fn windows_terminal_plugin_includes_common_tab_and_pane_shortcuts() {
        let report = PluginSnapshot::load(BUILTIN_PLUGINS, Path::new("missing-user-dir")).unwrap();
        let plugin = report.snapshot.for_process("wt.exe").unwrap();

        assert_eq!(plugin.binding_description("C-S-c"), Some("复制所选文本"));
        assert_eq!(plugin.binding_description("C-S-v"), Some("粘贴所选文本"));
        assert_eq!(plugin.binding_description("C-Tab"), Some("切换到下一个标签页"));
        assert_eq!(plugin.binding_description("C-S-Tab"), Some("切换到上一个标签页"));
        assert_eq!(plugin.binding_description("A-S-d"), Some("拆分窗格"));
        assert_eq!(plugin.binding_description("F11"), Some("切换全屏"));
    }

    #[test]
    fn user_plugin_overrides_builtin_by_id_and_binding() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("vscode.toml"), USER_VSCODE).unwrap();
        let report = PluginSnapshot::load(&[("vscode.toml", BUILTIN_VSCODE)], dir.path()).unwrap();
        let plugin = report.snapshot.for_process("CODE.EXE").unwrap();
        assert_eq!(plugin.binding_description("C-p"), Some("My Quick Open"));
        assert_eq!(plugin.binding_description("C-S-p"), Some("Command Palette"));
    }

    #[test]
    fn disabled_user_plugin_removes_builtin() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("vscode.toml"), DISABLED_VSCODE).unwrap();
        let report = PluginSnapshot::load(&[("vscode.toml", BUILTIN_VSCODE)], dir.path()).unwrap();
        assert!(report.snapshot.for_process("code.exe").is_none());
    }

    #[test]
    fn invalid_user_plugin_is_warning_not_fatal() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("broken.toml"), "not toml").unwrap();
        let report = PluginSnapshot::load(&[("vscode.toml", BUILTIN_VSCODE)], dir.path()).unwrap();
        assert_eq!(report.warnings.len(), 1);
        assert!(report.snapshot.for_process("code.exe").is_some());
    }

    #[test]
    fn conflicting_ids_for_same_process_are_fatal_within_one_origin() {
        let builtins = [
            ("one.toml", PLUGIN_ONE),
            ("two.toml", PLUGIN_TWO_SAME_PROCESS),
        ];
        assert!(PluginSnapshot::load(&builtins, Path::new("missing-user-dir")).is_err());
    }

    #[test]
    fn plugin_path_sort_tie_breaks_case_folded_names_by_original_path() {
        let mut paths = vec![PathBuf::from("plugin.toml"), PathBuf::from("Plugin.toml")];

        sort_plugin_paths(&mut paths);

        assert_eq!(
            paths,
            vec![PathBuf::from("Plugin.toml"), PathBuf::from("plugin.toml")]
        );
    }

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
        assert!(matches!(
            plugin.bindings[0].keys,
            BindingKeys::Alternatives(_)
        ));
        assert!(matches!(plugin.bindings[1].keys, BindingKeys::Sequence(_)));
    }

    #[test]
    fn accepts_optional_plugin_description() {
        let source = r#"
schema_version = 1
id = "vscode"
name = "Visual Studio Code"
description = "Editor shortcuts"
processes = ["Code.exe"]

[[bindings]]
keys = ["C-p"]
description = "Quick Open"
category = "Common"
priority = "essential"
"#;

        let plugin = parse_plugin_toml(source, PluginOrigin::BuiltIn).unwrap();
        assert_eq!(plugin.description.as_deref(), Some("Editor shortcuts"));
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
    fn rejects_missing_or_empty_bindings() {
        let missing_bindings = r#"
schema_version = 1
id = "x"
name = "X"
processes = ["x.exe"]
"#;
        assert!(parse_plugin_toml(missing_bindings, PluginOrigin::BuiltIn).is_err());

        let empty_bindings = r#"
schema_version = 1
id = "x"
name = "X"
processes = ["x.exe"]
bindings = []
"#;
        assert!(parse_plugin_toml(empty_bindings, PluginOrigin::BuiltIn).is_err());
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
