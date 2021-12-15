use crate::debug_log::log;
use crate::mpd_handler::{InfoFromShared, MPDHandler};
use crate::Opt;
use ggez::event::{self, EventHandler};
use ggez::graphics::{self, Color, DrawParam, Drawable, Text, TextFragment};
use ggez::Context;
use ggez::GameError;
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
}

impl EventHandler for MPDDisplay {
    fn update(&mut self, _ctx: &mut ggez::Context) -> Result<(), GameError> {
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
                    }
                }
            }
        }

        Ok(())
    }

    fn draw(&mut self, ctx: &mut ggez::Context) -> Result<(), GameError> {
        graphics::clear(ctx, Color::BLACK);

        self.notice_text.draw(ctx, DrawParam::default())?;

        graphics::present(ctx)
    }

    fn text_input_event(&mut self, _ctx: &mut Context, character: char) {
        if !self.is_initialized && self.opts.enable_prompt_password {
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
            } else if keycode == event::KeyCode::Return {
                self.password_entered = true;
            }
        }
    }
}
