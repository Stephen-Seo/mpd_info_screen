mod fontconfig;
mod freetype;

pub use self::freetype::font_has_char;
pub use fontconfig::{get_matching_font_from_char, get_matching_font_from_str};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ascii_verify() {
        let fetched_path =
            get_matching_font_from_char('a').expect("Should be able to find match for 'a'");
        if !font_has_char('a', &fetched_path).expect("Should be able to check font for 'a'") {
            panic!("fetched font does not have 'a'");
        }
    }
}
