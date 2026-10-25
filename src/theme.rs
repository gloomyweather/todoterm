//! Color palette: terminal ANSI colors, or the current Omarchy theme with live reload.

use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::SystemTime;

use anyhow::{anyhow, Result};
use ratatui::style::Color;

use crate::config::{ColorOverrides, ThemeMode};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Palette {
    pub fg: Color,
    pub muted: Color,
    pub accent: Color,
    pub selection_bg: Color,
    pub selection_fg: Color,
    pub danger: Color,
    pub warning: Color,
    pub success: Color,
    pub border: Color,
    /// Draw the selection with reversed video instead of `selection_bg`.
    pub reverse_selection: bool,
}

impl Palette {
    /// The terminal's own colors, so any terminal theme (light or dark) works.
    pub fn terminal() -> Self {
        Self {
            fg: Color::Reset,
            muted: Color::DarkGray,
            accent: Color::Blue,
            selection_bg: Color::Reset,
            selection_fg: Color::Reset,
            danger: Color::Red,
            warning: Color::Yellow,
            success: Color::Green,
            border: Color::DarkGray,
            reverse_selection: true,
        }
    }

    /// Build a palette from an Omarchy `colors.toml`. Missing keys fall back
    /// to terminal colors. The background is never painted.
    pub fn from_omarchy(colors_toml: &str) -> Result<Self> {
        let table: toml::Table = colors_toml.parse()?;
        let get = |key: &str| table.get(key).and_then(|v| v.as_str()).and_then(parse_hex);
        let rgb = |key: &str, fallback: Color| get(key).map_or(fallback, to_color);

        // Omarchy themes disagree on whether `muted` or `dark_foreground` is the
        // more readable one: use the higher-contrast one for text, the other for borders.
        let (muted, border) = match (get("muted"), get("dark_foreground")) {
            (Some(m), Some(d)) => match get("background") {
                Some(bg) if contrast(d, bg) > contrast(m, bg) => (Some(d), Some(m)),
                _ => (Some(m), Some(d)),
            },
            (one, other) => (one.or(other), one.or(other)),
        };

        let base = Self::terminal();
        let selection = get("selection");
        Ok(Self {
            fg: rgb("foreground", base.fg),
            muted: muted.map_or(base.muted, to_color),
            accent: rgb("accent", base.accent),
            selection_bg: selection.map_or(base.selection_bg, to_color),
            selection_fg: rgb("foreground", base.selection_fg),
            danger: rgb("red", base.danger),
            warning: rgb("yellow", base.warning),
            success: rgb("green", base.success),
            border: border.map_or(base.border, to_color),
            reverse_selection: selection.is_none(),
        })
    }

    /// Apply `[colors]` from the config on top of this palette.
    pub fn with_overrides(mut self, o: &ColorOverrides) -> Result<Self> {
        let fields: [(&Option<String>, &mut Color, &str); 9] = [
            (&o.fg, &mut self.fg, "fg"),
            (&o.muted, &mut self.muted, "muted"),
            (&o.accent, &mut self.accent, "accent"),
            (&o.selection_bg, &mut self.selection_bg, "selection_bg"),
            (&o.selection_fg, &mut self.selection_fg, "selection_fg"),
            (&o.danger, &mut self.danger, "danger"),
            (&o.warning, &mut self.warning, "warning"),
            (&o.success, &mut self.success, "success"),
            (&o.border, &mut self.border, "border"),
        ];
        for (value, slot, name) in fields {
            if let Some(v) = value {
                *slot = Color::from_str(v.trim()).map_err(|_| anyhow!("invalid color for {name}: {v:?}"))?;
            }
        }
        if o.selection_bg.is_some() {
            self.reverse_selection = false;
        }
        Ok(self)
    }
}

fn to_color((r, g, b): (u8, u8, u8)) -> Color {
    Color::Rgb(r, g, b)
}

fn parse_hex(s: &str) -> Option<(u8, u8, u8)> {
    let hex = s.trim().strip_prefix('#')?;
    if hex.len() != 6 || !hex.is_ascii() {
        return None;
    }
    let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
    Some((byte(0)?, byte(2)?, byte(4)?))
}

/// WCAG relative luminance contrast ratio.
fn contrast(a: (u8, u8, u8), b: (u8, u8, u8)) -> f32 {
    fn lum((r, g, b): (u8, u8, u8)) -> f32 {
        let ch = |c: u8| {
            let c = c as f32 / 255.0;
            if c <= 0.03928 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * ch(r) + 0.7152 * ch(g) + 0.0722 * ch(b)
    }
    let (la, lb) = (lum(a), lum(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

/// Identity of the active Omarchy theme file, to notice theme switches.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ThemeStamp {
    path: PathBuf,
    modified: Option<SystemTime>,
    len: u64,
    name: String,
}

/// Tracks the active theme and rebuilds the palette when it changes.
pub struct ThemeSource {
    mode: ThemeMode,
    /// Omarchy `current` directories to look in, newest layout first.
    roots: Vec<PathBuf>,
    overrides: ColorOverrides,
    seen: Option<ThemeStamp>,
    palette: Palette,
}

impl ThemeSource {
    pub fn new(mode: ThemeMode, home: Option<&Path>, overrides: ColorOverrides) -> Result<Self> {
        let roots = home
            .map(|h| {
                vec![
                    h.join(".local/state/omarchy/current"),
                    h.join(".config/omarchy/current"),
                ]
            })
            .unwrap_or_default();
        let mut source = Self {
            mode,
            roots,
            palette: Palette::terminal().with_overrides(&overrides)?,
            overrides,
            seen: None,
        };
        source.poll();
        Ok(source)
    }

    pub fn palette(&self) -> Palette {
        self.palette
    }

    fn current(&self) -> Option<ThemeStamp> {
        if self.mode == ThemeMode::Terminal {
            return None;
        }
        self.roots.iter().find_map(|root| {
            let file = root.join("theme/colors.toml");
            let meta = fs::metadata(&file).ok()?;
            Some(ThemeStamp {
                path: fs::canonicalize(&file).unwrap_or(file),
                modified: meta.modified().ok(),
                len: meta.len(),
                name: fs::read_to_string(root.join("theme.name")).unwrap_or_default(),
            })
        })
    }

    /// Re-check the theme; returns true when the palette changed.
    pub fn poll(&mut self) -> bool {
        let current = self.current();
        if current == self.seen {
            return false;
        }
        let palette = match &current {
            None => Palette::terminal(),
            Some(stamp) => {
                // A theme switch may be mid-write; keep the old palette and retry next tick.
                let Ok(text) = fs::read_to_string(&stamp.path) else {
                    return false;
                };
                let Ok(p) = Palette::from_omarchy(&text) else {
                    return false;
                };
                p
            }
        };
        // Overrides were validated in new().
        let palette = palette.with_overrides(&self.overrides).unwrap_or(palette);
        self.seen = current;
        let changed = palette != self.palette;
        self.palette = palette;
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOKYO: &str = include_str!("../tests/fixtures/tokyo-night.toml");
    const LATTE: &str = include_str!("../tests/fixtures/catppuccin-latte.toml");

    #[test]
    fn tokyo_night() {
        let p = Palette::from_omarchy(TOKYO).unwrap();
        assert_eq!(p.accent, Color::Rgb(0x7a, 0xa2, 0xf7));
        assert_eq!(p.fg, Color::Rgb(0xa9, 0xb1, 0xd6));
        assert_eq!(p.selection_bg, Color::Rgb(0x29, 0x2e, 0x42));
        assert_eq!(p.danger, Color::Rgb(0xf7, 0x76, 0x8e));
        assert_eq!(p.warning, Color::Rgb(0xe0, 0xaf, 0x68));
        assert_eq!(p.success, Color::Rgb(0x9e, 0xce, 0x6a));
        // dark_foreground reads better than muted on this dark background.
        assert_eq!(p.muted, Color::Rgb(0x56, 0x5f, 0x89));
        assert_eq!(p.border, Color::Rgb(0x41, 0x48, 0x68));
        assert!(!p.reverse_selection);
    }

    #[test]
    fn catppuccin_latte() {
        let p = Palette::from_omarchy(LATTE).unwrap();
        assert_eq!(p.accent, Color::Rgb(0x1e, 0x66, 0xf5));
        assert_eq!(p.fg, Color::Rgb(0x4c, 0x4f, 0x69));
        assert_eq!(p.muted, Color::Rgb(0x9c, 0xa0, 0xb0));
        // Nothing hard-coded white or black.
        for c in [p.fg, p.muted, p.accent, p.selection_bg, p.selection_fg] {
            assert_ne!(c, Color::White);
            assert_ne!(c, Color::Black);
        }
    }

    #[test]
    fn partial_file_falls_back() {
        let p = Palette::from_omarchy("accent = \"#112233\"\nred = \"nonsense\"").unwrap();
        let t = Palette::terminal();
        assert_eq!(p.accent, Color::Rgb(0x11, 0x22, 0x33));
        assert_eq!(p.danger, t.danger);
        assert_eq!(p.fg, t.fg);
        assert_eq!(p.muted, t.muted);
        assert!(p.reverse_selection);
        assert!(Palette::from_omarchy("not = [toml").is_err());
    }

    #[test]
    fn overrides() {
        let o = ColorOverrides {
            accent: Some("#ff0000".into()),
            selection_bg: Some("darkgray".into()),
            ..Default::default()
        };
        let p = Palette::terminal().with_overrides(&o).unwrap();
        assert_eq!(p.accent, Color::Rgb(255, 0, 0));
        assert_eq!(p.selection_bg, Color::DarkGray);
        assert!(!p.reverse_selection);
        let bad = ColorOverrides {
            muted: Some("blurple".into()),
            ..Default::default()
        };
        assert!(Palette::terminal().with_overrides(&bad).is_err());
    }

    #[test]
    fn live_reload() {
        let home = tempfile::tempdir().unwrap();
        let current = home.path().join(".local/state/omarchy/current");
        fs::create_dir_all(current.join("theme")).unwrap();

        let mut src = ThemeSource::new(ThemeMode::Auto, Some(home.path()), Default::default()).unwrap();
        assert_eq!(src.palette(), Palette::terminal());

        fs::write(current.join("theme/colors.toml"), TOKYO).unwrap();
        fs::write(current.join("theme.name"), "tokyo-night").unwrap();
        assert!(src.poll());
        assert_eq!(src.palette().accent, Color::Rgb(0x7a, 0xa2, 0xf7));
        assert!(!src.poll());

        fs::write(current.join("theme/colors.toml"), LATTE).unwrap();
        fs::write(current.join("theme.name"), "catppuccin-latte").unwrap();
        assert!(src.poll());
        assert_eq!(src.palette().accent, Color::Rgb(0x1e, 0x66, 0xf5));

        fs::remove_dir_all(current.join("theme")).unwrap();
        assert!(src.poll());
        assert_eq!(src.palette(), Palette::terminal());
    }

    #[test]
    fn legacy_location_and_terminal_mode() {
        let home = tempfile::tempdir().unwrap();
        let theme = home.path().join(".config/omarchy/current/theme");
        fs::create_dir_all(&theme).unwrap();
        fs::write(theme.join("colors.toml"), LATTE).unwrap();
        let src = ThemeSource::new(ThemeMode::Auto, Some(home.path()), Default::default()).unwrap();
        assert_eq!(src.palette().accent, Color::Rgb(0x1e, 0x66, 0xf5));
        let src = ThemeSource::new(ThemeMode::Terminal, Some(home.path()), Default::default()).unwrap();
        assert_eq!(src.palette(), Palette::terminal());
    }
}
