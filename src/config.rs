use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::fs;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Config {
    pub jit_confirm: bool,
    pub ttl_seconds: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            jit_confirm: true,
            ttl_seconds: 900,
        }
    }
}

impl Config {
    pub fn load_or_create(home_dir: &str) -> Self {
        let path = PathBuf::from(home_dir).join(".config").join("sudo-me").join("config.toml");
        if path.exists() {
            if let Ok(content) = fs::read_to_string(&path) {
                if let Ok(config) = toml::from_str(&content) {
                    return config;
                }
            }
        }
        let config = Config::default();
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _ = fs::write(&path, toml::to_string(&config).unwrap());
        config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default() {
        let config = Config::default();
        assert!(config.jit_confirm);
        assert_eq!(config.ttl_seconds, 900);
    }

    #[test]
    fn test_config_parsing() {
        let toml_str = r#"
            jit_confirm = false
            ttl_seconds = 300
        "#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert!(!config.jit_confirm);
        assert_eq!(config.ttl_seconds, 300);
    }
}
