use crate::debug_log::log;
use crate::mpd_handler::{InfoFromShared, MPDHandler};
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
use std::sync::{atomic::Ordering, Arc, RwLock};
use std::thread;
use std::time::{Duration, Instant};

const POLL_TIME: Duration = Duration::from_millis(333);
const INIT_FONT_SIZE_X: f32 = 24.0;
const INIT_FONT_SIZE_Y: f32 = 34.0;
const TEXT_X_OFFSET: f32 = 0.3;
const TEXT_OFFSET_Y_SPACING: f32 = 0.4;
const TEXT_HEIGHT_LIMIT: f32 = 55.0;
const ARTIST_HEIGHT_LIMIT: f32 = 40.0;

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
    mpd_handler: Option<Arc<RwLock<MPDHandler>>>,
    is_valid: bool,
    is_initialized: bool,
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
    timer: f64,
    length: f64,
    text_bg_mesh: Option<Mesh>,
}

impl MPDDisplay {
    pub fn new(_ctx: &mut Context, opts: Opt) -> Self {
        Self {
            opts,
            mpd_handler: None,
            is_valid: true,
            is_initialized: false,
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
            timer: 0.0,
            length: 0.0,
            text_bg_mesh: None,
        }
    }

    fn init_mpd_handler(&mut self) -> () {
        self.mpd_handler = MPDHandler::new(
            self.opts.host,
            self.opts.port,
            self.opts.password.clone().map_or(String::new(), |s| s),
        )
        .map_or_else(|_| None, |v| Some(v));
        if self.mpd_handler.is_some() {
            self.is_initialized = true;
            loop {
                self.dirty_flag =
                    MPDHandler::get_dirty_flag(self.mpd_handler.as_ref().unwrap().clone())
                        .map_or(None, |f| Some(f));
                if self.dirty_flag.is_some() {
                    break;
                } else {
                    thread::sleep(POLL_TIME);
                }
            }
            log("Successfully initialized MPDHandler");
        } else {
            self.is_valid = false;
            log("Failed to initialize MPDHandler");
        }
    }

    fn get_album_art_transform(&mut self, ctx: &mut Context, fill: bool) -> () {
        if fill {
            unimplemented!("filled image not implemented");
        } else {
            if let Some(image) = &self.album_art {
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
    }

    fn get_image_from_data(
        &mut self,
        ctx: &mut Context,
        data: (Vec<u8>, String),
    ) -> Result<(), String> {
        let mut image_format: image::ImageFormat = image::ImageFormat::Png;
        match data.1.as_str() {
            "image/png" => image_format = image::ImageFormat::Png,
            "image/jpg" | "image/jpeg" => image_format = image::ImageFormat::Jpeg,
            "image/gif" => image_format = image::ImageFormat::Gif,
            _ => (),
        }
        let img = ImageReader::with_format(Cursor::new(data.0), image_format)
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
                             is_artist: bool| {
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
                                ARTIST_HEIGHT_LIMIT
                            } else {
                                TEXT_HEIGHT_LIMIT
                            })
                    {
                        current_x = current_x * 4.0f32 / 5.0f32;
                        current_y = current_y * 4.0f32 / 5.0f32;
                        continue;
                    } else if screen_coords.w * 2.0 / 3.0 > width {
                        current_x = current_x * 5.0f32 / 4.0f32;
                        current_y = current_y * 5.0f32 / 4.0f32;
                        continue;
                    } else {
                        break;
                    }
                } else {
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

        if !self.filename_text.contents().is_empty() {
            set_transform(
                &mut self.filename_text,
                &mut self.filename_transform,
                &mut offset_y,
                &mut filename_y,
                true,
                false,
            );
        } else {
            log("filename text is empty");
        }

        if !self.artist_text.contents().is_empty() {
            set_transform(
                &mut self.artist_text,
                &mut self.artist_transform,
                &mut offset_y,
                &mut artist_y,
                true,
                true,
            );
        } else {
            log("artist text is empty");
        }

        if !self.title_text.contents().is_empty() {
            set_transform(
                &mut self.title_text,
                &mut self.title_transform,
                &mut offset_y,
                &mut title_y,
                true,
                false,
            );
        } else {
            log("title text is empty");
        }

        set_transform(
            &mut self.timer_text,
            &mut self.timer_transform,
            &mut offset_y,
            &mut timer_y,
            false,
            false,
        );

        let filename_dimensions = self.filename_text.dimensions(ctx);
        let artist_dimensions = self.artist_text.dimensions(ctx);
        let title_dimensions = self.title_text.dimensions(ctx);
        let timer_dimensions = self.timer_text.dimensions(ctx);

        let mesh: Mesh = MeshBuilder::new()
            .rectangle(
                DrawMode::fill(),
                Rect {
                    x: TEXT_X_OFFSET,
                    y: filename_y,
                    w: filename_dimensions.w,
                    h: filename_dimensions.h,
                },
                Color::from_rgba(0, 0, 0, 160),
            )?
            .rectangle(
                DrawMode::fill(),
                Rect {
                    x: TEXT_X_OFFSET,
                    y: artist_y,
                    w: artist_dimensions.w,
                    h: artist_dimensions.h,
                },
                Color::from_rgba(0, 0, 0, 160),
            )?
            .rectangle(
                DrawMode::fill(),
                Rect {
                    x: TEXT_X_OFFSET,
                    y: title_y,
                    w: title_dimensions.w,
                    h: title_dimensions.h,
                },
                Color::from_rgba(0, 0, 0, 160),
            )?
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
        }

        if self.is_valid && self.is_initialized && self.poll_instant.elapsed() > POLL_TIME {
            self.poll_instant = Instant::now();
            if !self.dirty_flag.is_none()
                && self
                    .dirty_flag
                    .as_ref()
                    .unwrap()
                    .swap(false, Ordering::Relaxed)
            {
                log("dirty_flag cleared, acquiring shared data...");
                self.shared = MPDHandler::get_current_song_info(self.mpd_handler.clone().unwrap())
                    .map_or(None, |f| Some(f));
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
                    log("Failed to acquire read lock for getting shared data");
                }
                if self.album_art.is_none() {
                    let album_art_data_result =
                        MPDHandler::get_art_data(self.mpd_handler.clone().unwrap());
                    if let Ok(art_data) = album_art_data_result {
                        let result = self.get_image_from_data(ctx, art_data);
                        if let Err(e) = result {
                            log(e);
                            self.album_art = None;
                            self.album_art_draw_transform = None;
                        } else {
                            self.get_album_art_transform(ctx, false);
                        }
                    } else {
                        self.album_art = None;
                        self.album_art_draw_transform = None;
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
                x: INIT_FONT_SIZE_X,
                y: INIT_FONT_SIZE_Y,
            },
        );

        Ok(())
    }

    fn draw(&mut self, ctx: &mut ggez::Context) -> Result<(), GameError> {
        graphics::clear(ctx, Color::BLACK);

        if self.album_art.is_some() && self.album_art_draw_transform.is_some() {
            log("Drawing album_art");
            self.album_art.as_ref().unwrap().draw(
                ctx,
                DrawParam {
                    trans: self.album_art_draw_transform.unwrap(),
                    ..Default::default()
                },
            )?;
        }

        self.notice_text.draw(ctx, DrawParam::default())?;

        if self.is_valid && self.is_initialized {
            if let Some(mesh) = &self.text_bg_mesh {
                mesh.draw(ctx, DrawParam::default())?;
            }

            self.filename_text.draw(
                ctx,
                DrawParam {
                    trans: self.filename_transform,
                    ..Default::default()
                },
            )?;

            self.artist_text.draw(
                ctx,
                DrawParam {
                    trans: self.artist_transform,
                    ..Default::default()
                },
            )?;

            self.title_text.draw(
                ctx,
                DrawParam {
                    trans: self.title_transform,
                    ..Default::default()
                },
            )?;

            self.timer_text.draw(
                ctx,
                DrawParam {
                    trans: self.timer_transform,
                    ..Default::default()
                },
            )?;
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

                if s.ends_with("*") {
                    self.notice_text = Text::new(TextFragment::new(s[0..(s.len() - 1)].to_owned()));
                }

                if let Some(input_p) = &mut self.opts.password {
                    input_p.pop();
                }
            } else if keycode == event::KeyCode::Return {
                self.password_entered = true;
                //log(format!("Entered \"{}\"", self.opts.password.as_ref().unwrap_or(&String::new())));
            }
        }
    }

    fn resize_event(&mut self, ctx: &mut Context, _width: f32, _height: f32) {
        self.get_album_art_transform(ctx, false);
        self.refresh_text_transforms(ctx)
            .expect("Failed to set text transforms");
    }
}
