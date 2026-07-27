//! config.json loading. The JSON structure is identical to the macOS
//! version; a mapping value may additionally be an array of chords to
//! express multi-step actions (e.g. "delete to line start" =
//! Shift+Home then Backspace, which has no single Windows keystroke).

use std::collections::HashMap;
use std::env;
use std::path::PathBuf;

use serde::Deserialize;

use crate::key_codes::vk_from_name;
use crate::state_machine::Keys;

#[derive(Debug, Clone, Default, Deserialize)]
pub struct VerboseConfig {
    #[serde(default)]
    pub on_state: bool,
    #[serde(default)]
    pub on_event: bool,
    #[serde(default)]
    pub on_action: bool,
}

#[derive(Deserialize)]
struct RawEntry {
    key: String,
    #[serde(default)]
    modifiers: Vec<String>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum RawMapping {
    One(RawEntry),
    Seq(Vec<RawEntry>),
}

#[derive(Deserialize)]
struct RawConfig {
    #[serde(default)]
    hyper_keys_map: HashMap<String, RawMapping>,
    #[serde(default)]
    verbose: VerboseConfig,
    #[serde(default)]
    hold_as_hyper: bool,
    #[serde(default)]
    blacklist: Vec<String>,
}

pub struct Config {
    pub map: HashMap<u16, Vec<Keys>>,
    pub verbose: VerboseConfig,
    pub hold_as_hyper: bool,
    /// Foreground process exe names for which Super++ is disabled.
    pub blacklist: Vec<String>,
    pub path: Option<PathBuf>,
}

/// Same lookup order as the mac version: SUPERPP_CONFIG (or legacy
/// SPACEPP_CONFIG) env var, then config.json next to the executable,
/// then the current directory.
pub fn resolve_path() -> Option<PathBuf> {
    for key in ["SUPERPP_CONFIG", "SPACEPP_CONFIG"] {
        if let Ok(p) = env::var(key) {
            let p = PathBuf::from(p);
            if p.exists() {
                return Some(p);
            }
        }
    }
    if let Ok(exe) = env::current_exe() {
        if let Some(dir) = exe.parent() {
            let p = dir.join("config.json");
            if p.exists() {
                return Some(p);
            }
        }
    }
    let p = PathBuf::from("config.json");
    if p.exists() {
        return Some(p);
    }
    None
}

fn entry_to_keys(entry: &RawEntry, warnings: &mut Vec<String>) -> Option<Keys> {
    let main = match vk_from_name(&entry.key) {
        Some(vk) => vk,
        None => {
            warnings.push(format!("unknown target key '{}'", entry.key));
            return None;
        }
    };
    let mut modifiers = Vec::new();
    for name in &entry.modifiers {
        match vk_from_name(name) {
            Some(vk) => modifiers.push(vk),
            None => warnings.push(format!("unknown modifier '{}'", name)),
        }
    }
    Some(Keys { main, modifiers })
}

pub fn parse(json: &str) -> Result<(Config, Vec<String>), String> {
    let raw: RawConfig = serde_json::from_str(json).map_err(|e| e.to_string())?;
    let mut warnings = Vec::new();
    let mut map = HashMap::new();

    for (source_name, mapping) in &raw.hyper_keys_map {
        let source_vk = match vk_from_name(source_name) {
            Some(vk) => vk,
            None => {
                warnings.push(format!("unknown source key '{}'", source_name));
                continue;
            }
        };
        let entries: Vec<&RawEntry> = match mapping {
            RawMapping::One(e) => vec![e],
            RawMapping::Seq(v) => v.iter().collect(),
        };
        let seq: Vec<Keys> = entries
            .iter()
            .filter_map(|e| entry_to_keys(e, &mut warnings))
            .collect();
        if !seq.is_empty() {
            map.insert(source_vk, seq);
        }
    }

    Ok((
        Config {
            map,
            verbose: raw.verbose,
            hold_as_hyper: raw.hold_as_hyper,
            blacklist: raw.blacklist,
            path: None,
        },
        warnings,
    ))
}

/// The repository's config.json, embedded at compile time so a single
/// exe is fully functional without any external file.
const DEFAULT_CONFIG: &str = include_str!("../config.json");

pub fn embedded_default() -> Config {
    let (cfg, _) = parse(DEFAULT_CONFIG).expect("embedded config.json must be valid");
    cfg
}

/// Minimal fallback (hjkl arrows), mirroring the mac version's behavior
/// when a config cannot be parsed at all.
pub fn fallback() -> Config {
    let mut map = HashMap::new();
    for (name, target) in [("h", "left_arrow"), ("j", "down_arrow"), ("k", "up_arrow"), ("l", "right_arrow")] {
        map.insert(
            vk_from_name(name).unwrap(),
            vec![Keys::plain(vk_from_name(target).unwrap())],
        );
    }
    Config {
        map,
        verbose: VerboseConfig::default(),
        hold_as_hyper: true,
        blacklist: Vec::new(),
        path: None,
    }
}

pub fn load() -> (Config, Vec<String>) {
    match resolve_path() {
        Some(path) => match std::fs::read_to_string(&path) {
            Ok(json) => match parse(&json) {
                Ok((mut cfg, warnings)) => {
                    cfg.path = Some(path);
                    (cfg, warnings)
                }
                Err(e) => (fallback(), vec![format!("config parse error: {e}")]),
            },
            Err(e) => (fallback(), vec![format!("config read error: {e}")]),
        },
        None => (
            embedded_default(),
            vec!["config.json not found, using embedded default config".into()],
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::key_codes::EXIT_KEY;

    #[test]
    fn parses_full_config_shape() {
        let json = r#"{
            "hyper_keys_map": {
                "q": {"key": "exit"},
                "h": {"key": "left_arrow"},
                "c": {"key": "c", "modifiers": ["control"]},
                "b": [{"key": "home", "modifiers": ["shift"]}, {"key": "delete"}]
            },
            "verbose": {"on_state": true},
            "hold_as_hyper": true
        }"#;
        let (cfg, warnings) = parse(json).unwrap();
        assert!(warnings.is_empty(), "{warnings:?}");
        assert!(cfg.hold_as_hyper);
        assert!(cfg.verbose.on_state);
        assert!(!cfg.verbose.on_event);

        let q = &cfg.map[&(b'Q' as u16)];
        assert_eq!(q[0].main, EXIT_KEY);

        let b = &cfg.map[&(b'B' as u16)];
        assert_eq!(b.len(), 2);
        assert_eq!(b[0].main, 0x24); // home
        assert_eq!(b[0].modifiers, vec![0x10]); // shift
        assert_eq!(b[1].main, 0x08); // backspace
    }

    #[test]
    fn parses_blacklist() {
        let json = r#"{"hyper_keys_map": {}, "blacklist": ["GameApp.exe", "other.exe"]}"#;
        let (cfg, warnings) = parse(json).unwrap();
        assert!(warnings.is_empty());
        assert_eq!(cfg.blacklist, vec!["GameApp.exe", "other.exe"]);
    }

    #[test]
    fn blacklist_defaults_to_empty() {
        let (cfg, _) = parse(r#"{"hyper_keys_map": {}}"#).unwrap();
        assert!(cfg.blacklist.is_empty());
    }

    #[test]
    fn embedded_default_config_is_complete() {
        let cfg = embedded_default();
        assert!(cfg.hold_as_hyper);
        // Full mapping, not the minimal hjkl fallback.
        assert!(cfg.map.len() > 20, "got {}", cfg.map.len());
        assert!(cfg.map.contains_key(&(b'Q' as u16))); // exit
        assert!(cfg.map.contains_key(&(b'C' as u16))); // Ctrl+C
    }

    #[test]
    fn unknown_keys_produce_warnings_not_errors() {
        let json = r#"{"hyper_keys_map": {"zz_bogus": {"key": "left_arrow"}, "h": {"key": "bogus"}}}"#;
        let (cfg, warnings) = parse(json).unwrap();
        assert!(cfg.map.is_empty());
        assert_eq!(warnings.len(), 2);
    }
}
