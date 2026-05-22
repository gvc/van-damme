use ratatui::style::Color;

// Cyberpunk-inspired color palette
pub const BG: Color = Color::Rgb(0x12, 0x1a, 0x24); // dark navy, primary surface
pub const ORANGE: Color = Color::Rgb(0xc8, 0x68, 0x18); // amber-orange, primary accent
pub const ORANGE_BRIGHT: Color = Color::Rgb(0xf0, 0x90, 0x20); // gold-neon, bright accent
pub const BLUE: Color = Color::Rgb(0x3a, 0x5a, 0x7a); // medium steel, borders/outlines
pub const GRAY: Color = Color::Rgb(0x50, 0x58, 0x60); // cool gray, brackets/operators
pub const GRAY_DIM: Color = Color::Rgb(0x50, 0x60, 0x70); // muted gray, inactive/comments
pub const TEXT: Color = Color::Rgb(0x8a, 0x9a, 0xaa); // light steel, default text
pub const CYAN: Color = Color::Rgb(0x40, 0xa0, 0xb0); // muted cyan, active windows/info
pub const CYAN_VIVID: Color = Color::Rgb(0x00, 0xd8, 0xe0); // electric cyan, bright indicators
pub const GREEN: Color = Color::Rgb(0x40, 0xa0, 0x40); // terminal green, success/git add
pub const ERROR: Color = Color::Rgb(0x80, 0x20, 0x20); // dark crimson, errors/critical
pub const SESSION_NAME: Color = Color::Rgb(0xe0, 0xa0, 0x30); // warm gold, session names
pub const NEON_PINK: Color = Color::Rgb(0xe0, 0x30, 0x80); // cyberpunk magenta accent
