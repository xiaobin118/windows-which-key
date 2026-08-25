use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::Path;

pub fn content_fingerprint(content: impl AsRef<[u8]>) -> u64 {
    let mut hasher = DefaultHasher::new();
    content.as_ref().hash(&mut hasher);
    hasher.finish()
}

pub fn source_fingerprint<'a, I>(sources: I) -> u64
where
    I: IntoIterator<Item = (&'a str, &'a str)>,
{
    let mut hasher = DefaultHasher::new();
    for (name, source) in sources {
        name.hash(&mut hasher);
        source.hash(&mut hasher);
    }
    hasher.finish()
}

pub fn directory_fingerprint(dir: &Path) -> u64 {
    let mut hasher = DefaultHasher::new();
    if let Ok(entries) = fs::read_dir(dir) {
        let mut paths = entries
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|path| {
                path.is_file()
                    && path
                        .extension()
                        .and_then(|extension| extension.to_str())
                        .is_some_and(|extension| extension.eq_ignore_ascii_case("toml"))
            })
            .collect::<Vec<_>>();
        paths.sort();
        for path in paths {
            path.to_string_lossy().hash(&mut hasher);
            if let Ok(meta) = fs::metadata(&path) {
                meta.len().hash(&mut hasher);
                if let Ok(modified) = meta.modified() {
                    if let Ok(duration) = modified.duration_since(std::time::UNIX_EPOCH) {
                        duration.as_nanos().hash(&mut hasher);
                    }
                }
            }
        }
    }
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_fingerprint_changes_with_content() {
        assert_eq!(content_fingerprint("abc"), content_fingerprint("abc"));
        assert_ne!(content_fingerprint("abc"), content_fingerprint("abd"));
    }

    #[test]
    fn source_fingerprint_is_deterministic() {
        let a = source_fingerprint([("one", "1"), ("two", "2")]);
        let b = source_fingerprint([("one", "1"), ("two", "2")]);
        assert_eq!(a, b);
    }
}
