//! Optional `~/.config/todoterm/config.toml`.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeMode {
    /// Omarchy colors when available, terminal colors otherwise.
    #[default]
    Auto,
    Omarchy,
    Terminal,
}

/// Manual color overrides, applied on top of the theme. Values are `#rrggbb`
/// or color names such as `red` or `darkgray`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ColorOverrides {
    #[serde(alias = "foreground")]
    pub fg: Option<String>,
    pub muted: Option<String>,
    pub accent: Option<String>,
    #[serde(alias = "selection")]
    pub selection_bg: Option<String>,
    pub selection_fg: Option<String>,
    #[serde(alias = "red")]
    pub danger: Option<String>,
    #[serde(alias = "yellow")]
    pub warning: Option<String>,
    #[serde(alias = "green")]
    pub success: Option<String>,
    pub border: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub file: Option<PathBuf>,
    pub kaomoji: bool,
    pub hide_done: bool,
    pub theme: ThemeMode,
    pub colors: ColorOverrides,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            file: None,
            kaomoji: true,
            hide_done: false,
            theme: ThemeMode::Auto,
            colors: ColorOverrides::default(),
        }
    }
}

impl Config {
    pub fn default_path() -> Option<PathBuf> {
        dirs::config_dir().map(|d| d.join("todoterm").join("config.toml"))
    }

    /// Load the config; a missing file gives the defaults.
    pub fn load(path: Option<&Path>) -> Result<Self> {
        let Some(path) = path else {
            return Ok(Self::default());
        };
        match fs::read_to_string(path) {
            Ok(text) => toml::from_str(&text).with_context(|| format!("invalid config {}", path.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e).with_context(|| format!("could not read {}", path.display())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_is_default() {
        let dir = tempfile::tempdir().unwrap();
        let c = Config::load(Some(&dir.path().join("nope.toml"))).unwrap();
        assert_eq!(c, Config::default());
        assert!(c.kaomoji);
    }

    #[test]
    fn parses_all_fields() {
        let c: Config = toml::from_str(
            r##"
            file = "~/notes/todo.json"
            kaomoji = false
            hide_done = true
            theme = "terminal"
            [colors]
            accent = "#ff0000"
            selection = "darkgray"
            "##,
        )
        .unwrap();
        assert_eq!(c.file, Some(PathBuf::from("~/notes/todo.json")));
        assert!(!c.kaomoji);
        assert!(c.hide_done);
        assert_eq!(c.theme, ThemeMode::Terminal);
        assert_eq!(c.colors.accent.as_deref(), Some("#ff0000"));
        assert_eq!(c.colors.selection_bg.as_deref(), Some("darkgray"));
    }

    #[test]
    fn rejects_typos() {
        assert!(toml::from_str::<Config>("kaomojii = false").is_err());
        assert!(toml::from_str::<Config>("theme = \"neon\"").is_err());
    }
}
