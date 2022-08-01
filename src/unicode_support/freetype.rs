use std::path::Path;

mod ffi {
    use freetype::freetype::{
        FT_Done_Face, FT_Done_FreeType, FT_Face, FT_FaceRec_, FT_Get_Char_Index, FT_Init_FreeType,
        FT_Library, FT_ModuleRec_, FT_Open_Args, FT_Open_Face, FT_Parameter_, FT_StreamRec_,
        FT_OPEN_PATHNAME,
    };
    use std::ffi::CString;
    use std::path::Path;

    pub struct FTLibrary {
        library: FT_Library,
    }

    impl Drop for FTLibrary {
        fn drop(&mut self) {
            if !self.library.is_null() {
                unsafe {
                    FT_Done_FreeType(self.library);
                }
            }
        }
    }

    impl FTLibrary {
        pub fn new() -> Option<FTLibrary> {
            unsafe {
                let mut library_ptr: FT_Library = 0 as FT_Library;
                if FT_Init_FreeType(&mut library_ptr) == 0 {
                    Some(FTLibrary {
                        library: library_ptr,
                    })
                } else {
                    None
                }
            }
        }

        pub fn get(&self) -> FT_Library {
            self.library
        }
    }

    pub struct FTOpenArgs {
        args: FT_Open_Args,
        // "args" has a pointer to the CString in "pathname", so it must be kept
        #[allow(dead_code)]
        pathname: Option<CString>,
    }

    impl FTOpenArgs {
        pub fn new_with_path(path: &Path) -> Self {
            unsafe {
                let cstring: CString = CString::from_vec_unchecked(
                    path.as_os_str().to_str().unwrap().as_bytes().to_vec(),
                );
                let args = FT_Open_Args {
                    flags: FT_OPEN_PATHNAME,
                    memory_base: std::ptr::null::<u8>(),
                    memory_size: 0,
                    pathname: cstring.as_ptr() as *mut i8,
                    stream: std::ptr::null_mut::<FT_StreamRec_>(),
                    driver: std::ptr::null_mut::<FT_ModuleRec_>(),
                    num_params: 0,
                    params: std::ptr::null_mut::<FT_Parameter_>(),
                };

                FTOpenArgs {
                    args,
                    pathname: Some(cstring),
                }
            }
        }

        #[allow(dead_code)]
        pub fn get_args(&self) -> FT_Open_Args {
            self.args
        }

        pub fn get_ptr(&mut self) -> *mut FT_Open_Args {
            &mut self.args as *mut FT_Open_Args
        }
    }

    pub struct FTFaces {
        faces: Vec<FT_Face>,
    }

    impl Drop for FTFaces {
        fn drop(&mut self) {
            for face in &self.faces {
                unsafe {
                    FT_Done_Face(*face);
                }
            }
        }
    }

    impl FTFaces {
        pub fn new(library: &FTLibrary, args: &mut FTOpenArgs) -> Result<FTFaces, String> {
            let mut faces = FTFaces { faces: Vec::new() };
            unsafe {
                let mut face: FT_Face = 0 as FT_Face;
                // first get number of faces
                let mut result = FT_Open_Face(
                    library.get(),
                    args.get_ptr(),
                    -1,
                    &mut face as *mut *mut FT_FaceRec_,
                );
                if result != 0 {
                    FT_Done_Face(face);
                    return Err(String::from("Failed to get number of faces"));
                }
                let count = (*face).num_faces;

                for i in 0..count {
                    result = FT_Open_Face(
                        library.get(),
                        args.get_ptr(),
                        i,
                        &mut face as *mut *mut FT_FaceRec_,
                    );
                    if result != 0 {
                        FT_Done_Face(face);
                        return Err(String::from("Failed to fetch face"));
                    }
                    faces.faces.push(face);
                }
            }

            Ok(faces)
        }

        pub fn has_char(&self, c: char) -> bool {
            let char_value: u64 = c as u64;

            for face in &self.faces {
                unsafe {
                    let result = FT_Get_Char_Index(*face, char_value);
                    if result != 0 {
                        return true;
                    }
                }
            }

            false
        }
    }
}

pub fn font_has_char(c: char, font_path: &Path) -> Result<bool, String> {
    let library = ffi::FTLibrary::new().ok_or_else(|| String::from("Failed to get FTLibrary"))?;
    let mut args = ffi::FTOpenArgs::new_with_path(font_path);
    let faces = ffi::FTFaces::new(&library, &mut args)?;

    Ok(faces.has_char(c))
}
