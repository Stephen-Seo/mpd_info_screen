use crate::mpd_handler::MPDHandler;
use crate::Opt;
use ggez::event::{self, EventHandler};
use ggez::graphics::{self, Color, DrawParam, Drawable, Rect, Text, TextFragment};
use ggez::timer::{check_update_time, fps, yield_now};
use ggez::Context;
use ggez::GameError;
use std::sync::{Arc, RwLock};

pub struct MPDDisplay {
    opts: Opt,
    mpd_handler: Option<Arc<RwLock<MPDHandler>>>,
    is_valid: bool,
    is_initialized: bool,
    notice_text: Text,
}

impl MPDDisplay {
    pub fn new(ctx: &mut Context, opts: Opt) -> Self {
        Self {
            opts,
            mpd_handler: None,
            is_valid: true,
            is_initialized: false,
            notice_text: Text::new(""),
        }
    }
}

impl EventHandler for MPDDisplay {
    fn update(&mut self, ctx: &mut ggez::Context) -> Result<(), GameError> {
        if !check_update_time(ctx, 10) {
            yield_now();
            return Ok(());
        }

        self.notice_text = Text::new(TextFragment::new(format!("fps is {}", fps(ctx))));

        yield_now();
        Ok(())
    }

    fn draw(&mut self, ctx: &mut ggez::Context) -> Result<(), GameError> {
        graphics::clear(ctx, Color::BLACK);

        self.notice_text.draw(ctx, DrawParam::default());

        graphics::present(ctx)
    }

    fn resize_event(&mut self, ctx: &mut Context, width: f32, height: f32) {
        graphics::set_screen_coordinates(
            ctx,
            Rect {
                x: 0.0,
                y: 0.0,
                w: width,
                h: height,
            },
        )
        .expect("Failed to handle resizing window");
    }
}
