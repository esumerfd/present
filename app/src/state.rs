use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

const STATE_FILE: &str = "/tmp/present-state.json";

/// Serializes tests that exercise the real, shared STATE_FILE (via
/// `save_state`/`load_state`/`App::new`/`NotesApp::new`) so parallel test
/// threads don't lose each other's writes in the read-modify-write cycle
/// below. Tests using a private per-test file (`save_state_to` etc.) don't
/// need this.
#[cfg(test)]
pub(crate) fn test_lock() -> std::sync::MutexGuard<'static, ()> {
    use std::sync::{Mutex, OnceLock};
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(())).lock().unwrap_or_else(|e| e.into_inner())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresentationState {
    pub current_topic: usize,
    pub panel_per_topic: Vec<usize>,
    pub visited: Vec<usize>,
}

fn canonical_key(assets_dir: &str) -> String {
    Path::new(assets_dir)
        .canonicalize()
        .unwrap_or_else(|_| Path::new(assets_dir).to_path_buf())
        .to_string_lossy()
        .to_string()
}

fn load_all(file: &str) -> Result<HashMap<String, PresentationState>> {
    let path = Path::new(file);
    if !path.exists() {
        return Ok(HashMap::new());
    }
    let content = fs::read_to_string(path)?;
    Ok(serde_json::from_str(&content).unwrap_or_default())
}

pub fn save_state(assets_dir: &str, state: &PresentationState) -> Result<()> {
    save_state_to(STATE_FILE, assets_dir, state)
}

pub fn load_state(assets_dir: &str) -> Result<Option<PresentationState>> {
    load_state_from(STATE_FILE, assets_dir)
}

pub fn clear_state(assets_dir: &str) -> Result<()> {
    clear_state_from(STATE_FILE, assets_dir)
}

fn clear_state_from(file: &str, assets_dir: &str) -> Result<()> {
    let key = canonical_key(assets_dir);
    let mut all = load_all(file)?;
    all.remove(&key);
    let json = serde_json::to_string_pretty(&all)?;
    fs::write(file, json)?;
    Ok(())
}

fn save_state_to(file: &str, assets_dir: &str, state: &PresentationState) -> Result<()> {
    let key = canonical_key(assets_dir);
    let mut all = load_all(file)?;
    all.insert(key, state.clone());
    let json = serde_json::to_string_pretty(&all)?;
    fs::write(file, json)?;
    Ok(())
}

fn load_state_from(file: &str, assets_dir: &str) -> Result<Option<PresentationState>> {
    let key = canonical_key(assets_dir);
    Ok(load_all(file)?.remove(&key))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returns_none_when_file_absent() {
        let f = "/tmp/present-state-test-absent.json";
        let _ = fs::remove_file(f);
        let result = load_state_from(f, "/tmp").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn returns_none_for_unknown_key() {
        let f = "/tmp/present-state-test-unknown-key.json";
        let _ = fs::remove_file(f);
        let state = PresentationState { current_topic: 1, panel_per_topic: vec![0], visited: vec![0] };
        save_state_to(f, "/tmp", &state).unwrap();
        let result = load_state_from(f, "/nonexistent-path-xyz").unwrap();
        assert!(result.is_none());
        let _ = fs::remove_file(f);
    }

    #[test]
    fn round_trips_state() {
        let f = "/tmp/present-state-test-round-trips.json";
        let _ = fs::remove_file(f);
        let state = PresentationState {
            current_topic: 2,
            panel_per_topic: vec![0, 1, 3],
            visited: vec![0, 1, 2],
        };
        save_state_to(f, "/tmp", &state).unwrap();
        let loaded = load_state_from(f, "/tmp").unwrap().expect("state should exist");
        assert_eq!(loaded.current_topic, 2);
        assert_eq!(loaded.panel_per_topic, vec![0, 1, 3]);
        assert_eq!(loaded.visited, vec![0, 1, 2]);
        let _ = fs::remove_file(f);
    }

    #[test]
    fn clear_removes_state_for_key() {
        let f = "/tmp/present-state-test-clear.json";
        let _ = fs::remove_file(f);
        let state = PresentationState { current_topic: 1, panel_per_topic: vec![2], visited: vec![0, 1] };
        save_state_to(f, "/tmp", &state).unwrap();
        clear_state_from(f, "/tmp").unwrap();
        let result = load_state_from(f, "/tmp").unwrap();
        assert!(result.is_none());
        let _ = fs::remove_file(f);
    }

    #[test]
    fn clear_preserves_other_keys() {
        let f = "/tmp/present-state-test-clear-preserve.json";
        let _ = fs::remove_file(f);
        let s1 = PresentationState { current_topic: 0, panel_per_topic: vec![1], visited: vec![0] };
        let s2 = PresentationState { current_topic: 2, panel_per_topic: vec![3], visited: vec![0, 1, 2] };
        save_state_to(f, "/tmp", &s1).unwrap();
        save_state_to(f, "/var", &s2).unwrap();
        clear_state_from(f, "/tmp").unwrap();
        assert!(load_state_from(f, "/tmp").unwrap().is_none());
        assert!(load_state_from(f, "/var").unwrap().is_some());
        let _ = fs::remove_file(f);
    }

    #[test]
    fn clear_is_safe_when_key_absent() {
        let f = "/tmp/present-state-test-clear-absent.json";
        let _ = fs::remove_file(f);
        clear_state_from(f, "/tmp").unwrap();
        let _ = fs::remove_file(f);
    }

    #[test]
    fn preserves_other_keys_on_save() {
        let f = "/tmp/present-state-test-multi-key.json";
        let _ = fs::remove_file(f);
        let s1 = PresentationState { current_topic: 0, panel_per_topic: vec![1], visited: vec![0] };
        let s2 = PresentationState { current_topic: 3, panel_per_topic: vec![2], visited: vec![0, 1, 2, 3] };
        save_state_to(f, "/tmp", &s1).unwrap();
        save_state_to(f, "/var", &s2).unwrap();
        let l1 = load_state_from(f, "/tmp").unwrap().expect("state for /tmp");
        let l2 = load_state_from(f, "/var").unwrap().expect("state for /var");
        assert_eq!(l1.current_topic, 0);
        assert_eq!(l2.current_topic, 3);
        let _ = fs::remove_file(f);
    }
}
