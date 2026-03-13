use ratatui::style::Color;

// Syndicate-inspired color palette
pub const BG: Color = Color::Rgb(26, 26, 46); // dark charcoal background
pub const ORANGE: Color = Color::Rgb(200, 90, 26); // warm amber/orange — primary accent
pub const ORANGE_BRIGHT: Color = Color::Rgb(220, 120, 40); // brighter orange for focused elements
pub const BLUE: Color = Color::Rgb(74, 106, 138); // muted steel blue — ghost/secondary
pub const GRAY: Color = Color::Rgb(60, 60, 80); // mid-gray for unfocused borders
pub const GRAY_DIM: Color = Color::Rgb(90, 90, 110); // dimmed text
pub const TEXT: Color = Color::Rgb(180, 180, 195); // light gray text
pub const ERROR: Color = Color::Rgb(200, 60, 60); // muted red for errors
pub const SESSION_NAME: Color = ORANGE; // session names in list
