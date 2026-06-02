use ratatui::style::Color;

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

