use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ContentRevision(String);

impl ContentRevision {
    pub fn from_content(content: impl AsRef<[u8]>) -> Self {
        Self(format!("sha256:{}", sha256_hex(content.as_ref())))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceRevisions {
    pub theme: ContentRevision,
    pub global_config: ContentRevision,
    pub plugins: HashMap<String, ContentRevision>,
}

impl ResourceRevisions {
    pub fn new(theme_content: impl AsRef<[u8]>, global_content: impl AsRef<[u8]>) -> Self {
        Self {
            theme: ContentRevision::from_content(theme_content),
            global_config: ContentRevision::from_content(global_content),
            plugins: HashMap::new(),
        }
    }

    pub fn set_theme_from_content(&mut self, content: impl AsRef<[u8]>) {
        self.theme = ContentRevision::from_content(content);
    }

    pub fn set_global_config_from_content(&mut self, content: impl AsRef<[u8]>) {
        self.global_config = ContentRevision::from_content(content);
    }

    pub fn set_plugin_from_content(&mut self, id: impl Into<String>, content: impl AsRef<[u8]>) {
        self.plugins
            .insert(id.into(), ContentRevision::from_content(content));
    }
}

#[derive(Debug, Clone)]
pub struct RuntimeSnapshot {
    pub generation: u64,
    pub revisions: ResourceRevisions,
}

impl RuntimeSnapshot {
    pub fn new(generation: u64, revisions: ResourceRevisions) -> Self {
        Self {
            generation,
            revisions,
        }
    }
}

#[derive(Debug)]
pub struct RuntimeSnapshotStore {
    current: RwLock<Arc<RuntimeSnapshot>>,
}

impl RuntimeSnapshotStore {
    pub fn new(snapshot: RuntimeSnapshot) -> Self {
        Self {
            current: RwLock::new(Arc::new(snapshot)),
        }
    }

    pub fn current(&self) -> Arc<RuntimeSnapshot> {
        Arc::clone(&self.current.read().expect("runtime snapshot lock poisoned"))
    }

    pub fn replace_with(
        &self,
        build: impl FnOnce(&RuntimeSnapshot) -> RuntimeSnapshot,
    ) -> Arc<RuntimeSnapshot> {
        let mut current = self
            .current
            .write()
            .expect("runtime snapshot lock poisoned");
        let replacement = Arc::new(build(current.as_ref()));
        assert!(
            replacement.generation > current.generation,
            "runtime snapshot generation must increase"
        );
        *current = Arc::clone(&replacement);
        replacement
    }
}

fn sha256_hex(input: &[u8]) -> String {
    const INITIAL: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let bit_length = (input.len() as u64).wrapping_mul(8);
    let mut message = input.to_vec();
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&bit_length.to_be_bytes());
    let mut hash = INITIAL;
    for chunk in message.chunks_exact(64) {
        let mut words = [0u32; 64];
        for (index, word) in words[..16].iter_mut().enumerate() {
            *word = u32::from_be_bytes(chunk[index * 4..index * 4 + 4].try_into().unwrap());
        }
        for index in 16..64 {
            let s0 = words[index - 15].rotate_right(7)
                ^ words[index - 15].rotate_right(18)
                ^ (words[index - 15] >> 3);
            let s1 = words[index - 2].rotate_right(17)
                ^ words[index - 2].rotate_right(19)
                ^ (words[index - 2] >> 10);
            words[index] = words[index - 16]
                .wrapping_add(s0)
                .wrapping_add(words[index - 7])
                .wrapping_add(s1);
        }
        let (mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h) = (
            hash[0], hash[1], hash[2], hash[3], hash[4], hash[5], hash[6], hash[7],
        );
        for index in 0..64 {
            let sum1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let choice = (e & f) ^ ((!e) & g);
            let temp1 = h
                .wrapping_add(sum1)
                .wrapping_add(choice)
                .wrapping_add(K[index])
                .wrapping_add(words[index]);
            let sum0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let majority = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = sum0.wrapping_add(majority);
            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }
        for (value, addend) in hash.iter_mut().zip([a, b, c, d, e, f, g, h]) {
            *value = value.wrapping_add(addend);
        }
    }
    hash.iter().map(|word| format!("{word:08x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_revisions_are_deterministic() {
        assert_eq!(
            ContentRevision::from_content("theme"),
            ContentRevision::from_content("theme")
        );
        assert_ne!(
            ContentRevision::from_content("theme"),
            ContentRevision::from_content("other")
        );
    }

    #[test]
    fn resource_revisions_track_each_resource_independently() {
        let mut revisions = ResourceRevisions::new("theme", "global");
        let original_global = revisions.global_config.clone();
        revisions.set_theme_from_content("new-theme");
        revisions.set_plugin_from_content("editor", "plugin");

        assert_ne!(revisions.theme, ContentRevision::from_content("theme"));
        assert_eq!(revisions.global_config, original_global);
        assert_eq!(
            revisions.plugins["editor"],
            ContentRevision::from_content("plugin")
        );
    }

    #[test]
    fn replacing_a_snapshot_increments_generation_monotonically() {
        let initial = RuntimeSnapshot::new(0, ResourceRevisions::new("theme", "global"));
        let store = RuntimeSnapshotStore::new(initial);

        let first = store.replace_with(|current| {
            RuntimeSnapshot::new(current.generation + 1, current.revisions.clone())
        });
        let second = store.replace_with(|current| {
            RuntimeSnapshot::new(current.generation + 1, current.revisions.clone())
        });

        assert_eq!(first.generation, 1);
        assert_eq!(second.generation, 2);
        assert_eq!(store.current().generation, 2);
    }
}
