use std::{path::PathBuf, str::FromStr};

mod ffi {
    use std::{ffi::CStr, os::raw::c_int};

    mod bindgen {
        #![allow(non_upper_case_globals)]
        #![allow(non_camel_case_types)]
        #![allow(non_snake_case)]
        #![allow(dead_code)]
        #![allow(deref_nullptr)]
        #![allow(clippy::redundant_static_lifetimes)]

        include!(concat!(env!("OUT_DIR"), "/unicode_support_bindings.rs"));
    }

    pub struct FcConfigWr {
        config: *mut bindgen::FcConfig,
    }

    impl Drop for FcConfigWr {
        fn drop(&mut self) {
            if !self.config.is_null() {
                unsafe {
                    bindgen::FcConfigDestroy(self.config);
                }
            }
        }
    }

    impl FcConfigWr {
        pub fn new() -> Result<Self, String> {
            let config = unsafe { bindgen::FcInitLoadConfigAndFonts() };
            if config.is_null() {
                Err(String::from("Failed to create FcConfig"))
            } else {
                Ok(Self { config })
            }
        }

        #[allow(dead_code)]
        pub fn get(&mut self) -> *mut bindgen::FcConfig {
            self.config
        }

        pub fn apply_pattern_to_config(&mut self, pattern: &mut FcPatternWr) -> bool {
            unsafe {
                bindgen::FcConfigSubstitute(
                    self.config,
                    pattern.get(),
                    bindgen::_FcMatchKind_FcMatchPattern,
                ) == bindgen::FcTrue as bindgen::FcBool
            }
        }

        pub fn font_match(&mut self, pattern: &mut FcPatternWr) -> Result<FcPatternWr, String> {
            unsafe {
                let mut result: bindgen::FcResult = 0 as bindgen::FcResult;
                let result_pattern = bindgen::FcFontMatch(
                    self.config,
                    pattern.get(),
                    &mut result as *mut bindgen::FcResult,
                );
                if result != bindgen::_FcResult_FcResultMatch {
                    if !result_pattern.is_null() {
                        bindgen::FcPatternDestroy(result_pattern);
                        return Err(String::from("Failed to FcFontMatch (FcResult is not FcResultMatch; result_pattern is not null)"));
                    } else {
                        return Err(format!(
                            "Failed to FcFontMatch (FcResult is not FcResultMatch; {result:?})"
                        ));
                    }
                } else if result_pattern.is_null() {
                    return Err(String::from(
                        "Failed to FcFontMatch (result_pattern is null)",
                    ));
                }

                Ok(FcPatternWr {
                    pattern: result_pattern,
                })
            }
        }
    }

    pub struct FcCharSetWr {
        charset: *mut bindgen::FcCharSet,
    }

    impl Drop for FcCharSetWr {
        fn drop(&mut self) {
            if !self.charset.is_null() {
                unsafe {
                    bindgen::FcCharSetDestroy(self.charset);
                }
            }
        }
    }

    impl FcCharSetWr {
        #[allow(dead_code)]
        pub fn new_with_str(s: &str) -> Result<Self, String> {
            let charset;
            unsafe {
                let charset_ptr = bindgen::FcCharSetCreate();
                if charset_ptr.is_null() {
                    return Err(String::from("Failed to create FcCharSet with str"));
                }
                charset = FcCharSetWr {
                    charset: charset_ptr,
                };
                for c in s.chars() {
                    if bindgen::FcCharSetAddChar(charset.charset, c as u32)
                        == bindgen::FcFalse as bindgen::FcBool
                    {
                        return Err(String::from("Failed to add chars from str into FcCharSet"));
                    }
                }
            }

            Ok(charset)
        }

        pub fn new_with_char(c: char) -> Result<Self, String> {
            let charset;
            unsafe {
                let charset_ptr = bindgen::FcCharSetCreate();
                if charset_ptr.is_null() {
                    return Err(String::from("Failed to create FcCharSet with char"));
                }
                charset = FcCharSetWr {
                    charset: charset_ptr,
                };
                if bindgen::FcCharSetAddChar(charset.charset, c as u32)
                    == bindgen::FcFalse as bindgen::FcBool
                {
                    return Err(String::from("Failed to add char to FcCharSet"));
                }
            }

            Ok(charset)
        }

        pub fn get(&mut self) -> *mut bindgen::FcCharSet {
            self.charset
        }
    }

    pub struct FcPatternWr {
        pattern: *mut bindgen::FcPattern,
    }

    impl Drop for FcPatternWr {
        fn drop(&mut self) {
            if !self.pattern.is_null() {
                unsafe {
                    bindgen::FcPatternDestroy(self.pattern);
                }
            }
        }
    }

    impl FcPatternWr {
        pub fn new_with_charset(c: &mut FcCharSetWr) -> Result<Self, String> {
            let pattern;
            unsafe {
                let pattern_ptr = bindgen::FcPatternCreate();
                if pattern_ptr.is_null() {
                    return Err(String::from("Failed to FcPatternCreate"));
                }
                pattern = Self {
                    pattern: pattern_ptr,
                };
                bindgen::FcDefaultSubstitute(pattern.pattern);

                let value = bindgen::FcValue {
                    type_: bindgen::_FcType_FcTypeCharSet,
                    u: bindgen::_FcValue__bindgen_ty_1 { c: c.get() },
                };

                if bindgen::FcPatternAdd(
                    pattern.pattern,
                    bindgen::FC_CHARSET as *const _ as *const i8,
                    value,
                    bindgen::FcTrue as bindgen::FcBool,
                ) == bindgen::FcFalse as bindgen::FcBool
                {
                    return Err(String::from("Failed to add FcCharSet to new Pattern"));
                }
            }

            Ok(pattern)
        }

        pub fn get(&mut self) -> *mut bindgen::FcPattern {
            self.pattern
        }

        pub fn get_count(&self) -> c_int {
            unsafe { bindgen::FcPatternObjectCount(self.pattern) }
        }

        pub fn filter_to_filenames(&self) -> Result<Self, String> {
            let pattern;
            unsafe {
                let mut file_object_set_filter = FcObjectSetWr::new_file_object_set()?;
                let pattern_ptr =
                    bindgen::FcPatternFilter(self.pattern, file_object_set_filter.get());
                if pattern_ptr.is_null() {
                    return Err(String::from("Failed to FcPatternFilter"));
                }
                pattern = Self {
                    pattern: pattern_ptr,
                };
            }

            Ok(pattern)
        }

        pub fn get_filename_contents(&self) -> Result<Vec<String>, String> {
            let mut vec: Vec<String> = Vec::new();
            let count = self.get_count();
            unsafe {
                let mut value = bindgen::FcValue {
                    type_: 0,
                    u: bindgen::_FcValue__bindgen_ty_1 { i: 0 },
                };

                for i in 0..count {
                    if bindgen::FcPatternGet(
                        self.pattern,
                        bindgen::FC_FILE as *const _ as *const i8,
                        i,
                        &mut value as *mut bindgen::FcValue,
                    ) == bindgen::_FcResult_FcResultMatch
                        && value.type_ == bindgen::_FcType_FcTypeString
                    {
                        let cs = CStr::from_ptr(value.u.s as *const i8);
                        vec.push(
                            cs.to_str()
                                .map_err(|_| String::from("Failed to convert CStr to String"))?
                                .to_owned(),
                        );
                    }
                }
            }

            Ok(vec)
        }
    }

    struct FcObjectSetWr {
        object_set: *mut bindgen::FcObjectSet,
    }

    impl Drop for FcObjectSetWr {
        fn drop(&mut self) {
            unsafe {
                if !self.object_set.is_null() {
                    bindgen::FcObjectSetDestroy(self.object_set);
                }
            }
        }
    }

    impl FcObjectSetWr {
        pub fn new_file_object_set() -> Result<Self, String> {
            let object_set;
            unsafe {
                let object_set_ptr = bindgen::FcObjectSetCreate();
                if object_set_ptr.is_null() {
                    return Err(String::from("Failed to FcObjectSetCreate"));
                }

                object_set = Self {
                    object_set: object_set_ptr,
                };

                if bindgen::FcObjectSetAdd(
                    object_set.object_set,
                    bindgen::FC_FILE as *const _ as *const i8,
                ) == bindgen::FcFalse as bindgen::FcBool
                {
                    return Err(String::from(
                        "Failed to add \"FC_FILE\" with FcObjectSetAdd",
                    ));
                }
            }

            Ok(object_set)
        }

        pub fn get(&mut self) -> *mut bindgen::FcObjectSet {
            self.object_set
        }
    }
}

#[allow(dead_code)]
pub fn get_matching_font_from_str(s: &str) -> Result<PathBuf, String> {
    let mut config = ffi::FcConfigWr::new()?;
    let mut charset = ffi::FcCharSetWr::new_with_str(s)?;
    let mut search_pattern = ffi::FcPatternWr::new_with_charset(&mut charset)?;
    if !config.apply_pattern_to_config(&mut search_pattern) {
        return Err(String::from("Failed to apply_pattern_to_config"));
    }
    let result_pattern = config.font_match(&mut search_pattern)?;
    let filtered_pattern = result_pattern.filter_to_filenames()?;
    let result_vec = filtered_pattern.get_filename_contents()?;

    if result_vec.is_empty() {
        Err(String::from(
            "Empty result_vec for get_matching_font_from_str",
        ))
    } else {
        PathBuf::from_str(&result_vec[0]).map_err(|e| e.to_string())
    }
}

pub fn get_matching_font_from_char(c: char) -> Result<PathBuf, String> {
    let mut config = ffi::FcConfigWr::new()?;
    let mut charset = ffi::FcCharSetWr::new_with_char(c)?;
    let mut search_pattern = ffi::FcPatternWr::new_with_charset(&mut charset)?;
    if !config.apply_pattern_to_config(&mut search_pattern) {
        return Err(String::from("Failed to apply_pattern_to_config"));
    }
    let result_pattern = config.font_match(&mut search_pattern)?;
    let filtered_pattern = result_pattern.filter_to_filenames()?;
    let result_vec = filtered_pattern.get_filename_contents()?;

    if result_vec.is_empty() {
        Err(String::from(
            "Empty result_vec for get_matching_font_from_char",
        ))
    } else {
        PathBuf::from_str(&result_vec[0]).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ascii_fetching() {
        let fetched_path =
            get_matching_font_from_char('a').expect("Should be able to find match for 'a'");
        println!("{:?}", fetched_path);
    }
}
