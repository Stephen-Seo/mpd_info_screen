mod debug_log;
mod display;
mod mpd_handler;
#[cfg(feature = "unicode_support")]
mod unicode_support;

use ggez::conf::{WindowMode, WindowSetup};
use ggez::event::winit_event::{ElementState, KeyboardInput, ModifiersState};
use ggez::event::{self, ControlFlow, EventHandler};
use ggez::input::keyboard::{self, KeyInput};
use ggez::{ContextBuilder, GameError};
use std::net::Ipv4Addr;
use std::path::PathBuf;
use std::thread;
use std::time::{Duration, Instant};
use structopt::StructOpt;

use debug_log::log;

#[derive(StructOpt, Debug, Clone)]
#[structopt(name = "mpd_info_screen")]
pub struct Opt {
    host: Ipv4Addr,
    #[structopt(default_value = "6600")]
    port: u16,
    #[structopt(short = "p")]
    password: Option<String>,
    #[structopt(long = "disable-show-title", help = "disable title display")]
    disable_show_title: bool,
    #[structopt(long = "disable-show-artist", help = "disable artist display")]
    disable_show_artist: bool,
    #[structopt(long = "disable-show-album", help = "disable album display")]
    disable_show_album: bool,
    #[structopt(long = "disable-show-filename", help = "disable filename display")]
    disable_show_filename: bool,
    #[structopt(long = "pprompt", help = "input password via prompt")]
    enable_prompt_password: bool,
    #[structopt(
        long = "no-scale-fill",
        help = "don't scale-fill the album art to the window"
    )]
    do_not_fill_scale_album_art: bool,
    #[structopt(
        short = "l",
        long = "log-level",
        possible_values = &debug_log::LogLevel::variants(),
        default_value = "Error",
        case_insensitive = true,
    )]
    log_level: debug_log::LogLevel,
    #[structopt(
        short,
        long,
        help = "sets the opacity of the text background (0-255)",
        default_value = "190"
    )]
    text_bg_opacity: u8,
}

fn main() -> Result<(), String> {
    let opt = Opt::from_args();
    println!("Got host addr == {}, port == {}", opt.host, opt.port);

    let (mut ctx, event_loop) = ContextBuilder::new("mpd_info_screen", "Stephen Seo")
        .window_setup(WindowSetup {
            title: "mpd info screen".into(),
            ..Default::default()
        })
        .window_mode(WindowMode {
            resizable: true,
            ..Default::default()
        })
        .build()
        .expect("Failed to create ggez context");

    // mount "/" read-only so that fonts can be loaded via absolute paths
    ctx.fs.mount(&PathBuf::from("/"), true);

    let mut display = display::MPDDisplay::new(&mut ctx, opt.clone());

    let mut modifiers_state: ModifiersState = ModifiersState::default();

    event_loop.run(move |mut event, _window_target, control_flow| {
        if !ctx.continuing {
            *control_flow = ControlFlow::Exit;
            return;
        }

        *control_flow = ControlFlow::WaitUntil(Instant::now() + Duration::from_millis(100));

        let ctx = &mut ctx;

        event::process_event(ctx, &mut event);
        match event {
            event::winit_event::Event::WindowEvent { event, .. } => match event {
                event::winit_event::WindowEvent::CloseRequested => ctx.request_quit(),
                event::winit_event::WindowEvent::ModifiersChanged(state) => {
                    modifiers_state = state;
                }
                event::winit_event::WindowEvent::KeyboardInput {
                    device_id: _,
                    input:
                        KeyboardInput {
                            virtual_keycode: Some(keycode),
                            state,
                            ..
                        },
                    is_synthetic: _,
                } => {
                    if keycode == keyboard::KeyCode::Escape {
                        *control_flow = ControlFlow::Exit;
                        return;
                    }
                    let ki = KeyInput {
                        scancode: 0,
                        keycode: Some(keycode),
                        mods: From::from(modifiers_state),
                    };
                    if state == ElementState::Pressed {
                        display.key_down_event(ctx, ki, false).ok();
                    } else {
                        display.key_up_event(ctx, ki).ok();
                    }
                }
                event::winit_event::WindowEvent::Resized(phys_size) => {
                    display
                        .resize_event(ctx, phys_size.width as f32, phys_size.height as f32)
                        .ok();
                }
                event::winit_event::WindowEvent::ReceivedCharacter(ch) => {
                    display.text_input_event(ctx, ch).ok();
                }
                x => log(
                    format!("Other window event fired: {:?}", x),
                    debug_log::LogState::Verbose,
                    opt.log_level,
                ),
            },
            event::winit_event::Event::MainEventsCleared => {
                ctx.time.tick();

                let mut game_result: Result<(), GameError> = display.update(ctx);
                if game_result.is_err() {
                    println!("Error update: {}", game_result.unwrap_err());
                    *control_flow = ControlFlow::Exit;
                    return;
                }

                ctx.gfx.begin_frame().unwrap();
                game_result = display.draw(ctx);
                if game_result.is_err() {
                    println!("Error draw: {}", game_result.unwrap_err());
                    *control_flow = ControlFlow::Exit;
                    return;
                }
                ctx.gfx.end_frame().unwrap();

                ctx.mouse.reset_delta();

                // sleep to force ~5 fps
                thread::sleep(Duration::from_millis(200));
                ggez::timer::yield_now();
            }
            x => log(
                format!("Device event fired: {:?}", x),
                debug_log::LogState::Verbose,
                opt.log_level,
            ),
        }
    });
}
