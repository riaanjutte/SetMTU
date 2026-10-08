//! Remembers each adapter's original MTU so Undo can put it back,
//! even after the app is closed and reopened.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Record {
    /// Interface LUID; unlike the index, it survives reboots.
    pub luid: u64,
    pub alias: String,
    pub mtu_v4: Option<u32>,
    pub mtu_v6: Option<u32>,
}

#[derive(Default, Serialize, Deserialize)]
pub struct UndoStore {
    pub records: Vec<Record>,
}

fn path() -> Option<PathBuf> {
    let base = std::env::var_os("LOCALAPPDATA")?;
    Some(PathBuf::from(base).join("SetMTU").join("undo.json"))
}

impl UndoStore {
    pub fn load() -> Self {
        path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) -> Result<(), String> {
        let p = path().ok_or("LOCALAPPDATA is not set.")?;
        if self.records.is_empty() {
            return match std::fs::remove_file(&p) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.to_string()),
                _ => Ok(()),
            };
        }
        if let Some(dir) = p.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(&p, json).map_err(|e| e.to_string())
    }

    pub fn has(&self, luid: u64) -> bool {
        self.records.iter().any(|r| r.luid == luid)
    }

    /// Keeps the first-seen original. A second "Set" click must not
    /// overwrite it with 1428, or Undo would have nothing to go back to.
    pub fn remember(&mut self, record: Record) {
        if !self.has(record.luid) {
            self.records.push(record);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(luid: u64, mtu: u32) -> Record {
        Record { luid, alias: "Ethernet".into(), mtu_v4: Some(mtu), mtu_v6: None }
    }

    #[test]
    fn second_set_keeps_first_original() {
        let mut store = UndoStore::default();
        store.remember(rec(1, 1500));
        store.remember(rec(1, 1428));
        assert_eq!(store.records.len(), 1);
        assert_eq!(store.records[0].mtu_v4, Some(1500));
    }

    #[test]
    fn different_adapters_tracked_separately() {
        let mut store = UndoStore::default();
        store.remember(rec(1, 1500));
        store.remember(rec(2, 1492));
        assert_eq!(store.records.len(), 2);
    }
}
