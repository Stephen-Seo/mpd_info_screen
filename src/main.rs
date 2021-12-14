mod display;
mod mpd_handler;

use ggez::conf::{WindowMode, WindowSetup};
use ggez::event::winit_event::KeyboardInput;
use ggez::event::{self, ControlFlow, EventHandler};
use ggez::graphics::{self, Rect};
use ggez::ContextBuilder;
use std::net::Ipv4Addr;
use std::thread;
use std::time::{Duration, Instant};
use structopt::StructOpt;

#[derive(StructOpt, Debug)]
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
    #[structopt(long = "disable-show-filename", help = "disable filename display")]
    disable_show_filename: bool,
    #[structopt(long = "pprompt", help = "input password via prompt")]
    enable_prompt_password: bool,
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

    let mut display = display::MPDDisplay::new(&mut ctx, opt);

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
                event::winit_event::WindowEvent::CloseRequested => event::quit(ctx),
                event::winit_event::WindowEvent::KeyboardInput {
                    device_id: _,
                    input:
                        KeyboardInput {
                            virtual_keycode: Some(keycode),
                            ..
                        },
                    is_synthetic: _,
                } => match keycode {
                    event::KeyCode::Escape | event::KeyCode::Q => {
                        *control_flow = ControlFlow::Exit;
                    }
                    _ => (),
                },
                event::winit_event::WindowEvent::Resized(phys_size) => {
                    graphics::set_screen_coordinates(
                        ctx,
                        Rect {
                            x: 0.0,
                            y: 0.0,
                            w: phys_size.width as f32,
                            h: phys_size.height as f32,
                        },
                    )
                    .expect("Failed to handle resizing window");
                }
                x => println!("Other window event fired: {:?}", x),
            },
            event::winit_event::Event::MainEventsCleared => {
                ctx.timer_context.tick();

                display.update(ctx).expect("Update failed");
                display.draw(ctx).expect("Draw failed");

                ctx.mouse_context.reset_delta();

                // sleep to force 10-11 fps
                thread::sleep(Duration::from_millis(90));
                ggez::timer::yield_now();
            }
            x => println!("Device event fired: {:?}", x),
        }
    });
}
