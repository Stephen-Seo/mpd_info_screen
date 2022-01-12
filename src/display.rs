use crate::debug_log::{self, log};
use crate::mpd_handler::{InfoFromShared, MPDHandler, MPDHandlerState};
use crate::Opt;
use ggez::event::{self, EventHandler};
use ggez::graphics::{
    self, Color, DrawMode, DrawParam, Drawable, Font, Image, Mesh, MeshBuilder, PxScale, Rect,
    Text, TextFragment, Transform,
};
use ggez::{timer, Context, GameError, GameResult};
use image::io::Reader as ImageReader;
use std::io::Cursor;
use std::sync::atomic::AtomicBool;
use std::sync::{atomic::Ordering, Arc, RwLockReadGuard};
use std::thread;
use std::time::{Duration, Instant};

const POLL_TIME: Duration = Duration::from_millis(333);
const INIT_FONT_SIZE_X: f32 = 24.0;
const INIT_FONT_SIZE_Y: f32 = 34.0;
const TEXT_X_OFFSET: f32 = 0.3;
const TEXT_OFFSET_Y_SPACING: f32 = 0.4;
const TEXT_HEIGHT_SCALE: f32 = 0.1;
const ARTIST_HEIGHT_SCALE: f32 = 0.08;
const TIMER_HEIGHT_SCALE: f32 = 0.07;
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

pub struct MPDDisplay {
    opts: Opt,
    mpd_handler: Option<MPDHandler>,
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
    filename_transform: Transform,
    artist_text: Text,
    artist_transform: Transform,
    title_text: Text,
    title_transform: Transform,
    timer_text: Text,
    timer_transform: Transform,
    timer_x: f32,
    timer_y: f32,
    timer: f64,
    length: f64,
    text_bg_mesh: Option<Mesh>,
    hide_text: bool,
    tried_album_art_in_dir: bool,
}

impl MPDDisplay {
    pub fn new(_ctx: &mut Context, opts: Opt) -> Self {
        Self {
            opts,
            mpd_handler: None,
            is_valid: true,
            is_initialized: false,
            is_authenticated: false,
            notice_text: Text::new(""),
            poll_instant: Instant::now() - POLL_TIME,
            shared: None,
            password_entered: false,
            dirty_flag: None,
            album_art: None,
            album_art_draw_transform: None,
            filename_text: Text::new(""),
            filename_transform: Transform::default(),
            artist_text: Text::new(""),
            artist_transform: Transform::default(),
            title_text: Text::new(""),
            title_transform: Transform::default(),
            timer_text: Text::new("0"),
            timer_transform: Transform::default(),
            timer_x: INIT_FONT_SIZE_X,
            timer_y: INIT_FONT_SIZE_Y,
            timer: 0.0,
            length: 0.0,
            text_bg_mesh: None,
            hide_text: false,
            tried_album_art_in_dir: false,
        }
    }

    fn init_mpd_handler(&mut self) {
        self.mpd_handler = MPDHandler::new(
            self.opts.host,
            self.opts.port,
            self.opts.password.clone().map_or(String::new(), |s| s),
            self.opts.log_level,
        )
        .ok();
        if self.mpd_handler.is_some() {
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
                debug_log::LogState::DEBUG,
                self.opts.log_level,
            );
        } else {
            self.is_valid = false;
            log(
                "Failed to initialize MPDHandler",
                debug_log::LogState::DEBUG,
                self.opts.log_level,
            );
        }
    }

    fn get_album_art_transform(&mut self, ctx: &mut Context, fill_scaled: bool) {
        if fill_scaled {
            if let Some(image) = &self.album_art {
                let screen_coords: Rect = graphics::screen_coordinates(ctx);
                let art_rect: Rect = image.dimensions();

                // try to fit to width first
                let mut x_scale = screen_coords.w / art_rect.w;
                let mut y_scale = x_scale;
                let mut new_width = art_rect.w * x_scale;
                let mut new_height = art_rect.h * y_scale;
                if new_height > screen_coords.h.abs() {
                    // fit to height instead
                    y_scale = screen_coords.h.abs() / art_rect.h;
                    x_scale = y_scale;
                    new_width = art_rect.w * x_scale;
                    new_height = art_rect.h * y_scale;
                }

                let offset_x: f32 = (screen_coords.w.abs() - new_width) / 2.0f32;
                let offset_y: f32 = (screen_coords.h.abs() - new_height) / 2.0f32;

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
            let screen_coords: Rect = graphics::screen_coordinates(ctx);
            let art_rect: Rect = image.dimensions();
            let offset_x: f32 = (screen_coords.w.abs() - art_rect.w.abs()) / 2.0f32;
            let offset_y: f32 = (screen_coords.h.abs() - art_rect.h.abs()) / 2.0f32;
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
            debug_log::LogState::DEBUG,
            self.opts.log_level,
        );

        let mut is_unknown_format: bool = false;

        match read_guard_opt.as_ref().unwrap().get_art_type().as_str() {
            "image/png" => image_format = image::ImageFormat::Png,
            "image/jpg" | "image/jpeg" => image_format = image::ImageFormat::Jpeg,
            "image/gif" => image_format = image::ImageFormat::Gif,
            _ => is_unknown_format = true,
        }

        #[allow(unused_assignments)]
        if is_unknown_format && !self.tried_album_art_in_dir {
            self.tried_album_art_in_dir = true;
            self.album_art = None;
            // Drop the "read_guard" so that the "force_try_other_album_art()"
            // can get a "write_guard"
            read_guard_opt = None;
            self.mpd_handler
                .as_ref()
                .unwrap()
                .force_try_other_album_art()
                .map_err(|_| String::from("Failed to force try other album art fetching method"))?;
            return Err("Got unknown format album art image".into());
        }

        let img = ImageReader::with_format(Cursor::new(&image_ref), image_format)
            .decode()
            .map_err(|e| format!("ERROR: Failed to decode album art image: {}", e))?;
        let rgba8 = img.to_rgba8();
        let ggez_img = Image::from_rgba8(
            ctx,
            rgba8.width() as u16,
            rgba8.height() as u16,
            rgba8.as_raw(),
        )
        .map_err(|e| format!("ERROR: Failed to load album art image in ggez Image: {}", e))?;

        self.album_art = Some(ggez_img);

        Ok(())
    }

    fn refresh_text_transforms(&mut self, ctx: &mut Context) -> GameResult<()> {
        let screen_coords: Rect = graphics::screen_coordinates(ctx);

        let text_height_limit = TEXT_HEIGHT_SCALE * screen_coords.h.abs();
        let artist_height_limit = ARTIST_HEIGHT_SCALE * screen_coords.h.abs();
        let timer_height = TIMER_HEIGHT_SCALE * screen_coords.h.abs();

        let mut offset_y: f32 = screen_coords.h;

        let mut filename_y: f32 = 0.0;
        let mut artist_y: f32 = 0.0;
        let mut title_y: f32 = 0.0;
        let mut timer_y: f32 = 0.0;

        let set_transform = |text: &mut Text,
                             transform: &mut Transform,
                             offset_y: &mut f32,
                             y: &mut f32,
                             is_string: bool,
                             is_artist: bool,
                             timer_x: &mut f32,
                             timer_y: &mut f32| {
            let mut current_x = INIT_FONT_SIZE_X;
            let mut current_y = INIT_FONT_SIZE_Y;
            let mut width: f32;
            let mut height: f32 = 0.0;
            let mut iteration_count: u8 = 0;
            loop {
                iteration_count += 1;
                if iteration_count > 8 {
                    break;
                }

                text.set_font(
                    Font::default(),
                    PxScale {
                        x: current_x,
                        y: current_y,
                    },
                );
                width = text.width(ctx);
                height = text.height(ctx);

                if is_string {
                    if screen_coords.w < width
                        || height
                            >= (if is_artist {
                                artist_height_limit
                            } else {
                                text_height_limit
                            })
                    {
                        current_x = current_x * DECREASE_AMT;
                        current_y = current_y * DECREASE_AMT;
                        continue;
                    } else if screen_coords.w * MIN_WIDTH_RATIO > width {
                        current_x = current_x * INCREASE_AMT;
                        current_y = current_y * INCREASE_AMT;
                        continue;
                    } else {
                        break;
                    }
                } else {
                    let diff_scale_y = current_y / height * timer_height;
                    let current_x = current_x * diff_scale_y / current_y;
                    text.set_font(
                        Font::default(),
                        PxScale {
                            x: current_x,
                            y: diff_scale_y,
                        },
                    );
                    *timer_x = current_x;
                    *timer_y = diff_scale_y;
                    // width = text.width(ctx); // not really used after this
                    height = text.height(ctx);
                    break;
                }
            }

            *y = *offset_y - height;
            *transform = Transform::Values {
                dest: [TEXT_X_OFFSET, *offset_y - height].into(),
                rotation: 0.0,
                scale: [1.0, 1.0].into(),
                offset: [0.0, 0.0].into(),
            };

            *offset_y -= height + TEXT_OFFSET_Y_SPACING;
        };

        if !self.filename_text.contents().is_empty() && !self.opts.disable_show_filename {
            set_transform(
                &mut self.filename_text,
                &mut self.filename_transform,
                &mut offset_y,
                &mut filename_y,
                true,
                false,
                &mut self.timer_x,
                &mut self.timer_y,
            );
        } else {
            log(
                "filename text is empty",
                debug_log::LogState::WARNING,
                self.opts.log_level,
            );
        }

        if !self.artist_text.contents().is_empty() && !self.opts.disable_show_artist {
            set_transform(
                &mut self.artist_text,
                &mut self.artist_transform,
                &mut offset_y,
                &mut artist_y,
                true,
                true,
                &mut self.timer_x,
                &mut self.timer_y,
            );
        } else {
            log(
                "artist text is empty",
                debug_log::LogState::WARNING,
                self.opts.log_level,
            );
        }

        if !self.title_text.contents().is_empty() && !self.opts.disable_show_title {
            set_transform(
                &mut self.title_text,
                &mut self.title_transform,
                &mut offset_y,
                &mut title_y,
                true,
                false,
                &mut self.timer_x,
                &mut self.timer_y,
            );
        } else {
            log(
                "title text is empty",
                debug_log::LogState::WARNING,
                self.opts.log_level,
            );
        }

        set_transform(
            &mut self.timer_text,
            &mut self.timer_transform,
            &mut offset_y,
            &mut timer_y,
            false,
            false,
            &mut self.timer_x,
            &mut self.timer_y,
        );

        let filename_dimensions = self.filename_text.dimensions(ctx);
        let artist_dimensions = self.artist_text.dimensions(ctx);
        let title_dimensions = self.title_text.dimensions(ctx);
        let timer_dimensions = self.timer_text.dimensions(ctx);

        let mut mesh_builder: MeshBuilder = MeshBuilder::new();
        if !self.opts.disable_show_filename {
            mesh_builder.rectangle(
                DrawMode::fill(),
                Rect {
                    x: TEXT_X_OFFSET,
                    y: filename_y,
                    w: filename_dimensions.w,
                    h: filename_dimensions.h,
                },
                Color::from_rgba(0, 0, 0, 160),
            )?;
        }
        if !self.opts.disable_show_artist {
            mesh_builder.rectangle(
                DrawMode::fill(),
                Rect {
                    x: TEXT_X_OFFSET,
                    y: artist_y,
                    w: artist_dimensions.w,
                    h: artist_dimensions.h,
                },
                Color::from_rgba(0, 0, 0, 160),
            )?;
        }
        if !self.opts.disable_show_title {
            mesh_builder.rectangle(
                DrawMode::fill(),
                Rect {
                    x: TEXT_X_OFFSET,
                    y: title_y,
                    w: title_dimensions.w,
                    h: title_dimensions.h,
                },
                Color::from_rgba(0, 0, 0, 160),
            )?;
        }
        let mesh: Mesh = mesh_builder
            .rectangle(
                DrawMode::fill(),
                Rect {
                    x: TEXT_X_OFFSET,
                    y: timer_y,
                    w: timer_dimensions.w,
                    h: timer_dimensions.h,
                },
                Color::from_rgba(0, 0, 0, 160),
            )?
            .build(ctx)?;

        self.text_bg_mesh = Some(mesh);

        Ok(())
    }
}

impl EventHandler for MPDDisplay {
    fn update(&mut self, ctx: &mut ggez::Context) -> Result<(), GameError> {
        if !self.is_valid {
            return Err(GameError::EventLoopError(
                "Failed to initialize MPDHandler".into(),
            ));
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

        if self.is_valid && self.is_initialized && self.poll_instant.elapsed() > POLL_TIME {
            self.poll_instant = Instant::now();
            if self.dirty_flag.is_some()
                && self
                    .dirty_flag
                    .as_ref()
                    .unwrap()
                    .swap(false, Ordering::Relaxed)
            {
                log(
                    "dirty_flag cleared, acquiring shared data...",
                    debug_log::LogState::DEBUG,
                    self.opts.log_level,
                );
                self.shared = self
                    .mpd_handler
                    .as_ref()
                    .unwrap()
                    .get_current_song_info()
                    .ok();
                if let Some(shared) = &self.shared {
                    if self.notice_text.contents() != shared.error_text {
                        self.notice_text = Text::new(TextFragment::new(shared.error_text.clone()));
                    }
                    if !shared.title.is_empty() {
                        self.title_text = Text::new(shared.title.clone());
                    } else {
                        self.dirty_flag
                            .as_ref()
                            .unwrap()
                            .store(true, Ordering::Relaxed);
                    }
                    if !shared.artist.is_empty() {
                        self.artist_text = Text::new(shared.artist.clone());
                    } else {
                        self.dirty_flag
                            .as_ref()
                            .unwrap()
                            .store(true, Ordering::Relaxed);
                    }
                    if !shared.filename.is_empty() {
                        if self.filename_text.contents() != shared.filename {
                            self.album_art = None;
                            self.tried_album_art_in_dir = false;
                        }
                        self.filename_text = Text::new(shared.filename.clone());
                    } else {
                        self.dirty_flag
                            .as_ref()
                            .unwrap()
                            .store(true, Ordering::Relaxed);
                    }
                    self.timer = shared.pos;
                    self.length = shared.length;
                    self.refresh_text_transforms(ctx)?;
                } else {
                    log(
                        "Failed to acquire read lock for getting shared data",
                        debug_log::LogState::DEBUG,
                        self.opts.log_level,
                    );
                }
                if self.album_art.is_none() {
                    let result = self.get_image_from_data(ctx);
                    if let Err(e) = result {
                        log(e, debug_log::LogState::WARNING, self.opts.log_level);
                        self.album_art = None;
                        self.album_art_draw_transform = None;
                    } else {
                        self.get_album_art_transform(ctx, !self.opts.do_not_fill_scale_album_art);
                    }
                }
            }
        }

        let delta = timer::delta(ctx);
        self.timer += delta.as_secs_f64();
        let timer_diff = seconds_to_time(self.length - self.timer);
        self.timer_text = Text::new(timer_diff);
        self.timer_text.set_font(
            Font::default(),
            PxScale {
                x: self.timer_x,
                y: self.timer_y,
            },
        );

        Ok(())
    }

    fn draw(&mut self, ctx: &mut ggez::Context) -> Result<(), GameError> {
        graphics::clear(ctx, Color::BLACK);

        if self.album_art.is_some() && self.album_art_draw_transform.is_some() {
            self.album_art.as_ref().unwrap().draw(
                ctx,
                DrawParam {
                    trans: self.album_art_draw_transform.unwrap(),
                    ..Default::default()
                },
            )?;
        }

        if !self.hide_text {
            self.notice_text.draw(ctx, DrawParam::default())?;

            if self.is_valid && self.is_initialized {
                if let Some(mesh) = &self.text_bg_mesh {
                    mesh.draw(ctx, DrawParam::default())?;
                }

                if !self.opts.disable_show_filename {
                    self.filename_text.draw(
                        ctx,
                        DrawParam {
                            trans: self.filename_transform,
                            ..Default::default()
                        },
                    )?;
                }

                if !self.opts.disable_show_artist {
                    self.artist_text.draw(
                        ctx,
                        DrawParam {
                            trans: self.artist_transform,
                            ..Default::default()
                        },
                    )?;
                }

                if !self.opts.disable_show_title {
                    self.title_text.draw(
                        ctx,
                        DrawParam {
                            trans: self.title_transform,
                            ..Default::default()
                        },
                    )?;
                }

                self.timer_text.draw(
                    ctx,
                    DrawParam {
                        trans: self.timer_transform,
                        ..Default::default()
                    },
                )?;
            }
        }

        graphics::present(ctx)
    }

    fn text_input_event(&mut self, _ctx: &mut Context, character: char) {
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
    }

    fn key_down_event(
        &mut self,
        _ctx: &mut Context,
        keycode: event::KeyCode,
        _keymods: event::KeyMods,
        _repeat: bool,
    ) {
        if !self.is_initialized && self.opts.enable_prompt_password {
            if keycode == event::KeyCode::Back {
                let s: String = self.notice_text.contents();

                if s.ends_with('*') {
                    self.notice_text = Text::new(TextFragment::new(s[0..(s.len() - 1)].to_owned()));
                }

                if let Some(input_p) = &mut self.opts.password {
                    input_p.pop();
                }
            } else if keycode == event::KeyCode::Return {
                self.password_entered = true;
            }
        } else if keycode == event::KeyCode::H {
            self.hide_text = true;
        }
    }

    fn key_up_event(
        &mut self,
        _ctx: &mut Context,
        keycode: event::KeyCode,
        _keymods: event::KeyMods,
    ) {
        if keycode == event::KeyCode::H {
            self.hide_text = false;
        }
    }

    fn resize_event(&mut self, ctx: &mut Context, _width: f32, _height: f32) {
        self.get_album_art_transform(ctx, !self.opts.do_not_fill_scale_album_art);
        self.refresh_text_transforms(ctx)
            .expect("Failed to set text transforms");
    }
}
