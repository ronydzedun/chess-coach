// SPDX-License-Identifier: GPL-3.0-or-later
//! Bundled, attributed SVG pieces and validated per-user appearance preferences.
use chess_core::{Color, Role};
use serde::{Deserialize, Serialize};
use std::{
    fmt, fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PieceSet {
    #[default]
    Merida,
    Celtic,
    RhosGfx,
}

impl PieceSet {
    pub const ALL: [Self; 3] = [Self::Merida, Self::Celtic, Self::RhosGfx];

    pub fn svg_bytes(self, color: Color, role: Role) -> &'static [u8] {
        match (self, color, role) {
            (Self::Merida, Color::White, Role::Pawn) => {
                include_bytes!("../../../assets/pieces/merida/wP.svg")
            }
            (Self::Merida, Color::White, Role::Knight) => {
                include_bytes!("../../../assets/pieces/merida/wN.svg")
            }
            (Self::Merida, Color::White, Role::Bishop) => {
                include_bytes!("../../../assets/pieces/merida/wB.svg")
            }
            (Self::Merida, Color::White, Role::Rook) => {
                include_bytes!("../../../assets/pieces/merida/wR.svg")
            }
            (Self::Merida, Color::White, Role::Queen) => {
                include_bytes!("../../../assets/pieces/merida/wQ.svg")
            }
            (Self::Merida, Color::White, Role::King) => {
                include_bytes!("../../../assets/pieces/merida/wK.svg")
            }
            (Self::Merida, Color::Black, Role::Pawn) => {
                include_bytes!("../../../assets/pieces/merida/bP.svg")
            }
            (Self::Merida, Color::Black, Role::Knight) => {
                include_bytes!("../../../assets/pieces/merida/bN.svg")
            }
            (Self::Merida, Color::Black, Role::Bishop) => {
                include_bytes!("../../../assets/pieces/merida/bB.svg")
            }
            (Self::Merida, Color::Black, Role::Rook) => {
                include_bytes!("../../../assets/pieces/merida/bR.svg")
            }
            (Self::Merida, Color::Black, Role::Queen) => {
                include_bytes!("../../../assets/pieces/merida/bQ.svg")
            }
            (Self::Merida, Color::Black, Role::King) => {
                include_bytes!("../../../assets/pieces/merida/bK.svg")
            }
            (Self::Celtic, Color::White, Role::Pawn) => {
                include_bytes!("../../../assets/pieces/celtic/wP.svg")
            }
            (Self::Celtic, Color::White, Role::Knight) => {
                include_bytes!("../../../assets/pieces/celtic/wN.svg")
            }
            (Self::Celtic, Color::White, Role::Bishop) => {
                include_bytes!("../../../assets/pieces/celtic/wB.svg")
            }
            (Self::Celtic, Color::White, Role::Rook) => {
                include_bytes!("../../../assets/pieces/celtic/wR.svg")
            }
            (Self::Celtic, Color::White, Role::Queen) => {
                include_bytes!("../../../assets/pieces/celtic/wQ.svg")
            }
            (Self::Celtic, Color::White, Role::King) => {
                include_bytes!("../../../assets/pieces/celtic/wK.svg")
            }
            (Self::Celtic, Color::Black, Role::Pawn) => {
                include_bytes!("../../../assets/pieces/celtic/bP.svg")
            }
            (Self::Celtic, Color::Black, Role::Knight) => {
                include_bytes!("../../../assets/pieces/celtic/bN.svg")
            }
            (Self::Celtic, Color::Black, Role::Bishop) => {
                include_bytes!("../../../assets/pieces/celtic/bB.svg")
            }
            (Self::Celtic, Color::Black, Role::Rook) => {
                include_bytes!("../../../assets/pieces/celtic/bR.svg")
            }
            (Self::Celtic, Color::Black, Role::Queen) => {
                include_bytes!("../../../assets/pieces/celtic/bQ.svg")
            }
            (Self::Celtic, Color::Black, Role::King) => {
                include_bytes!("../../../assets/pieces/celtic/bK.svg")
            }
            (Self::RhosGfx, Color::White, Role::Pawn) => {
                include_bytes!("../../../assets/pieces/rhosgfx/wP.svg")
            }
            (Self::RhosGfx, Color::White, Role::Knight) => {
                include_bytes!("../../../assets/pieces/rhosgfx/wN.svg")
            }
            (Self::RhosGfx, Color::White, Role::Bishop) => {
                include_bytes!("../../../assets/pieces/rhosgfx/wB.svg")
            }
            (Self::RhosGfx, Color::White, Role::Rook) => {
                include_bytes!("../../../assets/pieces/rhosgfx/wR.svg")
            }
            (Self::RhosGfx, Color::White, Role::Queen) => {
                include_bytes!("../../../assets/pieces/rhosgfx/wQ.svg")
            }
            (Self::RhosGfx, Color::White, Role::King) => {
                include_bytes!("../../../assets/pieces/rhosgfx/wK.svg")
            }
            (Self::RhosGfx, Color::Black, Role::Pawn) => {
                include_bytes!("../../../assets/pieces/rhosgfx/bP.svg")
            }
            (Self::RhosGfx, Color::Black, Role::Knight) => {
                include_bytes!("../../../assets/pieces/rhosgfx/bN.svg")
            }
            (Self::RhosGfx, Color::Black, Role::Bishop) => {
                include_bytes!("../../../assets/pieces/rhosgfx/bB.svg")
            }
            (Self::RhosGfx, Color::Black, Role::Rook) => {
                include_bytes!("../../../assets/pieces/rhosgfx/bR.svg")
            }
            (Self::RhosGfx, Color::Black, Role::Queen) => {
                include_bytes!("../../../assets/pieces/rhosgfx/bQ.svg")
            }
            (Self::RhosGfx, Color::Black, Role::King) => {
                include_bytes!("../../../assets/pieces/rhosgfx/bK.svg")
            }
        }
    }
}

impl fmt::Display for PieceSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Merida => "Merida",
            Self::Celtic => "Celtic",
            Self::RhosGfx => "RhosGFX",
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoardPalette {
    pub name: String,
    pub light: String,
    pub dark: String,
}

impl BoardPalette {
    pub fn new(name: &str, light: &str, dark: &str) -> Result<Self, String> {
        let palette = Self {
            name: name.trim().into(),
            light: light.to_ascii_uppercase(),
            dark: dark.to_ascii_uppercase(),
        };
        palette.validate()?;
        Ok(palette)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.name.is_empty()
            || self.name.chars().count() > 40
            || self.name.trim() != self.name
            || self.name.chars().any(char::is_control)
        {
            return Err(
                "Palette names must have 1–40 characters and no control characters.".into(),
            );
        }
        for color in [&self.light, &self.dark] {
            if color.len() != 7
                || !color.starts_with('#')
                || !color.as_bytes()[1..].iter().all(u8::is_ascii_hexdigit)
            {
                return Err("Colors must use the #RRGGBB format.".into());
            }
        }
        if self.light.eq_ignore_ascii_case(&self.dark) {
            return Err("Light and dark square colors must differ.".into());
        }
        Ok(())
    }

    /// Invalid public-field values fall back to the classic board colors.
    pub fn colors(&self) -> (iced::Color, iced::Color) {
        fn parse(value: &str) -> Option<iced::Color> {
            let hex = value.strip_prefix('#')?;
            if hex.len() != 6 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                return None;
            }
            let rgb = u32::from_str_radix(hex, 16).ok()?;
            Some(iced::Color::from_rgb8(
                (rgb >> 16) as u8,
                (rgb >> 8) as u8,
                rgb as u8,
            ))
        }
        (
            parse(&self.light).unwrap_or(iced::Color::from_rgb8(240, 217, 181)),
            parse(&self.dark).unwrap_or(iced::Color::from_rgb8(181, 136, 99)),
        )
    }

    pub fn presets() -> Vec<Self> {
        [
            ("Classic", "#F0D9B5", "#B58863"),
            ("Green", "#EBECD0", "#779556"),
            ("Blue", "#DEE3E6", "#6B8BA4"),
            ("Slate", "#D8DEE9", "#67758D"),
        ]
        .into_iter()
        .map(|(name, light, dark)| Self {
            name: name.into(),
            light: light.into(),
            dark: dark.into(),
        })
        .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub piece_set: PieceSet,
    pub engine_path: Option<String>,
    pub custom_palettes: Vec<BoardPalette>,
    pub selected_palette: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            piece_set: PieceSet::default(),
            engine_path: None,
            custom_palettes: Vec::new(),
            selected_palette: "Classic".into(),
        }
    }
}

impl Settings {
    pub fn palettes(&self) -> Vec<BoardPalette> {
        let mut palettes = BoardPalette::presets();
        palettes.extend(self.custom_palettes.clone());
        palettes
    }

    pub fn active_palette(&self) -> BoardPalette {
        self.palettes()
            .into_iter()
            .find(|p| p.name == self.selected_palette)
            .unwrap_or_else(|| BoardPalette::presets().remove(0))
    }

    pub fn add_palette(&mut self, palette: BoardPalette) -> Result<(), String> {
        palette.validate()?;
        if self.custom_palettes.len() >= 32 {
            return Err("You can save up to 32 custom palettes.".into());
        }
        if self
            .palettes()
            .iter()
            .any(|p| p.name.to_lowercase() == palette.name.to_lowercase())
        {
            return Err("A palette with this name already exists.".into());
        }
        self.selected_palette = palette.name.clone();
        self.custom_palettes.push(palette);
        Ok(())
    }

    pub fn remove_palette(&mut self, name: &str) {
        self.custom_palettes.retain(|p| p.name != name);
        if !self
            .palettes()
            .iter()
            .any(|p| p.name == self.selected_palette)
        {
            self.selected_palette = "Classic".into();
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self
            .engine_path
            .as_ref()
            .is_some_and(|path| path.is_empty() || path.len() > 4096 || path.contains('\0'))
        {
            return Err("Engine path must contain 1–4096 bytes without NUL characters.".into());
        }
        let mut checked = Self::default();
        for palette in &self.custom_palettes {
            checked.add_palette(palette.clone())?;
        }
        if !self
            .palettes()
            .iter()
            .any(|p| p.name == self.selected_palette)
        {
            return Err("The selected palette does not exist.".into());
        }
        Ok(())
    }

    pub fn config_path() -> Result<PathBuf, String> {
        directories::ProjectDirs::from("org", "chess-coach", "chess-coach")
            .map(|dirs| dirs.config_dir().join("appearance.json"))
            .ok_or_else(|| "Could not locate the user settings directory.".into())
    }

    pub fn load() -> Result<Self, String> {
        Self::load_from(&Self::config_path()?)
    }
    pub fn save(&self) -> Result<(), String> {
        self.save_to(&Self::config_path()?)
    }

    fn load_from(path: &Path) -> Result<Self, String> {
        let file = match fs::File::open(path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default());
            }
            Err(error) => return Err(format!("Could not read settings: {error}")),
        };
        let mut bytes = Vec::new();
        file.take(65_537)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() > 65_536 {
            return Err("The settings file is too large.".into());
        }
        let settings: Self =
            serde_json::from_slice(&bytes).map_err(|e| format!("Invalid settings file: {e}"))?;
        settings.validate()?;
        Ok(settings)
    }

    fn save_to(&self, path: &Path) -> Result<(), String> {
        self.validate()?;
        let parent = path.parent().ok_or("Invalid settings path.")?;
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        let bytes = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        // Same-directory replacement keeps the old complete file on write failure.
        let mut temporary = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
        temporary.write_all(&bytes).map_err(|e| e.to_string())?;
        temporary.as_file().sync_all().map_err(|e| e.to_string())?;
        temporary.persist(path).map_err(|e| e.to_string())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_custom_palettes_and_duplicate_names() {
        assert!(BoardPalette::new("", "#FFFFFF", "#000000").is_err());
        assert!(BoardPalette::new("Bad", "#FFFFFF", "#ffffff").is_err());
        assert!(BoardPalette::new("Bad", "#GGGGGG", "#000000").is_err());
        assert!(BoardPalette::new(&"x".repeat(41), "#FFFFFF", "#000000").is_err());
        let mut settings = Settings::default();
        assert!(
            settings
                .add_palette(BoardPalette::new("classic", "#FFFFFF", "#000000").unwrap())
                .is_err()
        );
        settings
            .add_palette(BoardPalette::new("Custom", "#abcdef", "#123456").unwrap())
            .unwrap();
        assert_eq!(settings.active_palette().light, "#ABCDEF");
        settings.remove_palette("Custom");
        assert_eq!(settings.selected_palette, "Classic");
    }

    #[test]
    fn settings_round_trip_missing_malformed_and_failed_save() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("appearance.json");
        assert_eq!(Settings::load_from(&path).unwrap(), Settings::default());
        let mut settings = Settings {
            piece_set: PieceSet::Celtic,
            engine_path: Some("/my engine/stockfish".into()),
            ..Settings::default()
        };
        settings
            .add_palette(BoardPalette::new("My board", "#AABBCC", "#223344").unwrap())
            .unwrap();
        settings.save_to(&path).unwrap();
        assert_eq!(Settings::load_from(&path).unwrap(), settings);
        let saved = fs::read(&path).unwrap();
        settings.selected_palette = "missing".into();
        assert!(settings.save_to(&path).is_err());
        assert_eq!(fs::read(&path).unwrap(), saved);
        fs::write(&path, b"not JSON").unwrap();
        assert!(Settings::load_from(&path).is_err());
        fs::write(
            &path,
            br##"{"custom_palettes":[{"name":"bad","light":"#oops","dark":"#000000"}]}"##,
        )
        .unwrap();
        assert!(Settings::load_from(&path).is_err());
        fs::write(&path, vec![b' '; 65_537]).unwrap();
        assert!(Settings::load_from(&path).is_err());
    }

    #[test]
    fn legacy_settings_keep_appearance_and_default_to_engine_discovery() {
        let settings: Settings =
            serde_json::from_str(r#"{"piece_set":"Celtic","selected_palette":"Blue"}"#).unwrap();
        settings.validate().unwrap();
        assert_eq!(settings.piece_set, PieceSet::Celtic);
        assert_eq!(settings.selected_palette, "Blue");
        assert!(settings.engine_path.is_none());
    }

    #[test]
    fn colors_parse_rgb_and_invalid_fields_fall_back() {
        let mut palette = BoardPalette::new("Example", "#123456", "#ABCDEF").unwrap();
        assert_eq!(
            palette.colors(),
            (
                iced::Color::from_rgb8(18, 52, 86),
                iced::Color::from_rgb8(171, 205, 239)
            )
        );
        palette.light = "👑".into();
        palette.dark = "#invalid".into();
        assert_eq!(palette.colors(), BoardPalette::presets()[0].colors());
    }

    #[test]
    fn every_set_has_twelve_svg_assets() {
        for set in PieceSet::ALL {
            for color in [Color::White, Color::Black] {
                for role in [
                    Role::Pawn,
                    Role::Knight,
                    Role::Bishop,
                    Role::Rook,
                    Role::Queen,
                    Role::King,
                ] {
                    let source = std::str::from_utf8(set.svg_bytes(color, role)).unwrap();
                    assert!(source.contains("<svg"));
                }
            }
        }
    }
}
