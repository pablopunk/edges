//! Border appearance settings

use serde::{Deserialize, Serialize};

/// Border style variants
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BorderStyle {
    /// Rounded corners
    Round,
    /// Square corners
    Square,
    /// Uniform 9.0 radius for all windows (macOS 26 style)
    Uniform,
}

impl Default for BorderStyle {
    fn default() -> Self {
        BorderStyle::Round
    }
}

/// Color specification - solid or gradient
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ColorSpec {
    Solid { color: u32 },
    Gradient {
        start: u32,
        end: u32,
        #[serde(default = "default_angle")]
        angle: f32,
    },
}

fn default_angle() -> f32 {
    45.0
}

/// Colors for different window states
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowColors {
    #[serde(flatten)]
    pub active: ColorSpec,
    #[serde(flatten)]
    pub inactive: ColorSpec,
    pub background: Option<u32>,
}

/// Complete border settings
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub style: BorderStyle,
    pub width: f32,
    pub hidpi: bool,
    pub colors: WindowColors,
    pub blur_radius: f32,
    pub show_background: bool,
    pub border_order: BorderOrder,
    pub blacklist: Vec<String>,
    pub whitelist: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BorderOrder {
    Above,
    Below,
}

impl Default for BorderOrder {
    fn default() -> Self {
        BorderOrder::Below
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            style: BorderStyle::Round,
            width: 4.0,
            hidpi: false,
            colors: WindowColors {
                active:   ColorSpec::Solid { color: 0xffe1e3e4 }, // JB default
                inactive: ColorSpec::Solid { color: 0x00000000 }, // transparent
                background: None,
            },
            blur_radius: 0.0,
            show_background: false,
            border_order: BorderOrder::Below,
            blacklist: Vec::new(),
            whitelist: Vec::new(),
        }
    }
}

impl Settings {
    /// Get the effective border width (adjusted for hidpi)
    #[allow(dead_code)]
    pub fn effective_width(&self) -> f32 {
        if self.hidpi {
            self.width * 2.0
        } else {
            self.width
        }
    }

    /// Calculate corner radius based on style
    #[allow(dead_code)]
    pub fn corner_radius(&self, window_corner_radius: f32) -> f32 {
        match self.style {
            BorderStyle::Round => window_corner_radius,
            BorderStyle::Square => 0.0,
            BorderStyle::Uniform => 9.0,
        }
    }
}

/// Config file format for ~/.config/edges/edges.toml
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ConfigFile {
    pub style: Option<String>,
    pub width: Option<f32>,
    pub hidpi: Option<bool>,
    pub active_color: Option<String>,
    pub inactive_color: Option<String>,
    pub order: Option<String>,
}

impl Default for ConfigFile {
    fn default() -> Self {
        Self { style: None, width: None, hidpi: None, active_color: None, inactive_color: None, order: None }
    }
}

pub fn parse_hex(s: &str) -> Option<u32> {
    let s = s.trim_start_matches("0x").trim_start_matches("0X");
    u32::from_str_radix(s, 16).ok()
}

impl ConfigFile {
    pub fn apply(&self, settings: &mut Settings) {
        if let Some(ref s) = self.style {
            settings.style = match s.as_str() {
                "round" => BorderStyle::Round,
                "square" => BorderStyle::Square,
                "uniform" => BorderStyle::Uniform,
                _ => settings.style,
            };
        }
        if let Some(w) = self.width { settings.width = w; }
        if let Some(h) = self.hidpi { settings.hidpi = h; }
        if let Some(ref c) = self.active_color {
            if let Some(hex) = parse_hex(c) {
                settings.colors.active = ColorSpec::Solid { color: hex };
            }
        }
        if let Some(ref c) = self.inactive_color {
            if let Some(hex) = parse_hex(c) {
                settings.colors.inactive = ColorSpec::Solid { color: hex };
            }
        }
        if let Some(ref o) = self.order {
            settings.border_order = match o.as_str() {
                "above" => BorderOrder::Above,
                "below" => BorderOrder::Below,
                _ => settings.border_order,
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_settings() {
        let settings = Settings::default();
        assert_eq!(settings.width, 4.0);
        assert!(!settings.hidpi);
    }

    #[test]
    fn test_hidpi_width() {
        let settings = Settings {
            width: 4.0,
            hidpi: true,
            ..Default::default()
        };
        assert_eq!(settings.effective_width(), 8.0);
    }

    #[test]
    fn test_corner_radius() {
        let round = Settings {
            style: BorderStyle::Round,
            ..Default::default()
        };
        let square = Settings {
            style: BorderStyle::Square,
            ..Default::default()
        };
        let uniform = Settings {
            style: BorderStyle::Uniform,
            ..Default::default()
        };

        assert_eq!(round.corner_radius(12.0), 12.0);
        assert_eq!(square.corner_radius(12.0), 0.0);
        assert_eq!(uniform.corner_radius(12.0), 9.0);
    }
}
