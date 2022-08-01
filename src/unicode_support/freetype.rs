use std::path::Path;

mod ffi {
    use freetype::freetype::{
        FT_Done_Face, FT_Done_Library, FT_Face, FT_FaceRec_, FT_Get_Char_Index, FT_Init_FreeType,
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
                    FT_Done_Library(self.library);
                }
            }
        }
    }

    impl FTLibrary {
        pub fn new() -> Option<FTLibrary> {
            unsafe {
                let mut library_ptr: FT_Library = 0 as FT_Library;
                if FT_Init_FreeType(&mut library_ptr) != 0 {
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
                    memory_base: 0 as *const u8,
                    memory_size: 0,
                    pathname: cstring.as_ptr() as *mut i8,
                    stream: 0 as *mut FT_StreamRec_,
                    driver: 0 as *mut FT_ModuleRec_,
                    num_params: 0,
                    params: 0 as *mut FT_Parameter_,
                };

                FTOpenArgs {
                    args,
                    pathname: Some(cstring),
                }
            }
        }

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
        pub fn new(library: &FTLibrary, args: &mut FTOpenArgs) -> Result<FTFaces, ()> {
            let mut faces = FTFaces { faces: Vec::new() };
            unsafe {
                let count;
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
                    return Err(());
                }
                count = (*face).num_faces;

                for i in 0..count {
                    result = FT_Open_Face(
                        library.get(),
                        args.get_ptr(),
                        i,
                        &mut face as *mut *mut FT_FaceRec_,
                    );
                    if result != 0 {
                        FT_Done_Face(face);
                        return Err(());
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

pub fn font_has_char(c: char, font_path: &Path) -> Result<bool, ()> {
    let library = ffi::FTLibrary::new().ok_or(())?;
    let mut args = ffi::FTOpenArgs::new_with_path(font_path);
    let faces = ffi::FTFaces::new(&library, &mut args)?;

    Ok(faces.has_char(c))
}
