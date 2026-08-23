use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

const BUNDLE_TYPE: &str = "which-key-plugin-bundle";
const BUNDLE_VERSION: u32 = 1;

#[derive(Debug, Serialize, Deserialize)]
pub struct PluginBundle {
    #[serde(rename = "type")]
    pub bundle_type: String,
    pub schema_version: u32,
    pub source_version: String,
    pub plugins: Vec<PluginBundleEntry>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PluginBundleEntry {
    pub file: String,
    pub content: String,
}

pub fn export_user_plugin_bundle(plugin_dir: &Path) -> Result<String> {
    let mut files = read_user_plugin_files(plugin_dir)?;
    files.sort_by(|left, right| left.file.cmp(&right.file));

    let bundle = PluginBundle {
        bundle_type: BUNDLE_TYPE.to_string(),
        schema_version: BUNDLE_VERSION,
        source_version: env!("CARGO_PKG_VERSION").to_string(),
        plugins: files,
    };
    Ok(serde_json::to_string(&bundle).context("序列化插件包失败")?)
}

pub fn import_user_plugin_bundle(bundle_json: &str, plugin_dir: &Path) -> Result<usize> {
    let bundle: PluginBundle = serde_json::from_str(bundle_json).context("解析插件包失败")?;
    if bundle.bundle_type != BUNDLE_TYPE {
        bail!("插件包类型不匹配");
    }
    if bundle.schema_version != BUNDLE_VERSION {
        bail!("不支持的插件包版本: {}", bundle.schema_version);
    }

    fs::create_dir_all(plugin_dir)
        .with_context(|| format!("创建插件目录失败: {}", plugin_dir.display()))?;

    let mut imported = 0usize;
    for entry in bundle.plugins {
        let file_name = sanitize_plugin_file(&entry.file)?;
        let target = plugin_dir.join(&file_name);
        fs::write(&target, entry.content)
            .with_context(|| format!("写入插件文件失败: {}", target.display()))?;
        imported += 1;
    }

    Ok(imported)
}

fn read_user_plugin_files(plugin_dir: &Path) -> Result<Vec<PluginBundleEntry>> {
    let mut entries = Vec::new();
    if !plugin_dir.exists() {
        return Ok(entries);
    }

    for entry in fs::read_dir(plugin_dir)
        .with_context(|| format!("读取插件目录失败: {}", plugin_dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        if !path
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("toml"))
        {
            continue;
        }
        let file = path
            .file_name()
            .and_then(|value| value.to_str())
            .context("插件文件名无效")?
            .to_string();
        let content = fs::read_to_string(&path)
            .with_context(|| format!("读取插件文件失败: {}", path.display()))?;
        entries.push(PluginBundleEntry { file, content });
    }

    Ok(entries)
}

fn sanitize_plugin_file(file: &str) -> Result<String> {
    let path = PathBuf::from(file);
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .context("插件包中的文件名无效")?;
    if name != file {
        bail!("插件包中的文件名必须是不含路径的纯文件名");
    }
    if !name.to_ascii_lowercase().ends_with(".toml") {
        bail!("插件包中的文件必须以 .toml 结尾");
    }
    Ok(name.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_and_import_bundle_roundtrip() {
        let temp = tempfile::tempdir().unwrap();
        let plugin_dir = temp.path().join("plugins");
        fs::create_dir_all(&plugin_dir).unwrap();
        fs::write(plugin_dir.join("a.toml"), "# a").unwrap();
        fs::write(plugin_dir.join("b.toml"), "# b").unwrap();

        let bundle = export_user_plugin_bundle(&plugin_dir).unwrap();
        let import_dir = temp.path().join("imported");
        let count = import_user_plugin_bundle(&bundle, &import_dir).unwrap();

        assert_eq!(count, 2);
        assert_eq!(fs::read_to_string(import_dir.join("a.toml")).unwrap(), "# a");
        assert_eq!(fs::read_to_string(import_dir.join("b.toml")).unwrap(), "# b");
    }

    #[test]
    fn reject_non_toml_bundle_entries() {
        let bundle = PluginBundle {
            bundle_type: BUNDLE_TYPE.to_string(),
            schema_version: BUNDLE_VERSION,
            source_version: "0.1.0".to_string(),
            plugins: vec![PluginBundleEntry {
                file: "bad.txt".to_string(),
                content: "x".to_string(),
            }],
        };
        let json = serde_json::to_string(&bundle).unwrap();
        let temp = tempfile::tempdir().unwrap();
        let err = import_user_plugin_bundle(&json, temp.path()).unwrap_err();
        assert!(err.to_string().contains(".toml"));
    }
}
