mod debug_log;
mod display;
mod mpd_handler;
mod signal;
#[cfg(feature = "unicode_support")]
mod unicode_support;

use clap::Parser;
use ggez::conf::{WindowMode, WindowSetup};
use ggez::event;
use ggez::{ContextBuilder, GameResult};
use std::fs::File;
use std::io::Read;
use std::net::Ipv4Addr;
use std::path::PathBuf;

#[derive(Parser, Debug, Clone)]
#[command(author, version, about, long_about = None)]
pub struct Opt {
    host: Ipv4Addr,
    #[arg(default_value = "6600")]
    port: u16,
    #[arg(short = 'p')]
    password: Option<String>,
    #[arg(long = "disable-show-title", help = "disable title display")]
    disable_show_title: bool,
    #[arg(long = "disable-show-artist", help = "disable artist display")]
    disable_show_artist: bool,
    #[arg(long = "disable-show-album", help = "disable album display")]
    disable_show_album: bool,
    #[arg(long = "disable-show-filename", help = "disable filename display")]
    disable_show_filename: bool,
    #[arg(long = "disable-show-percentage", help = "disable percentage display")]
    disable_show_percentage: bool,
    #[arg(
        long = "force-text-height-scale",
        help = "force-set text height relative to window height as a ratio (default 0.12)"
    )]
    force_text_height_scale: Option<f32>,
    #[arg(long = "pprompt", help = "input password via prompt")]
    enable_prompt_password: bool,
    #[arg(long = "pfile", help = "read password from file")]
    password_file: Option<PathBuf>,
    #[arg(
        long = "no-scale-fill",
        help = "don't scale-fill the album art to the window"
    )]
    do_not_fill_scale_album_art: bool,
    #[arg(short = 'l', long = "log-level", default_value = "error")]
    log_level: debug_log::LogLevel,
    #[arg(
        short,
        long,
        help = "sets the opacity of the text background (0-255)",
        default_value = "190"
    )]
    text_bg_opacity: u8,
}

fn main() -> GameResult<()> {
    let mut opt = Opt::parse();
    if let Some(forced_scale) = &mut opt.force_text_height_scale {
        if *forced_scale < 0.01 {
            *forced_scale = 0.01;
            println!("WARNING: Clamped \"force-text-height-scale\" to minimum of 0.01!");
        } else if *forced_scale > 0.5 {
            *forced_scale = 0.5;
            println!("WARNING: Clamped \"force-text-height-scale\" to maximum of 0.5!");
        }
    }
    println!("Got host addr == {}, port == {}", opt.host, opt.port);

    // Read password from file if exists, error otherwise.
    if let Some(psswd_file_path) = opt.password_file.as_ref() {
        let mut file = File::open(psswd_file_path).expect("pfile/password_file should exist");
        let mut content: String = String::new();

        file.read_to_string(&mut content)
            .expect("Should be able to read from pfile/password_file");

        if content.ends_with("\r\n") {
            content.truncate(content.len() - 2);
        } else if content.ends_with('\n') {
            content.truncate(content.len() - 1);
        }

        opt.password = Some(content);
    }

    // Set up signal handlers to request graceful shutdown.
    #[cfg(target_family = "unix")]
    {
        signal::register_signal(libc::SIGHUP).unwrap();
        signal::register_signal(libc::SIGINT).unwrap();
        signal::register_signal(libc::SIGTERM).unwrap();
    }
    #[cfg(target_family = "windows")]
    {
        signal::register_ctrl_handler().unwrap();
    }

    let (mut ctx, event_loop) = ContextBuilder::new("mpd_info_screen", "Stephen Seo")
        .window_setup(WindowSetup {
            title: "mpd info screen".into(),
            ..Default::default()
        })
        .window_mode(WindowMode {
            resizable: true,
            resize_on_scale_factor_change: true,
            ..Default::default()
        })
        .build()
        .expect("Failed to create ggez context");

    // mount "/" read-only so that fonts can be loaded via absolute paths
    ctx.fs.mount(&PathBuf::from("/"), true);

    let display = display::MPDDisplay::new(&mut ctx, opt.clone());

    event::run(ctx, event_loop, display)
}
