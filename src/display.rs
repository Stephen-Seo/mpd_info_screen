use crate::debug_log::{self, log};
use crate::mpd_handler::{InfoFromShared, MPDHandler, MPDHandlerState, MPDPlayState};
use crate::Opt;
use ggez::event::EventHandler;
use ggez::graphics::{
    self, Color, DrawMode, DrawParam, Drawable, Image, Mesh, MeshBuilder, PxScale, Rect, Text,
    TextFragment, Transform,
};
use ggez::input::keyboard::{self, KeyInput};
use ggez::mint::Vector2;
use ggez::{Context, GameError, GameResult};
use image::DynamicImage;
use image::ImageReader;
use std::io::Cursor;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{atomic::Ordering, Arc, RwLockReadGuard};
use std::thread;
use std::time::{Duration, Instant};

const POLL_TIME: Duration = Duration::from_millis(333);
const INIT_FONT_SIZE_RATIO: f32 = 1.4167;
const INIT_FONT_SIZE_X: f32 = 36.0;
const INIT_FONT_SIZE_Y: f32 = INIT_FONT_SIZE_X * INIT_FONT_SIZE_RATIO;
const TEXT_X_OFFSET: f32 = 0.3;
const TEXT_OFFSET_Y_SPACING: f32 = 0.4;
const TEXT_HEIGHT_SCALE: f32 = 0.12;
const ARTIST_HEIGHT_SCALE: f32 = 0.12;
const ALBUM_HEIGHT_SCALE: f32 = 0.12;
const TIMER_HEIGHT_SCALE_RATIO: f32 = 0.875;
const TIMER_HEIGHT_SCALE: f32 = TEXT_HEIGHT_SCALE * TIMER_HEIGHT_SCALE_RATIO;
const MIN_WIDTH_RATIO: f32 = 4.0 / 5.0;
const INCREASE_AMT: f32 = 6.0 / 5.0;
const DECREASE_AMT: f32 = 5.0 / 6.0;

fn seconds_to_time(seconds: f64) -> String {
    let seconds_int: u64 = seconds.floor() as u64;
    let minutes = seconds_int / 60;
    let new_seconds: f64 = seconds - (minutes * 60) as f64;
    let mut result: String;
    if minutes > 0 {
        result = minutes.to_string();
        result.push(':');
        if new_seconds < 10.0 {
            result.push('0');
        }
    } else {
        result = String::new();
    }
    result.push_str(&new_seconds.to_string());
    let idx_result = result.find('.');
    if let Some(idx) = idx_result {
        result.truncate(idx);
    }

    result
}

fn time_to_percentage(total: f64, current: f64) -> String {
    ((100.0f64 * current / total).round() as i32).to_string() + "%"
}

#[cfg(not(feature = "unicode_support"))]
#[allow(clippy::ptr_arg)]
fn string_to_text(
    string: String,
    _loaded_fonts: &mut Vec<(PathBuf, String)>,
    _ctx: &mut Context,
) -> Text {
    Text::new(TextFragment::from(string))
}

#[cfg(feature = "unicode_support")]
fn string_to_text(
    string: String,
    loaded_fonts: &mut Vec<(PathBuf, String)>,
    ctx: &mut Context,
) -> Text {
    use super::unicode_support;

    let mut text = Text::default();
    let mut current_fragment = TextFragment::default();

    if string.is_ascii() {
        current_fragment.text = string;
        text.add(current_fragment);
        return text;
    }

    let find_font =
        |c: char, loaded_fonts: &mut Vec<(PathBuf, String)>, ctx: &mut Context| -> Option<usize> {
            for (idx, (path, _)) in loaded_fonts.iter().enumerate() {
                let result = unicode_support::font_has_char(c, path);
                if result.is_ok() && result.unwrap() {
                    return Some(idx);
                }
            }

            let find_result = unicode_support::get_matching_font_from_char(c);
            if let Ok(path) = find_result {
                let new_font = ggez::graphics::FontData::from_path(ctx, &path);
                if let Ok(font) = new_font {
                    let font_name: String = path
                        .file_name()
                        .expect("Should be valid filename at end of Font path.")
                        .to_str()
                        .expect("Font filename should be valid unicode.")
                        .to_owned();
                    ctx.gfx.add_font(&font_name, font);
                    loaded_fonts.push((path, font_name));
                    return Some(loaded_fonts.len() - 1);
                } else {
                    log(
                        format!("Failed to load {:?}: {:?}", &path, new_font),
                        debug_log::LogState::Error,
                        debug_log::LogLevel::Error,
                    );
                }
            } else {
                log(
                    format!("Failed to find font for {c}"),
                    debug_log::LogState::Error,
                    debug_log::LogLevel::Error,
                );
            }

            None
        };

    let mut prev_is_ascii = true;
    for c in string.chars() {
        if c.is_ascii() {
            if prev_is_ascii {
                current_fragment.text.push(c);
            } else {
                if !current_fragment.text.is_empty() {
                    text.add(current_fragment);
                    current_fragment = Default::default();
                }
                current_fragment.text.push(c);
            }
            prev_is_ascii = true;
        } else {
            let idx_opt = find_font(c, loaded_fonts, ctx);
            if prev_is_ascii {
                if let Some(idx) = idx_opt {
                    if !current_fragment.text.is_empty() {
                        text.add(current_fragment);
                        current_fragment = Default::default();
                    }
                    let (_, font) = &loaded_fonts[idx];
                    current_fragment.font = Some(font.clone());
                }
                current_fragment.text.push(c);
            } else if let Some(idx) = idx_opt {
                let font = &loaded_fonts[idx].1;
                if let Some(current_font) = current_fragment.font.as_ref() {
                    if current_font == font {
                        current_fragment.text.push(c);
                    } else {
                        if !current_fragment.text.is_empty() {
                            text.add(current_fragment);
                            current_fragment = Default::default();
                        }
                        current_fragment.text.push(c);
                        current_fragment.font = Some(font.clone());
                    }
                } else if current_fragment.text.is_empty() {
                    current_fragment.text.push(c);
                    current_fragment.font = Some(font.clone());
                } else {
                    text.add(current_fragment);
                    current_fragment = Default::default();

                    current_fragment.text.push(c);
                    current_fragment.font = Some(font.clone());
                }
            } else {
                if !current_fragment.text.is_empty() && current_fragment.font.is_some() {
                    text.add(current_fragment);
                    current_fragment = Default::default();
                }
                current_fragment.text.push(c);
            }
            prev_is_ascii = false;
        }
    }

    if !current_fragment.text.is_empty() {
        text.add(current_fragment);
    }

    text
}

pub struct MPDDisplay {
    opts: Opt,
    mpd_handler: Result<MPDHandler, String>,
    is_valid: bool,
    is_initialized: bool,
    is_authenticated: bool,
    notice_text: Text,
    poll_instant: Instant,
    shared: Option<InfoFromShared>,
    password_entered: bool,
    dirty_flag: Option<Arc<AtomicBool>>,
    album_art: Option<Image>,
    album_art_draw_transform: Option<Transform>,
    filename_text: Text,
    filename_string_cache: String,
    filename_transform: Transform,
    artist_text: Text,
    artist_string_cache: String,
    artist_transform: Transform,
    title_text: Text,
    title_string_cache: String,
    title_transform: Transform,
    album_text: Text,
    album_string_cache: String,
    album_transform: Transform,
    timer_text: Text,
    timer_text_len: usize,
    timer_transform: Transform,
    timer_x: f32,
    timer_y: f32,
    timer: f64,
    length: f64,
    cached_filename_y: f32,
    cached_album_y: f32,
    cached_artist_y: f32,
    cached_title_y: f32,
    cached_timer_y: f32,
    text_bg_mesh: Option<Mesh>,
    hide_text: bool,
    tried_album_art_in_dir: bool,
    prev_mpd_play_state: MPDPlayState,
    mpd_play_state: MPDPlayState,
    loaded_fonts: Vec<(PathBuf, String)>,
}

impl MPDDisplay {
    pub fn new(_ctx: &mut Context, opts: Opt) -> Self {
        Self {
            opts,
            mpd_handler: Err(String::from("Uninitialized")),
            is_valid: true,
            is_initialized: false,
            is_authenticated: false,
            notice_text: Text::default(),
            poll_instant: Instant::now().checked_sub(POLL_TIME).unwrap(),
            shared: None,
            password_entered: false,
            dirty_flag: None,
            album_art: None,
            album_art_draw_transform: None,
            filename_text: Text::default(),
            filename_transform: Transform::default(),
            artist_text: Text::default(),
            artist_transform: Transform::default(),
            title_text: Text::default(),
            title_transform: Transform::default(),
            timer_text: Text::new("0"),
            timer_text_len: 0,
            timer_transform: Transform::default(),
            timer_x: INIT_FONT_SIZE_X,
            timer_y: INIT_FONT_SIZE_Y,
            timer: 0.0,
            length: 0.0,
            cached_filename_y: 0.0f32,
            cached_album_y: 0.0f32,
            cached_artist_y: 0.0f32,
            cached_title_y: 0.0f32,
            cached_timer_y: 0.0f32,
            text_bg_mesh: None,
            hide_text: false,
            tried_album_art_in_dir: false,
            prev_mpd_play_state: MPDPlayState::Playing,
            mpd_play_state: MPDPlayState::Playing,
            loaded_fonts: Vec::new(),
            filename_string_cache: String::new(),
            artist_string_cache: String::new(),
            title_string_cache: String::new(),
            album_text: Text::default(),
            album_string_cache: String::new(),
            album_transform: Transform::default(),
        }
    }

    fn init_mpd_handler(&mut self) {
        self.mpd_handler = MPDHandler::new(
            self.opts.host,
            self.opts.port,
            self.opts.password.clone().map_or(String::new(), |s| s),
            self.opts.log_level,
        );
        if self.mpd_handler.is_ok() {
            self.is_initialized = true;
            loop {
                self.dirty_flag = self.mpd_handler.as_ref().unwrap().get_dirty_flag().ok();
                if self.dirty_flag.is_some() {
                    break;
                } else {
                    thread::sleep(POLL_TIME);
                }
            }
            log(
                "Successfully initialized MPDHandler",
                debug_log::LogState::Debug,
                self.opts.log_level,
            );
        } else {
            self.is_valid = false;
            log(
                "Failed to initialize MPDHandler",
                debug_log::LogState::Debug,
                self.opts.log_level,
            );
        }
    }

    fn get_album_art_transform(&mut self, ctx: &mut Context, fill_scaled: bool) {
        if fill_scaled {
            if let Some(image) = &self.album_art {
                let drawable_size = ctx.gfx.drawable_size();
                let art_rect: Rect = image.dimensions(ctx).expect("Image should have dimensions");

                // try to fit to width first
                let mut x_scale = drawable_size.0 / art_rect.w;
                let mut y_scale = x_scale;
                let mut new_width = art_rect.w * x_scale;
                let mut new_height = art_rect.h * y_scale;
                if new_height > drawable_size.1.abs() {
                    // fit to height instead
                    y_scale = drawable_size.1.abs() / art_rect.h;
                    x_scale = y_scale;
                    new_width = art_rect.w * x_scale;
                    new_height = art_rect.h * y_scale;
                }

                let offset_x: f32 = (drawable_size.0.abs() - new_width) / 2.0f32;
                let offset_y: f32 = (drawable_size.1.abs() - new_height) / 2.0f32;

                self.album_art_draw_transform = Some(Transform::Values {
                    dest: [offset_x, offset_y].into(),
                    rotation: 0.0f32,
                    scale: [x_scale, y_scale].into(),
                    offset: [0.0f32, 0.0f32].into(),
                });
            } else {
                self.album_art_draw_transform = None;
            }
        } else if let Some(image) = &self.album_art {
            let drawable_size = ctx.gfx.drawable_size();
            let art_rect: Rect = image.dimensions(ctx).expect("Image should have dimensions");
            let offset_x: f32 = (drawable_size.0.abs() - art_rect.w.abs()) / 2.0f32;
            let offset_y: f32 = (drawable_size.1.abs() - art_rect.h.abs()) / 2.0f32;
            self.album_art_draw_transform = Some(Transform::Values {
                dest: [offset_x, offset_y].into(),
                rotation: 0.0f32,
                scale: [1.0f32, 1.0f32].into(),
                offset: [0.0f32, 0.0f32].into(),
            });
        } else {
            self.album_art_draw_transform = None;
        }
    }

    fn get_image_from_data(&mut self, ctx: &mut Context) -> Result<(), String> {
        let mut read_guard_opt: Option<RwLockReadGuard<'_, MPDHandlerState>> = self
            .mpd_handler
            .as_ref()
            .unwrap()
            .get_state_read_guard()
            .ok();
        if read_guard_opt.is_none() {
            return Err(String::from("Failed to get read_guard of MPDHandlerState"));
        } else if !read_guard_opt.as_ref().unwrap().is_art_data_ready() {
            return Err(String::from("MPDHandlerState does not have album art data"));
        }
        let image_ref = read_guard_opt.as_ref().unwrap().get_art_data();

        let mut image_format: image::ImageFormat = image::ImageFormat::Png;
        log(
            format!(
                "Got image_format type {}",
                read_guard_opt.as_ref().unwrap().get_art_type()
            ),
            debug_log::LogState::Debug,
            self.opts.log_level,
        );

        let mut is_unknown_format: bool = false;

        match read_guard_opt.as_ref().unwrap().get_art_type().as_str() {
            "image/png" => image_format = image::ImageFormat::Png,
            "image/jpg" | "image/jpeg" | "JPG" => image_format = image::ImageFormat::Jpeg,
            "image/gif" => image_format = image::ImageFormat::Gif,
            _ => is_unknown_format = true,
        }

        let try_second_art_fetch_method = |tried_in_dir: &mut bool,
                                           album_art: &mut Option<Image>,
                                           read_guard_opt: &mut Option<
            RwLockReadGuard<'_, MPDHandlerState>,
        >,
                                           mpd_handler: &Result<MPDHandler, String>|
         -> Result<(), String> {
            *tried_in_dir = true;
            album_art.take();
            // Drop the "read_guard" so that the "force_try_other_album_art()"
            // can get a "write_guard"
            read_guard_opt.take();
            mpd_handler
                .as_ref()
                .unwrap()
                .force_try_other_album_art()
                .map_err(|_| String::from("Failed to force try other album art fetching method"))?;
            Err("Got unknown format album art image".into())
        };

        if is_unknown_format && !self.tried_album_art_in_dir {
            return try_second_art_fetch_method(
                &mut self.tried_album_art_in_dir,
                &mut self.album_art,
                &mut read_guard_opt,
                &self.mpd_handler,
            );
        }

        let img_result = if is_unknown_format {
            let reader = ImageReader::new(Cursor::new(image_ref));
            let guessed_reader = reader
                .with_guessed_format()
                .map_err(|e| format!("Error: Failed to guess format of album art image: {e}"));
            if let Ok(reader) = guessed_reader {
                reader.decode().map_err(|e| {
                    format!("Error: Failed to decode album art image (guessed format): {e}")
                })
            } else {
                // Convert Ok(_) to Ok(DynamicImage) which will never be used
                // since the if statement covers it.
                guessed_reader.map(|_| -> DynamicImage { unreachable!() })
            }
        } else {
            ImageReader::with_format(Cursor::new(image_ref), image_format)
                .decode()
                .map_err(|e| format!("Error: Failed to decode album art image: {e}"))
        };
        if img_result.is_err() && !self.tried_album_art_in_dir {
            return try_second_art_fetch_method(
                &mut self.tried_album_art_in_dir,
                &mut self.album_art,
                &mut read_guard_opt,
                &self.mpd_handler,
            );
        }
        let img = img_result?;
        let rgba8 = img.to_rgba8();
        let ggez_img = Image::from_pixels(
            ctx,
            rgba8.as_raw(),
            wgpu_types::TextureFormat::Rgba8UnormSrgb,
            rgba8.width(),
            rgba8.height(),
        );

        self.album_art = Some(ggez_img);

        Ok(())
    }

    fn refresh_text_transforms(&mut self, ctx: &mut Context) -> GameResult<()> {
        let drawable_size = ctx.gfx.drawable_size();

        let text_height_scale: f32;
        let album_height_scale: f32;
        let artist_height_scale: f32;
        let timer_height_scale: f32;

        if let Some(forced_scale) = &self.opts.force_text_height_scale {
            text_height_scale = *forced_scale;
            album_height_scale = *forced_scale;
            artist_height_scale = *forced_scale;
            timer_height_scale = *forced_scale * TIMER_HEIGHT_SCALE_RATIO;
        } else {
            text_height_scale = TEXT_HEIGHT_SCALE;
            album_height_scale = ALBUM_HEIGHT_SCALE;
            artist_height_scale = ARTIST_HEIGHT_SCALE;
            timer_height_scale = TIMER_HEIGHT_SCALE;
        }

        let text_height_limit = text_height_scale * drawable_size.1.abs();
        let album_height_limit = album_height_scale * drawable_size.1.abs();
        let artist_height_limit = artist_height_scale * drawable_size.1.abs();
        let timer_height = timer_height_scale * drawable_size.1.abs();

        let mut offset_y: f32 = drawable_size.1;

        let set_transform = |text: &mut Text,
                             transform: &mut Transform,
                             offset_y: &mut f32,
                             y: &mut f32,
                             is_string: bool,
                             is_artist: bool,
                             is_album: bool,
                             timer_x: &mut f32,
                             timer_y: &mut f32| {
            let mut current_x = INIT_FONT_SIZE_X;
            let mut current_y = INIT_FONT_SIZE_Y;
            let mut width_height: Vector2<f32> = Vector2 { x: 0.0, y: 0.0 };
            let mut iteration_count: u8 = 0;
            loop {
                iteration_count += 1;
                if iteration_count > 8 {
                    break;
                }

                for fragment in text.fragments_mut() {
                    fragment.scale = Some(PxScale {
                        x: current_x,
                        y: current_y,
                    });
                }
                width_height = text
                    .measure(ctx)
                    .expect("Should be able to get width/height of text.");

                if is_string {
                    if drawable_size.0 < width_height.x
                        || width_height.y
                            >= (if is_artist {
                                artist_height_limit
                            } else if is_album {
                                album_height_limit
                            } else {
                                text_height_limit
                            })
                    {
                        current_x *= DECREASE_AMT;
                        current_y *= DECREASE_AMT;
                        continue;
                    } else if drawable_size.0 * MIN_WIDTH_RATIO > width_height.x {
                        current_x *= INCREASE_AMT;
                        current_y *= INCREASE_AMT;
                        continue;
                    } else {
                        break;
                    }
                } else {
                    let diff_scale_y = current_y / width_height.y * timer_height;
                    let current_x = current_x * diff_scale_y / current_y;
                    for fragment in text.fragments_mut() {
                        fragment.scale = Some(PxScale {
                            x: current_x,
                            y: diff_scale_y,
                        });
                    }
                    *timer_x = current_x;
                    *timer_y = diff_scale_y;
                    // width = text.width(ctx); // not really used after this
                    width_height.y = text
                        .measure(ctx)
                        .expect("Should be able to get width/height of text.")
                        .y;
                    break;
                }
            }

            *y = *offset_y - width_height.y;
            *transform = Transform::Values {
                dest: [TEXT_X_OFFSET, *offset_y - width_height.y].into(),
                rotation: 0.0,
                scale: [1.0, 1.0].into(),
                offset: [0.0, 0.0].into(),
            };

            *offset_y -= width_height.y + TEXT_OFFSET_Y_SPACING;
        };

        if !self.filename_text.contents().is_empty() && !self.opts.disable_show_filename {
            set_transform(
                &mut self.filename_text,
                &mut self.filename_transform,
                &mut offset_y,
                &mut self.cached_filename_y,
                true,
                false,
                false,
                &mut self.timer_x,
                &mut self.timer_y,
            );
        } else {
            log(
                "filename text is empty",
                debug_log::LogState::Warning,
                self.opts.log_level,
            );
        }

        if !self.album_text.contents().is_empty() && !self.opts.disable_show_album {
            set_transform(
                &mut self.album_text,
                &mut self.album_transform,
                &mut offset_y,
                &mut self.cached_album_y,
                true,
                false,
                true,
                &mut self.timer_x,
                &mut self.timer_y,
            );
        }

        if !self.artist_text.contents().is_empty() && !self.opts.disable_show_artist {
            set_transform(
                &mut self.artist_text,
                &mut self.artist_transform,
                &mut offset_y,
                &mut self.cached_artist_y,
                true,
                true,
                false,
                &mut self.timer_x,
                &mut self.timer_y,
            );
        } else {
            log(
                "artist text is empty",
                debug_log::LogState::Warning,
                self.opts.log_level,
            );
        }

        if !self.title_text.contents().is_empty() && !self.opts.disable_show_title {
            set_transform(
                &mut self.title_text,
                &mut self.title_transform,
                &mut offset_y,
                &mut self.cached_title_y,
                true,
                false,
                false,
                &mut self.timer_x,
                &mut self.timer_y,
            );
        } else {
            log(
                "title text is empty",
                debug_log::LogState::Warning,
                self.opts.log_level,
            );
        }

        set_transform(
            &mut self.timer_text,
            &mut self.timer_transform,
            &mut offset_y,
            &mut self.cached_timer_y,
            false,
            false,
            false,
            &mut self.timer_x,
            &mut self.timer_y,
        );

        self.update_bg_mesh(ctx)?;

        Ok(())
    }

    fn update_bg_mesh(&mut self, ctx: &mut Context) -> GameResult<()> {
        let filename_dimensions = self
            .filename_text
            .dimensions(ctx)
            .expect("Should be able to get dimensions of Text.");
        let album_dimensions = self
            .album_text
            .dimensions(ctx)
            .expect("Should be able to get dimensions of Text.");
        let artist_dimensions = self
            .artist_text
            .dimensions(ctx)
            .expect("Should be able to get dimensions of Text.");
        let title_dimensions = self
            .title_text
            .dimensions(ctx)
            .expect("Should be able to get dimensions of Text.");
        let timer_dimensions = self
            .timer_text
            .dimensions(ctx)
            .expect("Should be able to get dimensions of Text.");

        let mut mesh_builder: MeshBuilder = MeshBuilder::new();
        if !self.opts.disable_show_filename {
            mesh_builder.rectangle(
                DrawMode::fill(),
                Rect {
                    x: TEXT_X_OFFSET,
                    y: self.cached_filename_y,
                    w: filename_dimensions.w,
                    h: filename_dimensions.h,
                },
                Color::from_rgba(0, 0, 0, self.opts.text_bg_opacity),
            )?;
        }
        if !self.opts.disable_show_album {
            mesh_builder.rectangle(
                DrawMode::fill(),
                Rect {
                    x: TEXT_X_OFFSET,
                    y: self.cached_album_y,
                    w: album_dimensions.w,
                    h: album_dimensions.h,
                },
                Color::from_rgba(0, 0, 0, self.opts.text_bg_opacity),
            )?;
        }
        if !self.opts.disable_show_artist {
            mesh_builder.rectangle(
                DrawMode::fill(),
                Rect {
                    x: TEXT_X_OFFSET,
                    y: self.cached_artist_y,
                    w: artist_dimensions.w,
                    h: artist_dimensions.h,
                },
                Color::from_rgba(0, 0, 0, self.opts.text_bg_opacity),
            )?;
        }
        if !self.opts.disable_show_title {
            mesh_builder.rectangle(
                DrawMode::fill(),
                Rect {
                    x: TEXT_X_OFFSET,
                    y: self.cached_title_y,
                    w: title_dimensions.w,
                    h: title_dimensions.h,
                },
                Color::from_rgba(0, 0, 0, self.opts.text_bg_opacity),
            )?;
        }
        if self.mpd_play_state == MPDPlayState::Playing {
            mesh_builder.rectangle(
                DrawMode::fill(),
                Rect {
                    x: TEXT_X_OFFSET,
                    y: self.cached_timer_y,
                    w: timer_dimensions.w,
                    h: timer_dimensions.h,
                },
                Color::from_rgba(0, 0, 0, self.opts.text_bg_opacity),
            )?;
        }
        let mesh: Mesh = Mesh::from_data(ctx, mesh_builder.build());

        self.text_bg_mesh = Some(mesh);

        Ok(())
    }
}

impl EventHandler for MPDDisplay {
    fn update(&mut self, ctx: &mut ggez::Context) -> Result<(), GameError> {
        if !self.is_valid {
            if let Err(mpd_handler_error) = &self.mpd_handler {
                return Err(GameError::EventLoopError(format!(
                    "Failed to initialize MPDHandler: {mpd_handler_error}"
                )));
            } else {
                return Err(GameError::EventLoopError(
                    "Failed to initialize MPDHandler".into(),
                ));
            }
        }

        if !self.is_initialized {
            if self.opts.enable_prompt_password {
                if self.notice_text.contents().is_empty() {
                    self.notice_text = Text::new(TextFragment::new("password: "));
                } else if self.password_entered {
                    self.init_mpd_handler();
                }
            } else {
                self.init_mpd_handler();
            }
        } else if self.password_entered {
            'check_state: loop {
                let result = self.mpd_handler.as_ref().unwrap().is_authenticated();
                if let Ok(true) = result {
                    self.is_authenticated = true;
                    break;
                } else if let Err(()) = result {
                    continue;
                } else {
                    loop {
                        let check_fail_result =
                            self.mpd_handler.as_ref().unwrap().failed_to_authenticate();
                        if let Ok(true) = check_fail_result {
                            {
                                let mpd_handler = self.mpd_handler.clone().unwrap();
                                loop {
                                    let stop_thread_result = mpd_handler.stop_thread();
                                    if stop_thread_result.is_ok() {
                                        break;
                                    }
                                }
                            }
                            self.notice_text = Text::new(TextFragment::new("password: "));
                            self.opts.password = Some(String::new());
                            self.password_entered = false;
                            self.is_initialized = false;
                            break 'check_state;
                        } else if let Err(()) = check_fail_result {
                            continue;
                        } else {
                            break 'check_state;
                        }
                    }
                }
            }
        }

        self.prev_mpd_play_state = self.mpd_play_state;

        if self.is_valid && self.is_initialized && self.poll_instant.elapsed() > POLL_TIME {
            self.poll_instant = Instant::now();
            if self.dirty_flag.is_some()
                && self
                    .dirty_flag
                    .as_ref()
                    .unwrap()
                    .swap(false, Ordering::AcqRel)
            {
                log(
                    "dirty_flag cleared, acquiring shared data...",
                    debug_log::LogState::Debug,
                    self.opts.log_level,
                );
                self.shared = self
                    .mpd_handler
                    .as_ref()
                    .unwrap()
                    .get_mpd_handler_shared_state()
                    .ok();
                if let Some(shared) = &self.shared {
                    if self.notice_text.contents() != shared.error_text {
                        self.notice_text = Text::new(TextFragment::new(shared.error_text.clone()));
                    }
                    if shared.mpd_play_state != MPDPlayState::Playing {
                        if shared.mpd_play_state == MPDPlayState::Stopped {
                            self.title_text = Text::default();
                            self.artist_text = Text::default();
                            self.album_text = Text::default();
                            self.filename_text = Text::default();
                            self.timer = 0.0;
                            self.length = 0.0;
                            self.album_art = None;
                        }
                        self.mpd_play_state = shared.mpd_play_state;
                    } else {
                        self.mpd_play_state = MPDPlayState::Playing;
                        if !shared.title.is_empty() {
                            if shared.title != self.title_string_cache {
                                self.title_string_cache = shared.title.clone();
                                self.title_text = string_to_text(
                                    shared.title.clone(),
                                    &mut self.loaded_fonts,
                                    ctx,
                                );
                                log(
                                    format!("loaded_fonts size is {}", self.loaded_fonts.len()),
                                    debug_log::LogState::Debug,
                                    self.opts.log_level,
                                );
                            }
                        } else {
                            self.dirty_flag
                                .as_ref()
                                .unwrap()
                                .store(true, Ordering::Release);
                        }
                        if !shared.artist.is_empty() {
                            if shared.artist != self.artist_string_cache {
                                self.artist_string_cache = shared.artist.clone();
                                self.artist_text = string_to_text(
                                    shared.artist.clone(),
                                    &mut self.loaded_fonts,
                                    ctx,
                                );
                            }
                        } else {
                            self.dirty_flag
                                .as_ref()
                                .unwrap()
                                .store(true, Ordering::Release);
                        }
                        if !shared.album.is_empty() {
                            if shared.album != self.album_string_cache {
                                self.album_string_cache = shared.album.clone();
                                self.album_text = string_to_text(
                                    shared.album.clone(),
                                    &mut self.loaded_fonts,
                                    ctx,
                                );
                            }
                        } else {
                            self.dirty_flag
                                .as_ref()
                                .unwrap()
                                .store(true, Ordering::Release);
                        }
                        if !shared.filename.is_empty() {
                            if shared.filename != self.filename_string_cache {
                                self.filename_string_cache = shared.filename.clone();
                                if self.filename_text.contents() != shared.filename {
                                    self.album_art = None;
                                    self.tried_album_art_in_dir = false;
                                }
                                self.filename_text = string_to_text(
                                    shared.filename.clone(),
                                    &mut self.loaded_fonts,
                                    ctx,
                                );
                            }
                        } else {
                            self.dirty_flag
                                .as_ref()
                                .unwrap()
                                .store(true, Ordering::Release);
                        }
                        self.timer = shared.pos;
                        self.length = shared.length;
                        self.refresh_text_transforms(ctx)?;
                    }
                } else {
                    log(
                        "Failed to acquire read lock for getting shared data",
                        debug_log::LogState::Debug,
                        self.opts.log_level,
                    );
                }
                if self.album_art.is_none() {
                    let result = self.get_image_from_data(ctx);
                    if let Err(e) = result {
                        log(e, debug_log::LogState::Warning, self.opts.log_level);
                        self.album_art = None;
                        self.album_art_draw_transform = None;
                    } else {
                        self.get_album_art_transform(ctx, !self.opts.do_not_fill_scale_album_art);
                    }
                }
            }
        }

        let delta = ctx.time.delta();
        self.timer += delta.as_secs_f64();
        let mut timer_diff = seconds_to_time(self.length - self.timer);
        if !self.opts.disable_show_percentage {
            timer_diff = timer_diff + " " + &time_to_percentage(self.length, self.timer);
        }
        let timer_diff_len = timer_diff.len();
        self.timer_text = Text::new(timer_diff);
        self.timer_text.set_scale(PxScale {
            x: self.timer_x,
            y: self.timer_y,
        });
        if timer_diff_len != self.timer_text_len {
            self.timer_text_len = timer_diff_len;
            self.update_bg_mesh(ctx)?;
        } else if self.mpd_play_state != MPDPlayState::Playing
            && self.prev_mpd_play_state == MPDPlayState::Playing
        {
            self.update_bg_mesh(ctx)?;
        }

        Ok(())
    }

    fn draw(&mut self, ctx: &mut ggez::Context) -> Result<(), GameError> {
        let mut canvas = graphics::Canvas::from_frame(ctx, Color::BLACK);

        if self.mpd_play_state != MPDPlayState::Stopped
            && self.album_art.is_some()
            && self.album_art_draw_transform.is_some()
        {
            canvas.draw(
                self.album_art.as_ref().unwrap(),
                DrawParam {
                    transform: self.album_art_draw_transform.unwrap(),
                    ..Default::default()
                },
            );
        }

        if !self.hide_text {
            canvas.draw(&self.notice_text, DrawParam::default());

            if self.mpd_play_state != MPDPlayState::Stopped && self.is_valid && self.is_initialized
            {
                if let Some(mesh) = &self.text_bg_mesh {
                    canvas.draw(mesh, DrawParam::default());
                }

                if !self.opts.disable_show_filename {
                    canvas.draw(
                        &self.filename_text,
                        DrawParam {
                            transform: self.filename_transform,
                            ..Default::default()
                        },
                    );
                }

                if !self.opts.disable_show_album {
                    canvas.draw(
                        &self.album_text,
                        DrawParam {
                            transform: self.album_transform,
                            ..Default::default()
                        },
                    );
                }

                if !self.opts.disable_show_artist {
                    canvas.draw(
                        &self.artist_text,
                        DrawParam {
                            transform: self.artist_transform,
                            ..Default::default()
                        },
                    );
                }

                if !self.opts.disable_show_title {
                    canvas.draw(
                        &self.title_text,
                        DrawParam {
                            transform: self.title_transform,
                            ..Default::default()
                        },
                    );
                }

                if self.mpd_play_state == MPDPlayState::Playing {
                    canvas.draw(
                        &self.timer_text,
                        DrawParam {
                            transform: self.timer_transform,
                            ..Default::default()
                        },
                    );
                }
            }
        }

        canvas.finish(ctx)
    }

    fn text_input_event(&mut self, _ctx: &mut Context, character: char) -> Result<(), GameError> {
        if !self.is_initialized && self.opts.enable_prompt_password && !character.is_control() {
            if self.opts.password.is_none() {
                let s = String::from(character);
                self.opts.password = Some(s);
                self.notice_text.add('*');
            } else {
                self.opts.password.as_mut().unwrap().push(character);
                self.notice_text.add('*');
            }
        }

        Ok(())
    }

    fn key_down_event(
        &mut self,
        _ctx: &mut Context,
        input: KeyInput,
        _repeat: bool,
    ) -> Result<(), GameError> {
        if !self.is_initialized && self.opts.enable_prompt_password {
            if input.keycode == Some(keyboard::KeyCode::Back) {
                let s: String = self.notice_text.contents();

                if s.ends_with('*') {
                    self.notice_text = Text::new(TextFragment::new(s[0..(s.len() - 1)].to_owned()));
                }

                if let Some(input_p) = &mut self.opts.password {
                    input_p.pop();
                }
            } else if input.keycode == Some(keyboard::KeyCode::Return) {
                self.password_entered = true;
            }
        } else if input.keycode == Some(keyboard::KeyCode::H) {
            self.hide_text = true;
        }

        Ok(())
    }

    fn key_up_event(&mut self, _ctx: &mut Context, input: KeyInput) -> Result<(), GameError> {
        if input.keycode == Some(keyboard::KeyCode::H) {
            self.hide_text = false;
        }

        Ok(())
    }

    fn resize_event(
        &mut self,
        ctx: &mut Context,
        _width: f32,
        _height: f32,
    ) -> Result<(), GameError> {
        self.get_album_art_transform(ctx, !self.opts.do_not_fill_scale_album_art);
        self.refresh_text_transforms(ctx)
            .expect("Failed to set text transforms");

        Ok(())
    }
}
