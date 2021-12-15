use crate::debug_log::log;
use crate::mpd_handler::{InfoFromShared, MPDHandler};
use crate::Opt;
use ggez::event::{self, EventHandler};
use ggez::graphics::{
    self, Color, DrawParam, Drawable, Image, Rect, Text, TextFragment, Transform,
};
use ggez::Context;
use ggez::GameError;
use image::io::Reader as ImageReader;
use image::GenericImageView;
use std::io::Cursor;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::{Duration, Instant};

const POLL_TIME: Duration = Duration::from_millis(333);

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
    artist_text: Text,
    title_text: Text,
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
            artist_text: Text::new(""),
            title_text: Text::new(""),
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
                    .swap(false, std::sync::atomic::Ordering::Relaxed)
            {
                self.shared = MPDHandler::get_current_song_info(self.mpd_handler.clone().unwrap())
                    .map_or(None, |f| Some(f));
                if let Some(shared) = &self.shared {
                    if self.notice_text.contents() != shared.error_text {
                        self.notice_text = Text::new(TextFragment::new(shared.error_text.clone()));
                        self.title_text = Text::new(TextFragment::new(shared.title.clone()));
                        self.artist_text = Text::new(TextFragment::new(shared.artist.clone()));
                        self.filename_text = Text::new(TextFragment::new(shared.filename.clone()));
                    }
                }
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

        graphics::present(ctx)
    }

    fn text_input_event(&mut self, _ctx: &mut Context, character: char) {
        if !self.is_initialized
            && self.opts.enable_prompt_password
            && character.is_ascii()
            && !character.is_ascii_control()
        {
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
    }
}
