mod fontconfig;
mod freetype;

pub use self::freetype::font_has_char;
pub use fontconfig::{get_matching_font_from_char, get_matching_font_from_str};
