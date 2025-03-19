mod fontconfig;
mod freetype;

pub use self::freetype::font_has_char;
#[allow(unused_imports)]
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
        let fetched_path =
            get_matching_font_from_str("text").expect("Should be able to find match for 'text'");
        if !font_has_char('t', &fetched_path).expect("Shouuld be able to check font for 't'") {
            panic!("fetched font does not have 't'");
        }
        if !font_has_char('e', &fetched_path).expect("Shouuld be able to check font for 'e'") {
            panic!("fetched font does not have 'e'");
        }
        if !font_has_char('x', &fetched_path).expect("Shouuld be able to check font for 'x'") {
            panic!("fetched font does not have 'x'");
        }
    }
}
