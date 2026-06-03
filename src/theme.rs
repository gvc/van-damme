use ratatui::style::Color;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct Theme {
    pub bg: Color,
    pub text: Color,
    pub border: Color,
    pub border_bright: Color,
    pub accent: Color,
    pub accent_bright: Color,
    pub gray: Color,
    pub gray_dim: Color,
    pub cyan: Color,
    pub cyan_vivid: Color,
    pub green: Color,
    pub error: Color,
    pub session_name: Color,
    pub neon_pink: Color,
}

pub const SYNDICATE: Theme = Theme {
    bg: Color::Rgb(0x12, 0x1a, 0x24),
    text: Color::Rgb(0x8a, 0x9a, 0xaa),
    border: Color::Rgb(0x3a, 0x5a, 0x7a),
    border_bright: Color::Rgb(0xc8, 0x68, 0x18),
    accent: Color::Rgb(0xc8, 0x68, 0x18),
    accent_bright: Color::Rgb(0xf0, 0x90, 0x20),
    gray: Color::Rgb(0x50, 0x58, 0x60),
    gray_dim: Color::Rgb(0x50, 0x60, 0x70),
    cyan: Color::Rgb(0x40, 0xa0, 0xb0),
    cyan_vivid: Color::Rgb(0x00, 0xd8, 0xe0),
    green: Color::Rgb(0x40, 0xa0, 0x40),
    error: Color::Rgb(0x80, 0x20, 0x20),
    session_name: Color::Rgb(0xe0, 0xa0, 0x30),
    neon_pink: Color::Rgb(0xe0, 0x30, 0x80),
};

fn parse_hex(s: &str) -> Option<Color> {
    let s = s.trim().trim_start_matches('#');
    if s.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&s[0..2], 16).ok()?;
    let g = u8::from_str_radix(&s[2..4], 16).ok()?;
    let b = u8::from_str_radix(&s[4..6], 16).ok()?;
    Some(Color::Rgb(r, g, b))
}

/// Parse a theme TOML file. Missing fields fall back to SYNDICATE values.
pub fn parse_theme_file(path: &Path) -> Theme {
    let Ok(contents) = std::fs::read_to_string(path) else {
        return SYNDICATE.clone();
    };
    let Ok(table) = contents.parse::<toml::Table>() else {
        return SYNDICATE.clone();
    };
    let get = |key: &str, fallback: Color| -> Color {
        table
            .get(key)
            .and_then(|v| v.as_str())
            .and_then(parse_hex)
            .unwrap_or(fallback)
    };
    Theme {
        bg:           get("bg",           SYNDICATE.bg),
        text:         get("text",         SYNDICATE.text),
        border:       get("border",       SYNDICATE.border),
        border_bright:get("border_bright",SYNDICATE.border_bright),
        accent:       get("accent",       SYNDICATE.accent),
        accent_bright:get("accent_bright",SYNDICATE.accent_bright),
        gray:         get("gray",         SYNDICATE.gray),
        gray_dim:     get("gray_dim",     SYNDICATE.gray_dim),
        cyan:         get("cyan",         SYNDICATE.cyan),
        cyan_vivid:   get("cyan_vivid",   SYNDICATE.cyan_vivid),
        green:        get("green",        SYNDICATE.green),
        error:        get("error",        SYNDICATE.error),
        session_name: get("session_name", SYNDICATE.session_name),
        neon_pink:    get("neon_pink",    SYNDICATE.neon_pink),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_parse_hex_valid() {
        assert_eq!(parse_hex("#121a24"), Some(Color::Rgb(0x12, 0x1a, 0x24)));
        assert_eq!(parse_hex("f09020"), Some(Color::Rgb(0xf0, 0x90, 0x20)));
    }

    #[test]
    fn test_parse_hex_invalid() {
        assert_eq!(parse_hex(""), None);
        assert_eq!(parse_hex("#gggggg"), None);
        assert_eq!(parse_hex("#12345"), None);
    }

    #[test]
    fn test_parse_theme_file_missing_falls_back_to_syndicate() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("nonexistent.toml");
        let t = parse_theme_file(&path);
        assert_eq!(t.bg, SYNDICATE.bg);
        assert_eq!(t.accent, SYNDICATE.accent);
    }

    #[test]
    fn test_parse_theme_file_partial_overrides() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("theme.toml");
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(f, "bg = \"#2a2520\"").unwrap();
        writeln!(f, "accent = \"#e78a4e\"").unwrap();
        let t = parse_theme_file(&path);
        assert_eq!(t.bg, Color::Rgb(0x2a, 0x25, 0x20));
        assert_eq!(t.accent, Color::Rgb(0xe7, 0x8a, 0x4e));
        // unset fields fall back
        assert_eq!(t.text, SYNDICATE.text);
    }

    #[test]
    fn test_parse_theme_file_invalid_toml_falls_back() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("bad.toml");
        std::fs::write(&path, "not valid toml !!!").unwrap();
        let t = parse_theme_file(&path);
        assert_eq!(t.bg, SYNDICATE.bg);
    }
}
