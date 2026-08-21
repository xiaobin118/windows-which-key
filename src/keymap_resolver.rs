use std::sync::Arc;

use crate::config::ConfigurationSnapshot;
use crate::plugin::PluginSnapshot;
use crate::registry::ShortcutRegistry;

/// The immutable, application-specific keymap selected for a foreground process.
pub struct ResolvedKeymap {
    pub app_name: String,
    pub registry: Arc<ShortcutRegistry>,
}

/// Builds effective keymaps from a configuration snapshot without consulting Windows APIs.
pub struct KeymapResolver {
    global: Arc<ShortcutRegistry>,
    plugins: Arc<PluginSnapshot>,
}

impl KeymapResolver {
    pub fn new(global: ShortcutRegistry, plugins: Arc<PluginSnapshot>) -> Self {
        Self {
            global: Arc::new(global),
            plugins,
        }
    }

    pub fn from_snapshot(snapshot: &ConfigurationSnapshot) -> Self {
        Self {
            global: Arc::new(snapshot.global.clone()),
            plugins: Arc::clone(&snapshot.plugins),
        }
    }

    pub fn resolve(&self, process: Option<&str>) -> ResolvedKeymap {
        let normalized_process = process.and_then(normalize_process_name);
        let Some(plugin) = normalized_process
            .as_deref()
            .and_then(|process| self.plugins.for_process(process))
        else {
            return ResolvedKeymap {
                app_name: "Windows".to_string(),
                registry: Arc::clone(&self.global),
            };
        };

        let mut effective = (*self.global).clone();
        let plugin_registry = ShortcutRegistry::from_plugin(plugin)
            .expect("PluginSnapshot only contains validated plugin definitions");
        effective.merge_from(&plugin_registry);

        ResolvedKeymap {
            app_name: plugin.name.clone(),
            registry: Arc::new(effective),
        }
    }
}

fn normalize_process_name(process: &str) -> Option<String> {
    let process = process.trim();
    let executable = process.rsplit(['\\', '/']).next()?.trim();
    (!executable.is_empty()).then(|| executable.to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::Arc;

    use crate::config::ConfigurationService;
    use crate::keymap_resolver::KeymapResolver;
    use crate::plugin::PluginSnapshot;
    use crate::registry::ShortcutRegistry;
    use crate::shortcut::parse_shortcut;
    use crate::types::ResolveResult;

    const GLOBAL: &str = r#"
[globals]
"C-p" = { desc = "Global command" }
"M-d" = { desc = "Show desktop" }
"#;

    const INVALID_GLOBAL: &str = "[globals\n";

    const VSCODE: &str = r#"
schema_version = 1
id = "vscode"
name = "Visual Studio Code"
processes = ["Code.exe"]

[[bindings]]
keys = ["C-p"]
description = "Quick Open"
category = "Navigation"
priority = "essential"
"#;

    fn fixture_resolver() -> KeymapResolver {
        let plugins = PluginSnapshot::load(&[("vscode.toml", VSCODE)], Path::new("missing"))
            .unwrap()
            .snapshot;
        KeymapResolver::new(crate::config::parse_toml(GLOBAL).unwrap(), plugins)
    }

    fn fixture_configuration_service() -> ConfigurationService {
        ConfigurationService::from_sources(GLOBAL, &[("vscode.toml", VSCODE)], Path::new("missing"))
            .unwrap()
            .0
    }

    #[test]
    fn merges_app_over_global_and_falls_back_for_unknown_process() {
        let resolver = fixture_resolver();
        let vscode = resolver.resolve(Some("C:\\Program Files\\Microsoft VS Code\\CODE.EXE"));
        assert_eq!(vscode.app_name, "Visual Studio Code");
        assert_eq!(leaf_desc(&vscode.registry, "C-p"), "Quick Open");
        assert_eq!(leaf_desc(&vscode.registry, "M-d"), "Show desktop");

        let unknown = resolver.resolve(Some("unknown.exe"));
        assert_eq!(unknown.app_name, "Windows");
        assert_eq!(leaf_desc(&unknown.registry, "C-p"), "Global command");
    }

    #[test]
    fn failed_reload_keeps_previous_snapshot() {
        let service = fixture_configuration_service();
        let before = service.current();

        assert!(service
            .reload_from_sources(INVALID_GLOBAL, &[], Path::new("missing"))
            .is_err());

        assert!(Arc::ptr_eq(&before, &service.current()));
    }

    fn leaf_desc(registry: &ShortcutRegistry, shortcut: &str) -> String {
        match registry.resolve(&[], parse_shortcut(shortcut).unwrap()) {
            ResolveResult::Leaf(entry) => entry.desc,
            _ => panic!("expected a leaf"),
        }
    }
}
