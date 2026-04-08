use ratatui::style::Color;

// Syndicate-inspired color palette — sourced from theme-syndicate.toml
pub const BG: Color = Color::Rgb(0x12, 0x1a, 0x24); // bg — dark navy, primary surface
pub const ORANGE: Color = Color::Rgb(0xc0, 0x60, 0x20); // orange — burnt orange, primary accent
pub const ORANGE_BRIGHT: Color = Color::Rgb(0xe0, 0x80, 0x30); // orange_light — bright amber
pub const BLUE: Color = Color::Rgb(0x3a, 0x5a, 0x7a); // blue — medium steel, borders/outlines
pub const GRAY: Color = Color::Rgb(0x50, 0x58, 0x60); // gray — cool gray, brackets/operators
pub const GRAY_DIM: Color = Color::Rgb(0x50, 0x60, 0x70); // fg_dim — muted gray, inactive/comments
pub const TEXT: Color = Color::Rgb(0x8a, 0x9a, 0xaa); // fg — light steel, default text
pub const ERROR: Color = Color::Rgb(0x80, 0x20, 0x20); // red — dark crimson, errors/critical
pub const SESSION_NAME: Color = Color::Rgb(0xe0, 0xa0, 0x30); // gold — warm gold, numbers/special values
